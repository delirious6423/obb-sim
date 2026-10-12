use std::sync::{Arc, Mutex};

use sim_core::{GameView, Manager, ManagerMaker, ReliefCall, StealCall};

use crate::tables::Tables;

/// Totals of what a manager decided, summed over all games. Counts only, so
/// the totals do not depend on the order games finished in.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DecisionStats {
    pub steals_sent: u64,
    pub steals_held: u64,
    pub relievers_brought: u64,
    /// Brought in with leverage above 1.5 / below 0.7.
    pub high_leverage_changes: u64,
    pub low_leverage_changes: u64,
    pub games: u64,
}

/// Shared tally one club's managers write to.
#[derive(Debug, Default)]
pub struct DecisionLog {
    stats: Mutex<DecisionStats>,
}

impl DecisionLog {
    #[must_use]
    pub fn stats(&self) -> DecisionStats {
        *self.stats.lock().expect("decision log poisoned")
    }
}

/// Builds the table-driven manager for a club.
#[derive(Debug, Clone)]
pub struct TableManagerMaker {
    tables: Arc<Tables>,
    /// What the club believes about each of its relievers, indexed like the
    /// team's pitcher list from 1 (higher is better).
    bullpen: Vec<f64>,
    log: Arc<DecisionLog>,
    /// Pitch count at which the starter is lifted at an inning's start
    /// (plus 15 within an inning); the engine's own limit by default.
    starter_limit: u16,
    reliever_limit: u16,
}

/// The engine's reliever pitch limit at the start of an inning.
pub const RELIEVER_LIMIT: u16 = 30;

/// The engine's starter pitch limit at the start of an inning.
pub const STARTER_LIMIT: u16 = 100;

impl TableManagerMaker {
    #[must_use]
    pub fn new(tables: Arc<Tables>, bullpen: Vec<f64>, log: Arc<DecisionLog>) -> Self {
        Self {
            tables,
            bullpen,
            log,
            starter_limit: STARTER_LIMIT,
            reliever_limit: RELIEVER_LIMIT,
        }
    }

    /// Lift starters at `starter` pitches at an inning's start (`starter +
    /// 15` within an inning) and relievers at `reliever` pitches at an
    /// inning's start (`reliever + 15` within one) instead of the engine's
    /// limits.
    #[must_use]
    pub fn with_limits(mut self, starter: u16, reliever: u16) -> Self {
        self.starter_limit = starter;
        self.reliever_limit = reliever;
        self
    }
}

impl ManagerMaker for TableManagerMaker {
    fn make(&self, _slot: usize) -> Box<dyn Manager> {
        Box::new(TableManager {
            tables: Arc::clone(&self.tables),
            bullpen: self.bullpen.clone(),
            log: Arc::clone(&self.log),
            local: DecisionStats::default(),
            starter_limit: self.starter_limit,
            reliever_limit: self.reliever_limit,
        })
    }
}

struct TableManager {
    tables: Arc<Tables>,
    bullpen: Vec<f64>,
    log: Arc<DecisionLog>,
    local: DecisionStats,
    starter_limit: u16,
    reliever_limit: u16,
}

/// Steals must beat holding by this many runs to be sent.
const STEAL_MARGIN: f64 = 0.01;

impl TableManager {
    /// Believed quality of pitcher `i` (an index into the team's list).
    fn quality(&self, i: usize) -> f64 {
        self.bullpen.get(i.wrapping_sub(1)).copied().unwrap_or(0.0)
    }

    /// Pick from the unused relievers by leverage: the best arm when it
    /// matters, the worst when it does not.
    fn choose(&self, v: &GameView, li: f64) -> Option<u8> {
        let mut pool: Vec<u8> = (1..v.pitchers)
            .filter(|&i| !v.used[usize::from(i)])
            .collect();
        if pool.is_empty() {
            return None;
        }
        pool.sort_by(|&a, &b| {
            self.quality(usize::from(a))
                .total_cmp(&self.quality(usize::from(b)))
                .then(a.cmp(&b))
        });
        let top = pool.len() - 1;
        let rank = if li >= 1.6 {
            top
        } else if li >= 1.0 {
            top.div_ceil(2) + usize::from(top == 1)
        } else if li >= 0.6 {
            top / 2
        } else {
            0
        };
        Some(pool[rank.min(top)])
    }

    fn bring(&mut self, v: &GameView, li: f64) -> ReliefCall {
        match self.choose(v, li) {
            Some(i) => {
                self.local.relievers_brought += 1;
                if li > 1.5 {
                    self.local.high_leverage_changes += 1;
                } else if li < 0.7 {
                    self.local.low_leverage_changes += 1;
                }
                ReliefCall::Bring(i)
            }
            None => ReliefCall::Keep,
        }
    }
}

impl Manager for TableManager {
    fn steal(&mut self, v: &GameView, from: u8) -> StealCall {
        let p = self.tables.steal_success(from);
        match self.tables.steal_gain(v, from, p) {
            Some(g) if g > STEAL_MARGIN => {
                self.local.steals_sent += 1;
                StealCall::Go
            }
            _ => {
                self.local.steals_held += 1;
                StealCall::Hold
            }
        }
    }

    fn relief(&mut self, v: &GameView, half_start: bool) -> ReliefCall {
        if v.pitchers < 2 {
            return ReliefCall::Keep;
        }
        let li = self.tables.leverage(v);
        let starter = v.pitcher == 0;
        // The starter and tired relievers go by the same limits as everyone's.
        let (pitch_limit, run_limit) = match (starter, half_start) {
            (true, true) => (self.starter_limit, 6),
            (true, false) => (self.starter_limit + 15, 8),
            (false, true) => (self.reliever_limit, 3),
            (false, false) => (self.reliever_limit + 15, 5),
        };
        if v.pitches_in_outing >= pitch_limit || v.runs_in_outing >= run_limit {
            return self.bring(v, li);
        }
        // A fresh start of an inning in a tight late spot: put the best
        // unused arm in, if he is clearly better than who is pitching.
        if half_start
            && v.inning >= 7
            && li >= 1.6
            && let Some(best) = self.choose(v, li)
        {
            let cur = if starter {
                f64::NEG_INFINITY
            } else {
                self.quality(usize::from(v.pitcher))
            };
            if self.quality(usize::from(best)) > cur + 1e-9 {
                return self.bring(v, li);
            }
        }
        ReliefCall::Keep
    }

    fn game_over(&mut self, _runs: [u16; 2]) {
        self.local.games += 1;
        let mut s = self.log.stats.lock().expect("decision log poisoned");
        s.steals_sent += self.local.steals_sent;
        s.steals_held += self.local.steals_held;
        s.relievers_brought += self.local.relievers_brought;
        s.high_leverage_changes += self.local.high_leverage_changes;
        s.low_leverage_changes += self.local.low_leverage_changes;
        s.games += self.local.games;
        self.local = DecisionStats::default();
    }
}
