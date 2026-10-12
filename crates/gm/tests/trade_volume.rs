//! Trade volume and trade quality by difficulty (ignored by default; they
//! need the git-ignored `data/internal/params.json`):
//!
//!     cargo test --release -p gm --test trade_volume -- --ignored --nocapture
//!
//! Every club is run by the same preset, so the numbers are the league's own
//! trade market at that difficulty. Results are collected per seed and printed
//! once at the end, so the output is not interleaved.

use std::collections::BTreeMap;
use std::path::Path;

use calibration::Calibration;
use connect::{
    BuiltInCommissioner, BuiltInManager, BuiltInOwner, Seats, run_year_with, start_with,
};
use gm::{Personality, Preset, SurplusGm};
use rayon::prelude::*;
use world::command::Event;
use world::player::DEFAULT_POOL;
use world::rating::Model;
use world::state::World;

const SEEDS: [u64; 5] = [20_250_101, 1, 2, 3, 4];
const BURN_IN: u8 = 10;
const SEASONS: usize = 10;

fn calibration() -> Calibration {
    Calibration::load(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/internal/params.json"),
    )
    .unwrap_or_else(|e| panic!("{e}"))
}

/// Trade counts for one season of one league.
#[derive(Debug, Clone, Default)]
pub struct SeasonTrades {
    pub proposed: usize,
    pub accepted: usize,
    /// Answered no by the club it was put to.
    pub declined: usize,
    /// Never answered (lapsed when the season started).
    pub lapsed: usize,
    /// Refused by the league when proposed or accepted, by reason.
    pub refused: BTreeMap<&'static str, usize>,
    /// Accepted trades moving more than two players.
    pub multi: usize,
    /// Accepted trades that move at least `DUMP` millions a year of net salary
    /// from one club to the other.
    pub dump: usize,
    /// Accepted trades with a player on one side only.
    pub one_sided: usize,
}

/// Net salary (millions a year) that makes a trade a salary-relief deal.
const DUMP: f64 = 5.0;

/// Why the league refused a trade command, from the error text.
fn reason(error: &str) -> &'static str {
    if error.contains("would hold") {
        "roster max"
    } else if error.contains("would be left with") {
        "roster floor"
    } else if error.contains("payroll cap") {
        "payroll cap"
    } else if error.contains("not on club") {
        "stale player"
    } else if error.contains("open trade offers") {
        "offer cap"
    } else if error.contains("no such offer") {
        "no such offer"
    } else if error.contains("phase") || error.contains("Phase") || error.contains("calendar") {
        "phase"
    } else {
        "other"
    }
}

/// Plays `BURN_IN` founding years, then `SEASONS` league years with every
/// club run by `make`, and counts the trades of each season.
pub fn trade_stats(
    cal: &Calibration,
    seed: u64,
    make: &(dyn Fn(u8) -> SurplusGm + Sync),
) -> Vec<SeasonTrades> {
    let mut w = World::new(seed, cal.params, Model::from_calibration(cal), DEFAULT_POOL);
    let mut seats = Seats::fill(
        Box::new(BuiltInCommissioner {
            burn_in_years: BURN_IN,
        }),
        |_| Box::new(BuiltInOwner),
        |t| Box::new(make(t)),
        |_| Box::new(BuiltInManager::default()),
    );
    let mut log = Vec::new();
    start_with(&mut w, &mut seats, &mut log).expect("found");
    let mut out = Vec::new();
    for _ in 0..SEASONS {
        let (log_at, viol_at) = (log.len(), seats.violations.len());
        run_year_with(&mut w, &mut seats, &mut log).expect("a year");
        let mut s = SeasonTrades::default();
        let season = &log[log_at..];
        let answered: Vec<u32> = season
            .iter()
            .filter_map(|e| match e {
                Event::TradeAnswered { offer, accepted } => {
                    if *accepted {
                        s.accepted += 1;
                    } else {
                        s.declined += 1;
                    }
                    Some(*offer)
                }
                _ => None,
            })
            .collect();
        let accepted: Vec<u32> = season
            .iter()
            .filter_map(|e| match e {
                Event::TradeAnswered {
                    offer,
                    accepted: true,
                } => Some(*offer),
                _ => None,
            })
            .collect();
        for e in season {
            if let Event::TradeProposed {
                offer, give, get, ..
            } = e
            {
                s.proposed += 1;
                if !answered.contains(offer) {
                    s.lapsed += 1;
                }
                if accepted.contains(offer) {
                    let sal = |v: &[world::player::PlayerId]| -> f64 {
                        v.iter()
                            .filter_map(|id| w.players.get(id)?.contract.map(|c| c.salary))
                            .sum()
                    };
                    if give.len() + get.len() > 2 {
                        s.multi += 1;
                    }
                    if give.is_empty() || get.is_empty() {
                        s.one_sided += 1;
                    }
                    if (sal(give) - sal(get)).abs() >= DUMP {
                        s.dump += 1;
                    }
                }
            }
        }
        for v in &seats.violations[viol_at..] {
            if v.seat == "general manager" {
                *s.refused.entry(reason(&v.error)).or_insert(0) += 1;
            }
        }
        out.push(s);
    }
    out
}

#[test]
#[ignore = "needs data/internal/params.json; development aid"]
fn trade_volume() {
    let cal = calibration();
    let mut report = String::new();
    let mut totals = Vec::new();
    for preset in Preset::ALL {
        let runs: Vec<Vec<SeasonTrades>> = SEEDS
            .par_iter()
            .map(|&seed| {
                trade_stats(&cal, seed, &|t| {
                    SurplusGm::new(
                        seed ^ (u64::from(t) << 32),
                        preset.profile(Personality::BALANCED),
                    )
                })
            })
            .collect();
        let n = (SEEDS.len() * SEASONS) as f64;
        let mut sum = SeasonTrades::default();
        let mut per_year = [0usize; SEASONS];
        for run in &runs {
            for (y, s) in run.iter().enumerate() {
                sum.proposed += s.proposed;
                sum.accepted += s.accepted;
                sum.declined += s.declined;
                sum.lapsed += s.lapsed;
                sum.multi += s.multi;
                sum.dump += s.dump;
                sum.one_sided += s.one_sided;
                per_year[y] += s.accepted;
                for (k, v) in &s.refused {
                    *sum.refused.entry(k).or_insert(0) += v;
                }
            }
        }
        report.push_str(&format!(
            "{preset:?}: per season proposed {:.1} accepted {:.1} (per club {:.2}); declined {:.1} lapsed {:.1}; refused {:?}\n",
            sum.proposed as f64 / n,
            sum.accepted as f64 / n,
            sum.accepted as f64 / n / 30.0,
            sum.declined as f64 / n,
            sum.lapsed as f64 / n,
            sum.refused,
        ));
        let acc = sum.accepted.max(1) as f64;
        report.push_str(&format!(
            "  of accepted: multi-player {:.0}%, salary relief (>= {DUMP}M net) {:.0}%, one-sided {:.0}%\n",
            100.0 * sum.multi as f64 / acc,
            100.0 * sum.dump as f64 / acc,
            100.0 * sum.one_sided as f64 / acc,
        ));
        let years: Vec<String> = per_year
            .iter()
            .map(|c| format!("{:.1}", *c as f64 / SEEDS.len() as f64))
            .collect();
        report.push_str(&format!(
            "  accepted per league-year (mean of seeds): {}\n",
            years.join(" ")
        ));
        totals.push((preset, sum.accepted as f64 / n));
    }
    println!("{report}");
    for (p, a) in totals {
        println!("TOTAL {p:?} accepted per league-season {a:.1}");
    }
}

/// Wraps a general manager and records how each offer put to it looked.
struct Diag {
    gm: SurplusGm,
    profile: gm::Profile,
    seed: u64,
    log: std::sync::Arc<std::sync::Mutex<Vec<gm::trade::Verdict>>>,
}

impl connect::GmAgent for Diag {
    fn trades(&self) -> bool {
        true
    }

    fn act(&mut self, turn: &connect::GmTurn) -> Vec<connect::GmAction> {
        if let Some(t) = &turn.obs.trade {
            for o in &t.incoming {
                let v = gm::trade::judge(&turn.obs, t, o, &self.profile, self.seed);
                self.log.lock().expect("log").push(v);
            }
        }
        self.gm.act(turn)
    }
}

/// Why offers are declined, by preset (development aid).
#[test]
#[ignore = "development aid"]
fn trade_verdicts() {
    let cal = calibration();
    for preset in Preset::ALL {
        let log = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let seed = 7;
        let mut w = World::new(
            seed,
            cal.params,
            Model::from_calibration(&cal),
            DEFAULT_POOL,
        );
        let mut seats = Seats::fill(
            Box::new(BuiltInCommissioner {
                burn_in_years: BURN_IN,
            }),
            |_| Box::new(BuiltInOwner),
            |t| {
                let profile = preset.profile(Personality::BALANCED);
                let s = seed ^ (u64::from(t) << 32);
                Box::new(Diag {
                    gm: SurplusGm::new(s, profile),
                    profile,
                    seed: s,
                    log: log.clone(),
                })
            },
            |_| Box::new(BuiltInManager::default()),
        );
        let mut events = Vec::new();
        start_with(&mut w, &mut seats, &mut events).expect("found");
        for _ in 0..4 {
            run_year_with(&mut w, &mut seats, &mut events).expect("a year");
        }
        let p = preset.profile(Personality::BALANCED);
        let bar = p.personality.trade_margin - p.accept_slack();
        let v = log.lock().expect("log");
        let mut c: BTreeMap<&str, usize> = BTreeMap::new();
        let mut gains = Vec::new();
        for x in v.iter() {
            let k = match x {
                gm::trade::Verdict::Stale => "stale",
                gm::trade::Verdict::Roster => "roster",
                gm::trade::Verdict::Payroll => "payroll",
                gm::trade::Verdict::Weaker => "weaker now",
                gm::trade::Verdict::Gain(g) => {
                    gains.push(*g);
                    if *g > bar {
                        "gain ok"
                    } else if *g > 0.0 {
                        "gain small"
                    } else {
                        "gain negative"
                    }
                }
            };
            *c.entry(k).or_insert(0) += 1;
        }
        gains.sort_by(f64::total_cmp);
        let q = |f: f64| {
            gains
                .get((f * (gains.len().max(1) - 1) as f64) as usize)
                .copied()
                .unwrap_or(0.0)
        };
        println!(
            "{preset:?}: {c:?} gain quartiles {:.1} {:.1} {:.1} (bar {bar:.2})",
            q(0.25),
            q(0.5),
            q(0.75)
        );
    }
}

/// What the tested clubs of the evaluation ladder get from their trades with
/// the reference clubs, in true terms (development aid).
#[test]
#[ignore = "development aid"]
fn ladder_trades() {
    let cal = calibration();
    for preset in [Preset::Rookie, Preset::HallOfFame] {
        let rows: Vec<(usize, f64, f64, f64, f64)> = SEEDS
            .par_iter()
            .map(|&seed| {
                let mut w =
                    World::new(seed, cal.params, Model::from_calibration(&cal), DEFAULT_POOL);
                let mut seats = Seats::fill(
                    Box::new(BuiltInCommissioner {
                        burn_in_years: BURN_IN,
                    }),
                    |_| Box::new(BuiltInOwner),
                    |t| -> Box<dyn connect::GmAgent> {
                        if t % 5 == 0 {
                            Box::new(SurplusGm::new(
                                seed ^ (u64::from(t) << 32),
                                preset.profile(Personality::BALANCED),
                            ))
                        } else {
                            Box::new(gm::bots::ReferenceGm)
                        }
                    },
                    |_| Box::new(BuiltInManager::default()),
                );
                let mut log = Vec::new();
                start_with(&mut w, &mut seats, &mut log).expect("found");
                let (mut n, mut now, mut pay, mut q, mut players) = (0, 0.0, 0.0, 0.0, 0.0);
                for _ in 0..SEASONS {
                    let at = log.len();
                    run_year_with(&mut w, &mut seats, &mut log).expect("a year");
                    let season = &log[at..];
                    for e in season {
                        let Event::TradeProposed {
                            offer,
                            from,
                            to,
                            give,
                            get,
                        } = e
                        else {
                            continue;
                        };
                        if !season.iter().any(|x| {
                            matches!(x, Event::TradeAnswered { offer: o, accepted: true } if o == offer)
                        }) {
                            continue;
                        }
                        // Sign: + is what the tested club gains.
                        let sign = if from % 5 == 0 {
                            1.0
                        } else if to % 5 == 0 {
                            -1.0
                        } else {
                            continue;
                        };
                        let f = |v: &[world::player::PlayerId], g: &dyn Fn(&world::player::Player) -> f64| -> f64 {
                            v.iter().filter_map(|id| w.players.get(id)).map(g).sum()
                        };
                        let wins = |p: &world::player::Player| {
                            w.rules.war_price * world::scout::war_of(p.kind, p.quality).max(0.0)
                        };
                        let sal = |p: &world::player::Player| p.contract.map_or(0.0, |c| c.salary);
                        let qual = |p: &world::player::Player| p.quality;
                        n += 1;
                        now += sign * (f(get, &wins) - f(give, &wins));
                        pay += sign * (f(get, &sal) - f(give, &sal));
                        q += sign * (f(get, &qual) - f(give, &qual));
                        players += (give.len() + get.len()) as f64;
                    }
                }
                (n, now, pay, q, players)
            })
            .collect();
        let n: usize = rows.iter().map(|r| r.0).sum();
        let s = |i: usize| -> f64 {
            rows.iter()
                .map(|r| match i {
                    1 => r.1,
                    2 => r.2,
                    3 => r.3,
                    _ => r.4,
                })
                .sum::<f64>()
                / n.max(1) as f64
        };
        println!(
            "{preset:?} in ladder: {:.1} trades/season by 6 clubs; per trade: true wins value now {:+.2}M, salary {:+.2}M, sum quality {:+.4}, players {:.1}",
            n as f64 / (SEEDS.len() * SEASONS) as f64,
            s(1),
            s(2),
            s(3),
            s(4)
        );
    }
}
