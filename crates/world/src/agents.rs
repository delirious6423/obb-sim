//! Fixed-behaviour agents for the roles the league needs filled: a general
//! manager and an owner per club. They see only what their role may see:
//! a club's own books and roster, the public market, and noisy scouting
//! reports. They never read a player's true talent.

use serde::{Deserialize, Serialize};

use crate::command::Command;
use crate::player::{Level, Origin, PlayerId, slot_bonus};
use crate::rating::Kind;
use crate::scout::{asking_salary, asking_years, opinion, war_of};
use crate::state::{CREDIT, MAJORS_PER_KIND, MIN_SALARY, Phase, ROSTER_MAX_PER_KIND, World};

/// Bonus ceiling relative to the slot at which a pick will still sign.
const SIGNABLE: f64 = 1.5;
/// Quality gain (runs per PA) a free agent must offer over the club's
/// thirteenth man of the same kind before the GM bothers.
const UPGRADE_MARGIN: f64 = 0.0015;

/// Longest contract a club may sign.
pub const MAX_YEARS: u8 = 7;

/// A player as one club sees him.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Seen {
    pub id: PlayerId,
    pub kind: Kind,
    pub age: i32,
    pub level: Level,
    pub origin: Origin,
    /// This club's noisy opinion of current and peak quality.
    pub now: f64,
    pub peak: f64,
    /// What the market asks per year, and for how long.
    pub ask: f64,
    pub ask_years: u8,
    /// Asking signing bonus (amateurs).
    pub demand: f64,
    /// Salary still owed if released (own players only; zero otherwise).
    pub owed: f64,
    /// Contract terms, public for any signed player (zero if none).
    pub salary: f64,
    pub years: u8,
}

impl Seen {
    /// What the club ranks him by for the roster: how good he is now, with a
    /// little credit for young players still growing.
    #[must_use]
    pub fn score(&self) -> f64 {
        if self.age <= 24 {
            0.85 * self.now + 0.15 * self.peak
        } else {
            self.now
        }
    }

    /// What the club ranks an amateur by in the draft: ceiling matters.
    #[must_use]
    pub fn draft_score(&self) -> f64 {
        0.5 * self.now + 0.5 * self.peak
    }

    /// Years worth signing him for. The market asks the same salary whatever
    /// the length, so a player still improving is worth locking in for as long
    /// as possible, and an older one only for what the market asks.
    #[must_use]
    pub fn sign_years(&self) -> u8 {
        match self.age {
            ..=25 => MAX_YEARS,
            26..=27 => 5.max(self.ask_years),
            _ => self.ask_years,
        }
    }

    /// Value to the club per year minus the price, in millions.
    #[must_use]
    pub fn surplus(&self, war_price: f64) -> f64 {
        war_price * war_of(self.kind, self.score()).max(0.0) - self.ask
    }
}

fn by_score_desc(a: &Seen, b: &Seen) -> std::cmp::Ordering {
    b.score()
        .partial_cmp(&a.score())
        .unwrap_or(std::cmp::Ordering::Equal)
        .then(a.id.cmp(&b.id))
}

/// The pick on the clock for a club.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DraftObs {
    /// Index of the pick in the draft order.
    pub pick: usize,
    /// Largest signing bonus the pick will sign for.
    pub ceiling: f64,
    /// Whether this is an international round.
    pub international: bool,
    /// Prospects still available, as this club sees them.
    pub prospects: Vec<Seen>,
}

/// A player on another club, as this club sees him.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Held {
    pub club: u8,
    pub seen: Seen,
}

/// A trade offer put to this club.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IncomingOffer {
    pub id: u32,
    pub from: u8,
    /// Players the other club would send here.
    pub give: Vec<Seen>,
    /// Players it asks for from this club.
    pub get: Vec<Seen>,
}

/// What a general manager sees when trading is open.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TradeObs {
    pub incoming: Vec<IncomingOffer>,
    /// Major- and top-minor-league players on every other club.
    pub market: Vec<Held>,
}

/// Everything a general manager may see: the club's own books and roster,
/// the public market, noisy scouting reports. No true talent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GmObs {
    pub team: u8,
    pub year: i32,
    pub phase: Phase,
    pub cash: f64,
    pub budget: f64,
    pub payroll: f64,
    pub payroll_cap: f64,
    pub war_price: f64,
    pub roster: Vec<Seen>,
    pub free_agents: Vec<Seen>,
    /// Present only while this club is on the clock.
    pub draft: Option<DraftObs>,
    /// Present only when trading is open and the observation asked for it.
    pub trade: Option<TradeObs>,
}

/// A general manager's window on the league.
pub struct GmView<'a> {
    w: &'a World,
    pub team: u8,
}

impl<'a> GmView<'a> {
    #[must_use]
    pub fn new(w: &'a World, team: u8) -> Self {
        Self { w, team }
    }

    fn see(&self, id: PlayerId) -> Seen {
        let p = &self.w.players[&id];
        let o = opinion(self.w, self.team, p);
        Seen {
            id,
            kind: p.kind,
            age: p.age(self.w.year),
            level: p.level,
            origin: p.origin,
            now: o.now,
            peak: o.peak,
            ask: asking_salary(self.w, p),
            ask_years: asking_years(self.w, p),
            demand: crate::scout::amateur_demand(self.w, p),
            owed: p
                .contract
                .filter(|_| p.team == Some(self.team))
                .map_or(0.0, |c| c.salary * f64::from(c.years)),
            salary: p.contract.map_or(0.0, |c| c.salary),
            years: p.contract.map_or(0, |c| c.years),
        }
    }

    /// The pick on the clock for this club.
    fn draft(&self) -> Option<DraftObs> {
        if self.w.phase != Phase::Drafting {
            return None;
        }
        let d = self.w.draft.as_ref()?;
        if d.order.get(d.next) != Some(&self.team) {
            return None;
        }
        let international = d.next >= d.amateur_picks;
        let slot = if international {
            1.0
        } else {
            slot_bonus((d.next + 1) as u16)
        };
        Some(DraftObs {
            pick: d.next,
            ceiling: SIGNABLE * slot,
            international,
            prospects: d.remaining.iter().map(|id| self.see(*id)).collect(),
        })
    }

    #[must_use]
    pub fn observe(&self) -> GmObs {
        let c = &self.w.clubs[usize::from(self.team)];
        GmObs {
            team: self.team,
            year: self.w.year,
            phase: self.w.phase,
            cash: c.cash,
            budget: c.budget,
            payroll: self.w.payroll(usize::from(self.team)),
            payroll_cap: self.w.rules.payroll_cap,
            war_price: self.w.rules.war_price,
            roster: c.players.iter().map(|id| self.see(*id)).collect(),
            free_agents: self.w.free_agents.iter().map(|id| self.see(*id)).collect(),
            draft: self.draft(),
            trade: None,
        }
    }

    /// The usual observation plus the trade market and offers put to this club.
    #[must_use]
    pub fn observe_trades(&self) -> GmObs {
        let mut obs = self.observe();
        let me = usize::from(self.team);
        let market = self
            .w
            .clubs
            .iter()
            .enumerate()
            .filter(|(t, _)| *t != me)
            .flat_map(|(t, c)| {
                c.players
                    .iter()
                    .filter(|id| self.w.players[*id].level <= Level::Triple)
                    .map(move |id| (t as u8, *id))
            })
            .map(|(club, id)| Held {
                club,
                seen: self.see(id),
            })
            .collect();
        let incoming = self
            .w
            .offers
            .iter()
            .filter(|o| o.to == self.team)
            .map(|o| IncomingOffer {
                id: o.id,
                from: o.from,
                give: o.give.iter().map(|id| self.see(*id)).collect(),
                get: o.get.iter().map(|id| self.see(*id)).collect(),
            })
            .collect();
        obs.trade = Some(TradeObs { incoming, market });
        obs
    }
}

/// Make the club's draft pick: the best-scouted prospect who will sign.
#[must_use]
pub fn draft_pick(v: &GmObs) -> Option<Command> {
    let d = v.draft.as_ref()?;
    let mut best: Option<Seen> = None;
    for s in &d.prospects {
        if (s.origin == Origin::International) != d.international
            || s.demand > d.ceiling
            || v.cash - s.demand < -CREDIT
        {
            continue;
        }
        let better = |a: &Seen, b: &Seen| {
            a.draft_score()
                .partial_cmp(&b.draft_score())
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(b.id.cmp(&a.id))
                == std::cmp::Ordering::Greater
        };
        if best.is_none_or(|b| better(s, &b)) {
            best = Some(*s);
        }
    }
    Some(match best {
        Some(s) => Command::DraftPlayer {
            team: v.team,
            player: s.id,
        },
        None => Command::PassPick { team: v.team },
    })
}

/// One free-agent signing, or `None` when the club is content.
#[must_use]
pub fn free_agency_step(v: &GmObs) -> Option<Command> {
    let roster = &v.roster;
    let fa = &v.free_agents;
    let payroll = v.payroll;
    let price = v.war_price;
    let room = |ask: f64| {
        ask <= MIN_SALARY + 1e-9
            || (payroll + ask <= v.budget + 1e-9 && payroll + ask <= v.payroll_cap)
    };
    let mut best: Option<(f64, Seen)> = None;
    for kind in [Kind::Batter, Kind::Pitcher] {
        let mut mine: Vec<&Seen> = roster.iter().filter(|s| s.kind == kind).collect();
        mine.sort_by(|a, b| by_score_desc(a, b));
        let short = mine.len() < MAJORS_PER_KIND;
        if mine.len() >= ROSTER_MAX_PER_KIND {
            continue;
        }
        let bar = if short {
            f64::NEG_INFINITY
        } else {
            mine[MAJORS_PER_KIND - 1].score() + UPGRADE_MARGIN
        };
        for f in fa.iter().filter(|f| f.kind == kind) {
            if f.score() <= bar || !room(f.ask) {
                continue;
            }
            let surplus = f.surplus(price);
            if !short && surplus <= 0.0 {
                continue;
            }
            if best.is_none_or(|(s, b)| (surplus, b.id) > (s, f.id)) {
                best = Some((surplus, *f));
            }
        }
    }
    best.map(|(_, f)| Command::SignFreeAgent {
        team: v.team,
        player: f.id,
        salary: f.ask,
        years: f.sign_years(),
    })
}

/// Put the roster in order: best thirteen of each kind in the majors, then
/// the minor-league levels; release whoever does not fit.
#[must_use]
pub fn set_levels(v: &GmObs) -> Vec<Command> {
    let roster = &v.roster;
    let mut out = Vec::new();
    for kind in [Kind::Batter, Kind::Pitcher] {
        let mut mine: Vec<&Seen> = roster.iter().filter(|s| s.kind == kind).collect();
        mine.sort_by(|a, b| by_score_desc(a, b));
        let mut at = 0;
        for level in [Level::Majors, Level::Triple, Level::Double, Level::Single] {
            for s in mine.iter().skip(at).take(level.capacity()) {
                if s.level != level {
                    out.push(Command::SetLevel {
                        team: v.team,
                        player: s.id,
                        level,
                    });
                }
            }
            at += level.capacity();
        }
        // Release the surplus, cheapest contracts first if cash is short.
        let mut spare: Vec<&&Seen> = mine.iter().skip(at).collect();
        spare.sort_by(|a, b| {
            a.owed
                .partial_cmp(&b.owed)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        for s in spare {
            out.push(Command::ReleasePlayer {
                team: v.team,
                player: s.id,
            });
        }
    }
    out
}

/// What an owner may see: the club's books and the league's payroll limits.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OwnerObs {
    pub team: u8,
    pub year: i32,
    pub phase: Phase,
    pub market: f64,
    pub cash: f64,
    pub revenue_prev: f64,
    pub hype: f64,
    pub win_pct_prev: f64,
    pub payroll: f64,
    pub budget: f64,
    pub payroll_floor: f64,
    pub payroll_cap: f64,
}

impl OwnerObs {
    #[must_use]
    pub fn new(w: &World, team: u8) -> Self {
        let c = &w.clubs[usize::from(team)];
        Self {
            team,
            year: w.year,
            phase: w.phase,
            market: c.market,
            cash: c.cash,
            revenue_prev: c.revenue_prev,
            hype: c.hype,
            win_pct_prev: c.win_pct_prev,
            payroll: w.payroll(usize::from(team)),
            budget: c.budget,
            payroll_floor: w.rules.payroll_floor,
            payroll_cap: w.rules.payroll_cap,
        }
    }
}

/// The owner's payroll budget for the coming season, from what the club
/// earned last year and how much cash it holds.
#[must_use]
pub fn owner_budget_for(o: &OwnerObs) -> f64 {
    let other = 80.0 + 10.0 * o.market;
    let reserve = 0.2 * o.revenue_prev;
    let cash_push = 0.25 * (o.cash - reserve).clamp(-80.0, 40.0);
    (0.95 * o.revenue_prev - other + cash_push).clamp(o.payroll_floor, o.payroll_cap)
}

#[must_use]
pub fn owner_budget(w: &World, team: u8) -> Command {
    Command::SetBudget {
        team,
        payroll: owner_budget_for(&OwnerObs::new(w, team)),
    }
}
