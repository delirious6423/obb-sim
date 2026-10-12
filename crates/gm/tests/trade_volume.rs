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
}

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
        for e in season {
            if let Event::TradeProposed { offer, .. } = e {
                s.proposed += 1;
                if !answered.contains(offer) {
                    s.lapsed += 1;
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

/// Quality of trades by difficulty: how much value a club gives away per
/// accepted trade. Development aid for the easy-versus-hard check.
#[test]
#[ignore = "development aid"]
fn trade_value_by_preset() {
    let cal = calibration();
    for preset in Preset::ALL {
        let n: usize = SEEDS
            .par_iter()
            .map(|&seed| {
                trade_stats(&cal, seed, &|t| {
                    SurplusGm::new(
                        seed ^ (u64::from(t) << 32),
                        preset.profile(Personality::BALANCED),
                    )
                })
                .iter()
                .map(|s| s.accepted)
                .sum::<usize>()
            })
            .sum();
        println!("{preset:?}: accepted total {n}");
    }
}
