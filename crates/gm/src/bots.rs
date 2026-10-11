//! Exploit bots: deliberately lopsided general managers used to check that
//! no cheap trick gives a lasting edge. Each one leans on one rule of the
//! league and otherwise plays like the built-in general manager.

use connect::{BuiltInGm, GmAction, GmAgent, GmStage, GmTurn};
use world::agents::Seen;
use world::player::Level;
use world::rating::Kind;
use world::state::{MAJORS_PER_KIND, MIN_SALARY, ROSTER_MAX_PER_KIND};

fn room(t: &GmTurn, ask: f64) -> bool {
    let o = &t.obs;
    ask <= MIN_SALARY + 1e-9
        || (o.payroll + ask <= o.budget + 1e-9 && o.payroll + ask <= o.payroll_cap)
}

fn count(t: &GmTurn, kind: Kind) -> usize {
    t.obs.roster.iter().filter(|s| s.kind == kind).count()
}

fn best_now(t: &GmTurn, ok: impl Fn(&Seen) -> bool) -> Option<&Seen> {
    t.obs
        .free_agents
        .iter()
        .filter(|f| count(t, f.kind) < ROSTER_MAX_PER_KIND && room(t, f.ask) && ok(f))
        .max_by(|a, b| a.now.total_cmp(&b.now).then(b.id.cmp(&a.id)))
}

/// Signs the best-looking free agent it can fit under the budget, whatever
/// the price, until the budget is gone.
#[derive(Debug, Default)]
pub struct Hoarder;

impl GmAgent for Hoarder {
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
                GmAction::RespondTrade {
                    offer: o.id,
                    accept: sum(&o.give) > sum(&o.get),
                }
            })
            .collect();
        let mut mine: Vec<&Seen> = turn
            .obs
            .roster
            .iter()
            .filter(|s| s.level <= Level::Triple)
            .collect();
        mine.sort_by(|a, b| a.now.total_cmp(&b.now).then(a.id.cmp(&b.id)));
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
        let Some(t) = (turn.stage == GmStage::Trade)
            .then_some(turn.obs.trade.as_ref())
            .flatten()
        else {
            return BuiltInGm.act(turn);
        };
        t.incoming
            .iter()
            .map(|o| {
                let sum = |v: &[Seen]| v.iter().map(Seen::score).sum::<f64>();
                let fits = [Kind::Batter, Kind::Pitcher].into_iter().all(|k| {
                    let n = |v: &[Seen]| v.iter().filter(|s| s.kind == k).count();
                    count(turn, k) + n(&o.give) - n(&o.get) <= ROSTER_MAX_PER_KIND
                });
                GmAction::RespondTrade {
                    offer: o.id,
                    accept: fits && sum(&o.give) - sum(&o.get) > REFERENCE_TRADE_MARGIN,
                }
            })
            .collect()
    }
}

/// With the usual signing step finished, buy the best upgrade the budget
/// still allows, whatever the price: a budget is there to be spent.
fn spend_leftover(t: &GmTurn) -> Vec<GmAction> {
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
        let bar = mine[MAJORS_PER_KIND - 1] + 0.0015;
        for f in t.obs.free_agents.iter().filter(|f| f.kind == kind) {
            if f.score() > bar
                && room(t, f.ask)
                && best.is_none_or(|b| (f.score(), b.id) > (b.score(), f.id))
            {
                best = Some(f);
            }
        }
    }
    best.map(|f| GmAction::Sign {
        player: f.id,
        salary: f.ask,
        years: 1,
    })
    .into_iter()
    .collect()
}

/// Quality (runs per PA, summed) a trade must add before the reference agrees.
const REFERENCE_TRADE_MARGIN: f64 = 0.001;
