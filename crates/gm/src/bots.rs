//! Exploit bots: deliberately lopsided general managers used to check that
//! no cheap trick gives a lasting edge. Each one leans on one rule of the
//! league and otherwise plays like the built-in general manager.

use connect::{BuiltInGm, GmAction, GmAgent, GmStage, GmTurn};
use world::agents::{Seen, salary_dumps};
use world::player::Level;
use world::rating::Kind;
use world::state::{MAJORS_PER_KIND, MIN_SALARY, ROSTER_MAX_PER_KIND};

fn room(t: &GmTurn, f: &Seen) -> bool {
    t.obs.may_sign(f.kind, f.ask)
}

fn count(t: &GmTurn, kind: Kind) -> usize {
    t.obs.roster.iter().filter(|s| s.kind == kind).count()
}

fn best_now(t: &GmTurn, ok: impl Fn(&Seen) -> bool) -> Option<&Seen> {
    t.obs
        .free_agents
        .iter()
        .filter(|f| count(t, f.kind) < ROSTER_MAX_PER_KIND && room(t, f) && ok(f))
        .max_by(|a, b| a.now.total_cmp(&b.now).then(b.id.cmp(&a.id)))
}

/// Signs the best-looking free agent it can fit under the budget, whatever
/// the price, until the budget is gone.
#[derive(Debug, Default)]
pub struct Hoarder;

impl GmAgent for Hoarder {
    fn trades(&self) -> bool {
        BuiltInGm.trades()
    }

    fn act(&mut self, turn: &GmTurn) -> Vec<GmAction> {
        if turn.stage != GmStage::FreeAgency {
            return BuiltInGm.act(turn);
        }
        best_now(turn, |_| true)
            .map(|f| GmAction::Sign {
                player: f.id,
                salary: f.ask,
                years: f.ask_years,
            })
            .into_iter()
            .collect()
    }
}

/// Spends the minimum: signs only players who ask the league minimum.
#[derive(Debug, Default)]
pub struct Tanker;

impl GmAgent for Tanker {
    fn trades(&self) -> bool {
        BuiltInGm.trades()
    }

    fn act(&mut self, turn: &GmTurn) -> Vec<GmAction> {
        if turn.stage != GmStage::FreeAgency {
            return BuiltInGm.act(turn);
        }
        best_now(turn, |f| f.ask <= MIN_SALARY + 1e-9)
            .map(|f| GmAction::Sign {
                player: f.id,
                salary: f.ask,
                years: 1,
            })
            .into_iter()
            .collect()
    }
}

/// Built-in choices, but every contract runs seven years.
#[derive(Debug, Default)]
pub struct Stretcher;

impl GmAgent for Stretcher {
    fn trades(&self) -> bool {
        BuiltInGm.trades()
    }

    fn act(&mut self, turn: &GmTurn) -> Vec<GmAction> {
        let mut acts = BuiltInGm.act(turn);
        for a in &mut acts {
            if let GmAction::Sign { years, .. } = a {
                *years = 7;
            }
        }
        acts
    }
}

/// Offers many one-for-one swaps picked by raw scouting reports and accepts
/// anything that looks like a gain on them.
#[derive(Debug, Default)]
pub struct TradeSpammer;

impl GmAgent for TradeSpammer {
    fn trades(&self) -> bool {
        true
    }

    fn act(&mut self, turn: &GmTurn) -> Vec<GmAction> {
        let Some(t) = (turn.stage == GmStage::Trade)
            .then_some(turn.obs.trade.as_ref())
            .flatten()
        else {
            return BuiltInGm.act(turn);
        };
        let mut out: Vec<GmAction> = t
            .incoming
            .iter()
            .map(|o| {
                let sum = |v: &[Seen]| v.iter().map(|s| s.now).sum::<f64>();
                let pay = |v: &[Seen]| v.iter().map(|s| s.salary).sum::<f64>();
                // Greedy on quality, but never for salary the club may not take.
                let affordable = turn
                    .obs
                    .may_take_salary(turn.obs.payroll, pay(&o.give) - pay(&o.get));
                GmAction::RespondTrade {
                    offer: o.id,
                    accept: affordable && sum(&o.give) > sum(&o.get),
                }
            })
            .collect();
        out.extend(dumps(turn));
        let mut mine: Vec<&Seen> = turn
            .obs
            .roster
            .iter()
            .filter(|s| s.level <= Level::Triple)
            .collect();
        mine.sort_by(|a, b| a.now.total_cmp(&b.now).then(a.id.cmp(&b.id)));
        let in_debt = |club: u8| t.clubs.iter().any(|f| f.club == club && f.in_debt);
        let mut used = Vec::new();
        for x in mine.iter().take(8) {
            let best = t
                .market
                .iter()
                .filter(|h| {
                    h.seen.kind == x.kind
                        && h.seen.now > x.now
                        && (h.seen.salary - x.salary).abs() < 3.0
                        && !used.contains(&h.seen.id)
                        // No salary to a side the league would refuse it to.
                        && (h.seen.salary <= x.salary || !turn.obs.in_debt())
                        && (x.salary <= h.seen.salary || !in_debt(h.club))
                })
                .max_by(|a, b| {
                    a.seen
                        .now
                        .total_cmp(&b.seen.now)
                        .then(b.seen.id.cmp(&a.seen.id))
                });
            if let Some(h) = best {
                used.push(h.seen.id);
                out.push(GmAction::ProposeTrade {
                    partner: h.club,
                    give: vec![x.id],
                    get: vec![h.seen.id],
                });
            }
            if used.len() >= 5 {
                break;
            }
        }
        out
    }
}

/// The reference general manager of the evaluation ladder: the built-in one,
/// plus a plain rule for trade offers (accept when the players coming in
/// score better on its own reports than the players going out), plus
/// spending the budget the usual step leaves idle on one-year upgrades.
#[derive(Debug, Default)]
pub struct ReferenceGm;

impl GmAgent for ReferenceGm {
    fn trades(&self) -> bool {
        true
    }

    fn act(&mut self, turn: &GmTurn) -> Vec<GmAction> {
        if turn.stage == GmStage::FreeAgency {
            let acts = BuiltInGm.act(turn);
            if !acts.is_empty() {
                return acts;
            }
            return spend_leftover(turn);
        }
        BuiltInGm.act(turn)
    }
}

/// Salary-shedding offers when the club is in debt (none otherwise).
fn dumps(t: &GmTurn) -> Vec<GmAction> {
    salary_dumps(&t.obs)
        .into_iter()
        .filter_map(|c| GmAction::from_command(&c))
        .collect()
}

/// With the usual signing step finished, buy the best upgrade the budget
/// still allows, whatever the price: a budget is there to be spent. First
/// upgrades to the thirteen who play, then to the depth behind them (the
/// Triple-A level), who play when injuries strike.
fn spend_leftover(t: &GmTurn) -> Vec<GmAction> {
    let depth = MAJORS_PER_KIND + Level::Triple.capacity();
    for spots in [MAJORS_PER_KIND, depth] {
        if let Some(a) = best_upgrade(t, spots) {
            return vec![a];
        }
    }
    Vec::new()
}

/// The best free agent who beats the club's `spots`-th best player of his
/// kind, if the budget has room.
fn best_upgrade(t: &GmTurn, spots: usize) -> Option<GmAction> {
    let mut best: Option<&Seen> = None;
    for kind in [Kind::Batter, Kind::Pitcher] {
        let mut mine: Vec<f64> = t
            .obs
            .roster
            .iter()
            .filter(|s| s.kind == kind)
            .map(Seen::score)
            .collect();
        mine.sort_by(|a, b| b.total_cmp(a));
        if mine.len() < MAJORS_PER_KIND || mine.len() >= ROSTER_MAX_PER_KIND {
            continue;
        }
        let bar = mine.get(spots - 1).copied().unwrap_or(f64::NEG_INFINITY) + 0.0015;
        for f in t.obs.free_agents.iter().filter(|f| f.kind == kind) {
            if f.score() > bar
                && room(t, f)
                && best.is_none_or(|b| (f.score(), b.id) > (b.score(), f.id))
            {
                best = Some(f);
            }
        }
    }
    best.map(|f| GmAction::Sign {
        player: f.id,
        salary: f.ask,
        years: if f.age <= 25 { f.sign_years() } else { 1 },
    })
}
