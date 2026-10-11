//! Fixed-behaviour agents for the roles the league needs filled: a general
//! manager and an owner per club. They see only what their role may see:
//! a club's own books and roster, the public market, and noisy scouting
//! reports. They never read a player's true talent.

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

/// A player as one club sees him.
#[derive(Debug, Clone, Copy)]
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
        }
    }

    #[must_use]
    pub fn roster(&self) -> Vec<Seen> {
        self.w.clubs[usize::from(self.team)]
            .players
            .iter()
            .map(|id| self.see(*id))
            .collect()
    }

    #[must_use]
    pub fn free_agents(&self) -> Vec<Seen> {
        self.w.free_agents.iter().map(|id| self.see(*id)).collect()
    }

    #[must_use]
    pub fn payroll(&self) -> f64 {
        self.w.payroll(usize::from(self.team))
    }

    #[must_use]
    pub fn budget(&self) -> f64 {
        self.w.clubs[usize::from(self.team)].budget
    }

    #[must_use]
    pub fn war_price(&self) -> f64 {
        self.w.rules.war_price
    }

    /// The pick on the clock for this club: (pick index, signable-bonus
    /// ceiling, whether it is an international round, prospects left).
    fn on_the_clock(&self) -> Option<(usize, f64, bool)> {
        if self.w.phase != Phase::Drafting {
            return None;
        }
        let d = self.w.draft.as_ref()?;
        if d.order.get(d.next) != Some(&self.team) {
            return None;
        }
        let intl = d.next >= d.amateur_picks;
        let slot = if intl {
            1.0
        } else {
            slot_bonus((d.next + 1) as u16)
        };
        Some((d.next, SIGNABLE * slot, intl))
    }
}

/// Make the club's draft pick: the best-scouted prospect who will sign.
#[must_use]
pub fn draft_pick(v: &GmView) -> Option<Command> {
    let (_, ceiling, intl) = v.on_the_clock()?;
    let d = v.w.draft.as_ref()?;
    let mut best: Option<Seen> = None;
    for id in &d.remaining {
        let s = v.see(*id);
        let cash = v.w.clubs[usize::from(v.team)].cash;
        if (s.origin == Origin::International) != intl
            || s.demand > ceiling
            || cash - s.demand < -CREDIT
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
        if best.is_none_or(|b| better(&s, &b)) {
            best = Some(s);
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
pub fn free_agency_step(v: &GmView) -> Option<Command> {
    let roster = v.roster();
    let fa = v.free_agents();
    let payroll = v.payroll();
    let price = v.war_price();
    let room = |ask: f64| {
        ask <= MIN_SALARY + 1e-9
            || (payroll + ask <= v.budget() + 1e-9 && payroll + ask <= v.w.rules.payroll_cap)
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
        years: f.ask_years,
    })
}

/// Put the roster in order: best thirteen of each kind in the majors, then
/// the minor-league levels; release whoever does not fit.
#[must_use]
pub fn set_levels(v: &GmView) -> Vec<Command> {
    let roster = v.roster();
    let mut cash = v.w.clubs[usize::from(v.team)].cash;
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
            if cash - s.owed >= -CREDIT {
                cash -= s.owed;
                out.push(Command::ReleasePlayer {
                    team: v.team,
                    player: s.id,
                });
            }
        }
    }
    out
}

/// The owner's payroll budget for the coming season, from what the club
/// earned last year and how much cash it holds.
#[must_use]
pub fn owner_budget(w: &World, team: u8) -> Command {
    let c = &w.clubs[usize::from(team)];
    let other = 80.0 + 10.0 * c.market;
    let reserve = 0.2 * c.revenue_prev;
    let cash_push = 0.25 * (c.cash - reserve).clamp(-80.0, 40.0);
    let payroll = (0.95 * c.revenue_prev - other + cash_push)
        .clamp(w.rules.payroll_floor, w.rules.payroll_cap);
    Command::SetBudget { team, payroll }
}
