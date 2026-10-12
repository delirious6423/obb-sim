//! Fixed-behaviour agents for the roles the league needs filled: a general
//! manager and an owner per club. They see only what their role may see:
//! a club's own books and roster, the public market, and noisy scouting
//! reports. They never read a player's true talent.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::command::Command;
use crate::player::{Level, Origin, PlayerId, slot_bonus};
use crate::rating::Kind;
use crate::scout::{asking_salary, asking_years, opinion, war_of};
use crate::state::{
    CAPACITY_MAX, MAJORS_PER_KIND, MIN_SALARY, Phase, ROSTER_MAX_PER_KIND, Rules, Stadium, Stance,
    TICKET_ELASTICITY, TICKET_RANGE, TICKET_REF, TradeOffer, World,
};

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
    /// Public facts about every club, this one included (standings, payroll,
    /// roster size).
    #[serde(default)]
    pub clubs: Vec<ClubFacts>,
    /// This club's own offers still waiting for an answer.
    #[serde(default)]
    pub outgoing: Vec<TradeOffer>,
}

/// What anyone can read about another club: last season's record, its
/// payroll and how many players of each kind it holds.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ClubFacts {
    pub club: u8,
    pub win_pct_prev: f64,
    pub payroll: f64,
    pub batters: u8,
    pub pitchers: u8,
    /// Whether the club is in debt: such a club may take on no salary by
    /// trade, so offers that add to its payroll are pointless.
    #[serde(default)]
    pub in_debt: bool,
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
    /// What the owner has told the general manager the club is trying to do.
    #[serde(default)]
    pub stance: Stance,
    pub roster: Vec<Seen>,
    pub free_agents: Vec<Seen>,
    /// Present only while this club is on the clock.
    pub draft: Option<DraftObs>,
    /// Present only when trading is open and the observation asked for it.
    pub trade: Option<TradeObs>,
}

impl GmObs {
    /// Whether the club is in debt (see [`World::in_debt`]).
    #[must_use]
    pub fn in_debt(&self) -> bool {
        self.cash < 0.0
    }

    /// Whether the club may sign a free agent of `kind` asking `ask` a year:
    /// the same test as `Command::SignFreeAgent`. A minimum-salary deal needs
    /// no budget room; a club in debt may only sign minimum-salary players
    /// while it is short of a major-league staff of that kind.
    #[must_use]
    pub fn may_sign(&self, kind: Kind, ask: f64) -> bool {
        let minimum = ask <= MIN_SALARY + 1e-9;
        if self.in_debt() {
            return minimum
                && self.roster.iter().filter(|s| s.kind == kind).count() < MAJORS_PER_KIND;
        }
        minimum
            || (self.payroll + ask <= self.budget + 1e-9 && self.payroll + ask <= self.payroll_cap)
    }

    /// The free agent of `kind` this club may sign at the minimum salary
    /// whatever he asks, because it is short of a staff of that kind and
    /// nobody of that kind asks the minimum (see [`World::short_fill`]).
    #[must_use]
    pub fn short_fill(&self, kind: Kind) -> Option<&Seen> {
        if self.roster.iter().filter(|s| s.kind == kind).count() >= MAJORS_PER_KIND {
            return None;
        }
        let mut best: Option<&Seen> = None;
        for f in self.free_agents.iter().filter(|f| f.kind == kind) {
            if f.ask <= MIN_SALARY + 1e-9 {
                return None;
            }
            if best.is_none_or(|b| f.ask < b.ask) {
                best = Some(f);
            }
        }
        best
    }

    /// Whether the club may take on `added` salary (millions a year, net) in
    /// a trade, its payroll being `payroll` by then: never while in debt
    /// (the league refuses it), otherwise only within the owner's budget
    /// (or the payroll it already carries, if higher) and the cap. Shedding
    /// salary is always allowed.
    #[must_use]
    pub fn may_take_salary(&self, payroll: f64, added: f64) -> bool {
        added <= 1e-9
            || (!self.in_debt()
                && payroll + added <= self.budget.max(self.payroll) + 1e-9
                && payroll + added <= self.payroll_cap + 1e-9)
    }
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

    /// One player as this club sees him.
    #[must_use]
    pub fn see_player(&self, id: PlayerId) -> Seen {
        self.see(id)
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
            stance: c.stance,
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
        let outgoing = self
            .w
            .offers
            .iter()
            .filter(|o| o.from == self.team)
            .cloned()
            .collect();
        let clubs = (0..self.w.clubs.len())
            .map(|t| {
                let c = &self.w.clubs[t];
                let n = |k: Kind| {
                    c.players
                        .iter()
                        .filter(|id| self.w.players[*id].kind == k)
                        .count() as u8
                };
                ClubFacts {
                    club: t as u8,
                    win_pct_prev: c.win_pct_prev,
                    payroll: self.w.payroll(t),
                    batters: n(Kind::Batter),
                    pitchers: n(Kind::Pitcher),
                    in_debt: self.w.in_debt(t as u8),
                }
            })
            .collect();
        obs.trade = Some(TradeObs {
            incoming,
            market,
            clubs,
            outgoing,
        });
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
            || v.cash - s.demand < 0.0
            || v.in_debt()
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
    let price = v.war_price;
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
            if f.score() <= bar || !v.may_sign(kind, f.ask) {
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
    if let Some((_, f)) = best {
        return Some(Command::SignFreeAgent {
            team: v.team,
            player: f.id,
            salary: f.ask,
            years: f.sign_years(),
        });
    }
    short_fill_step(v)
}

/// A club short of a staff of one kind, with nobody of that kind asking the
/// minimum, fills it at the minimum salary for one year (see
/// [`World::short_fill`]).
#[must_use]
pub fn short_fill_step(v: &GmObs) -> Option<Command> {
    [Kind::Batter, Kind::Pitcher].into_iter().find_map(|kind| {
        v.short_fill(kind).map(|f| Command::SignFreeAgent {
            team: v.team,
            player: f.id,
            salary: MIN_SALARY,
            years: 1,
        })
    })
}

/// Quality (summed scores) a trade must add before the built-in general
/// manager agrees to it.
const TRADE_MARGIN: f64 = 0.001;

/// Most salary-shedding offers a club in debt makes in one trading turn.
const DUMPS_PER_TURN: usize = 2;

/// The built-in general manager's trading turn. It answers offers put to it
/// (yes when the players coming in score better on its own reports and the
/// payroll stays inside its budget and the cap); a club in debt also offers
/// its dearest contracts for a cheap, weaker player of the same kind, each to
/// a different club, to shed salary. A solvent club proposes nothing.
#[must_use]
pub fn trade_moves(v: &GmObs) -> Vec<Command> {
    let mut out: Vec<Command> = v
        .trade
        .iter()
        .flat_map(|t| &t.incoming)
        .map(|o| Command::RespondTrade {
            team: v.team,
            offer: o.id,
            accept: trade_ok(v, &o.give, &o.get),
        })
        .collect();
    out.extend(salary_dumps(v));
    out
}

/// Whether the built-in general manager takes `inn` for `out`.
#[must_use]
pub fn trade_ok(v: &GmObs, inn: &[Seen], out: &[Seen]) -> bool {
    let sum = |s: &[Seen], f: fn(&Seen) -> f64| s.iter().map(f).sum::<f64>();
    let fits = [Kind::Batter, Kind::Pitcher].into_iter().all(|k| {
        let n = |s: &[Seen]| s.iter().filter(|x| x.kind == k).count();
        v.roster.iter().filter(|s| s.kind == k).count() + n(inn) <= ROSTER_MAX_PER_KIND + n(out)
    });
    let added = sum(inn, |s| s.salary) - sum(out, |s| s.salary);
    fits && v.may_take_salary(v.payroll, added)
        && sum(inn, Seen::score) - sum(out, Seen::score) > TRADE_MARGIN
}

/// Offers a club in debt makes to shed salary: its dearest contracts, each
/// for the best minimum-salary player of the same kind on another club who
/// is clearly weaker (so the other club gains on the deal). Clubs that are
/// themselves in debt cannot take the salary and are skipped, and so are
/// clubs this one already has an open offer to. Empty for a solvent club or
/// when the trade market is closed.
#[must_use]
pub fn salary_dumps(v: &GmObs) -> Vec<Command> {
    let Some(t) = v.trade.as_ref().filter(|_| v.in_debt()) else {
        return Vec::new();
    };
    let mut dear: Vec<&Seen> = v
        .roster
        .iter()
        .filter(|s| s.salary > 2.0 * MIN_SALARY)
        .collect();
    dear.sort_by(|a, b| b.salary.total_cmp(&a.salary).then(a.id.cmp(&b.id)));
    // Partners that cannot take the salary or already have an offer.
    let mut used: BTreeSet<u8> = t
        .clubs
        .iter()
        .filter(|f| f.in_debt)
        .map(|f| f.club)
        .chain(t.outgoing.iter().map(|o| o.to))
        .collect();
    let offered: BTreeSet<PlayerId> = t
        .outgoing
        .iter()
        .flat_map(|o| o.give.iter().chain(&o.get).copied())
        .collect();
    let mut out = Vec::new();
    for x in dear.into_iter().filter(|x| !offered.contains(&x.id)) {
        if out.len() >= DUMPS_PER_TURN {
            break;
        }
        let back = t
            .market
            .iter()
            .filter(|h| {
                h.seen.kind == x.kind
                    && h.seen.salary <= MIN_SALARY + 1e-9
                    && h.seen.score() < x.score() - 2.0 * TRADE_MARGIN
                    && !used.contains(&h.club)
            })
            .max_by(|a, b| {
                a.seen
                    .score()
                    .total_cmp(&b.seen.score())
                    .then(b.seen.id.cmp(&a.seen.id))
            });
        if let Some(h) = back {
            used.insert(h.club);
            out.push(Command::ProposeTrade {
                team: v.team,
                partner: h.club,
                give: vec![x.id],
                get: vec![h.seen.id],
            });
        }
    }
    out
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
        // The surplus goes, worst first. A release pays the rest of the
        // contract at once, so a player the club cannot pay off without
        // going below zero cash stays (his salary is owed either way) and
        // the next-worst player goes instead.
        let room: usize = [Level::Majors, Level::Triple, Level::Double, Level::Single]
            .iter()
            .map(|l| l.capacity())
            .sum();
        let surplus = mine.len().saturating_sub(room);
        let mut cash = v.cash;
        let mut gone = BTreeSet::new();
        for s in mine.iter().rev() {
            if gone.len() == surplus {
                break;
            }
            if cash - s.owed >= 0.0 {
                cash -= s.owed;
                gone.insert(s.id);
            }
        }
        if gone.len() < surplus {
            let mut rest: Vec<&Seen> = mine
                .iter()
                .filter(|s| !gone.contains(&s.id))
                .copied()
                .collect();
            rest.sort_by(|a, b| a.owed.total_cmp(&b.owed).then(a.id.cmp(&b.id)));
            let short = surplus - gone.len();
            gone.extend(rest.iter().take(short).map(|s| s.id));
        }
        let mut spare: Vec<&Seen> = mine
            .iter()
            .filter(|s| gone.contains(&s.id))
            .copied()
            .collect();
        mine.retain(|s| !gone.contains(&s.id));
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
        // Release the surplus, cheapest contracts first.
        spare.sort_by(|a, b| a.owed.total_cmp(&b.owed));
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
    /// The club's ballpark and ticket price.
    #[serde(default)]
    pub stadium: Stadium,
    /// Last season's average crowd per home game (thousands) and gate receipts.
    #[serde(default)]
    pub attendance_prev: f64,
    #[serde(default)]
    pub gate_prev: f64,
    #[serde(default)]
    pub stance: Stance,
}

impl OwnerObs {
    #[must_use]
    pub fn new(w: &World, team: u8) -> Self {
        let c = &w.clubs[usize::from(team)];
        let (attendance_prev, gate_prev) = last_crowd(w, team);
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
            stadium: c.stadium,
            attendance_prev,
            gate_prev,
            stance: c.stance,
        }
    }

    /// Whether last season's crowd filled the park (so true demand is not
    /// known, only that it was at least the capacity).
    #[must_use]
    pub fn sold_out(&self) -> bool {
        self.attendance_prev >= 0.98 * self.stadium.capacity
    }
}

/// Last settled season's average crowd (thousands per home game) and gate.
///
/// A rescued year is reported as `(0.0, 0.0)`: its crowd was drawn at the old
/// ticket price, which the league has since reset, so it says nothing about
/// demand at the price the owner is setting now. Zero means "no usable
/// information", which keeps the owner's ticket price unchanged.
#[must_use]
pub fn last_crowd(w: &World, team: u8) -> (f64, f64) {
    match w.finances[usize::from(team)].last() {
        Some(f) if !f.rescued => (f.attendance, f.gate),
        _ => (0.0, 0.0),
    }
}

/// The owner's payroll budget for the coming season, from what the club
/// earned last year and how much cash it holds.
#[must_use]
pub fn owner_budget_for(o: &OwnerObs) -> f64 {
    let other = 80.0 + 10.0 * o.market + 0.1 * o.stadium.capacity * o.stadium.quality;
    let reserve = 0.2 * o.revenue_prev;
    let cash_push = 0.25 * (o.cash - reserve).clamp(-80.0, 40.0);
    let limit = crate::state::budget_limit_for(
        o.market,
        &o.stadium,
        o.revenue_prev,
        o.cash,
        o.payroll_floor,
    );
    (0.95 * o.revenue_prev - other + cash_push)
        .min(limit)
        .clamp(o.payroll_floor, o.payroll_cap)
}

#[must_use]
pub fn owner_budget(w: &World, team: u8) -> Command {
    Command::SetBudget {
        team,
        payroll: owner_budget_for(&OwnerObs::new(w, team)),
    }
}

/// The ticket price an owner sets for the coming season. A sold-out park
/// means demand is unknown but at least the capacity, so raise the price a
/// step; otherwise demand is known from last year's crowd, and the best
/// price is where demand just fills the park (move half way there).
#[must_use]
pub fn owner_ticket_for(o: &OwnerObs) -> f64 {
    let st = &o.stadium;
    let next = if o.attendance_prev <= 0.0 {
        st.ticket
    } else if o.sold_out() {
        st.ticket * 1.08
    } else {
        let base = o.attendance_prev
            / (libm::pow(st.ticket / TICKET_REF, -TICKET_ELASTICITY) * st.quality);
        let fill = TICKET_REF * libm::pow(base * st.quality / st.capacity, 1.0 / TICKET_ELASTICITY);
        0.5 * (st.ticket + fill)
    };
    next.clamp(TICKET_RANGE.0, TICKET_RANGE.1)
}

/// A ballpark project (millions) when the park sells out at a high price and
/// the club can pay out of cash it does not need; none otherwise.
#[must_use]
pub fn owner_build_for(o: &OwnerObs) -> Option<f64> {
    let st = &o.stadium;
    let spare = o.cash - 0.4 * o.revenue_prev;
    if o.sold_out()
        && st.ticket > 1.3 * TICKET_REF
        && st.capacity < CAPACITY_MAX - 1.0
        && spare > 20.0
    {
        Some((0.25 * spare).clamp(10.0, 40.0))
    } else {
        None
    }
}

/// What the owner tells the general manager, from how the club did.
#[must_use]
pub fn owner_stance_for(o: &OwnerObs) -> Stance {
    stance_from_record(o.win_pct_prev)
}

/// The direction a club's record points to (what anyone reading the
/// standings would guess the club is trying to do).
#[must_use]
pub fn stance_from_record(win_pct_prev: f64) -> Stance {
    if win_pct_prev >= 0.56 {
        Stance::Contend
    } else if win_pct_prev <= 0.42 {
        Stance::Rebuild
    } else {
        Stance::Balanced
    }
}

/// What a commissioner sees of one club's books.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClubBooks {
    pub team: u8,
    pub market: f64,
    pub cash: f64,
    pub revenue_prev: f64,
    pub payroll: f64,
}

/// The commissioner's window on league finances (public books).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LeagueObs {
    pub year: i32,
    pub phase: Phase,
    pub rules: Rules,
    pub clubs: Vec<ClubBooks>,
}

impl LeagueObs {
    #[must_use]
    pub fn new(w: &World) -> Self {
        Self {
            year: w.year,
            phase: w.phase,
            rules: w.rules.clone(),
            clubs: w
                .clubs
                .iter()
                .enumerate()
                .map(|(t, c)| ClubBooks {
                    team: t as u8,
                    market: c.market,
                    cash: c.cash,
                    revenue_prev: c.revenue_prev,
                    payroll: w.payroll(t),
                })
                .collect(),
        }
    }
}

/// Revenue sharing the league office would set: more when clubs are short of
/// cash or when small markets cannot keep up, less when everyone is flush.
/// It never touches the payroll limits.
#[must_use]
pub fn commissioner_rules_for(l: &LeagueObs) -> Option<Rules> {
    let n = l.clubs.len() as f64;
    if n == 0.0 {
        return None;
    }
    let broke = l
        .clubs
        .iter()
        .filter(|c| c.cash < 0.05 * c.revenue_prev)
        .count() as f64;
    let flush = l
        .clubs
        .iter()
        .filter(|c| c.cash > 0.5 * c.revenue_prev)
        .count() as f64;
    let mut rules = l.rules.clone();
    if broke / n > 0.1 {
        rules.revenue_sharing = (rules.revenue_sharing + 0.02).min(0.25);
    } else if flush / n > 0.8 {
        rules.revenue_sharing = (rules.revenue_sharing - 0.01).max(0.06);
    }
    (rules != l.rules).then_some(rules)
}

/// Everything the built-in owner does in an offseason, in order.
#[must_use]
pub fn owner_moves(w: &World, team: u8) -> Vec<Command> {
    let o = OwnerObs::new(w, team);
    owner_moves_for(&o)
}

/// The built-in owner's moves for an observation.
#[must_use]
pub fn owner_moves_for(o: &OwnerObs) -> Vec<Command> {
    let team = o.team;
    let mut v = vec![Command::SetBudget {
        team,
        payroll: owner_budget_for(o),
    }];
    let price = owner_ticket_for(o);
    if (price - o.stadium.ticket).abs() > 0.005 {
        v.push(Command::SetTicketPrice { team, price });
    }
    if let Some(amount) = owner_build_for(o) {
        v.push(Command::BuildStadium { team, amount });
    }
    let stance = owner_stance_for(o);
    if stance != o.stance {
        v.push(Command::SetStance { team, stance });
    }
    v
}
