//! Phase 4b exit gates (ignored by default: they need the git-ignored
//! `data/internal/params.json` and take a while):
//!
//!     cargo test --release -p gm --test ladder -- --ignored --nocapture
//!
//! The evaluation ladder plays full leagues in which six clubs (every fifth)
//! are run by the general manager under test and the other 24 by the reference
//! general manager (the built-in one, which also answers trade offers by a plain rule); a general manager's score is the win-percentage
//! gap between its clubs and the reference clubs, season by season.
//!
//! * `ai_beats_reference`: the Hall of Fame preset beats the reference with
//!   z >= 2.
//! * `presets_rank_in_order`: scores rise Rookie < Veteran < All-Star <
//!   Hall of Fame, and Hall of Fame beats Rookie with z >= 2.
//! * `exploit_bots_gain_no_lasting_edge`: over 50 seasons on five seeds, in
//!   seasons 26-50 no listed exploit bot beats the reference with z >= 2 or by
//!   more than 0.015 win percentage. The standard error is clustered by league
//!   run (one mean per seed), because seasons within a run are autocorrelated.
//!
//! Thresholds are fixed here before any run and are not edited to pass.

use std::path::Path;

use calibration::Calibration;
use connect::{
    BuiltInCommissioner, BuiltInManager, BuiltInOwner, GmAgent, Seats, run_year_with, start_with,
};
use gm::bots::{Hoarder, ReferenceGm, Stretcher, Tanker, TradeSpammer};
use gm::{Personality, Preset, SurplusGm};
use protocol::Actor;
use rayon::prelude::*;
use world::command::Command;
use world::player::DEFAULT_POOL;
use world::rating::Model;
use world::state::World;

const SEEDS: [u64; 5] = [20_250_101, 1, 2, 3, 4];
const BURN_IN: u8 = 10;
const PRESET_YEARS: usize = 20;
const BOT_SEEDS: usize = 5;
const BOT_YEARS: usize = 50;
const Z_REQUIRED: f64 = 2.0;
const BOT_MAX_EDGE: f64 = 0.015;

fn calibration() -> Calibration {
    Calibration::load(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/internal/params.json"),
    )
    .unwrap_or_else(|e| panic!("{e}"))
}

fn tested(team: u8) -> bool {
    team.is_multiple_of(5)
}

/// Per season: mean win% of the tested clubs minus the mean of the others.
fn gaps(
    cal: &Calibration,
    seed: u64,
    years: usize,
    make: &(dyn Fn(u8) -> Box<dyn GmAgent> + Sync),
) -> Vec<f64> {
    let mut w = World::new(seed, cal.params, Model::from_calibration(cal), DEFAULT_POOL);
    let mut seats = Seats::fill(
        Box::new(BuiltInCommissioner {
            burn_in_years: BURN_IN,
        }),
        |_| Box::new(BuiltInOwner),
        |t| {
            if tested(t) {
                make(t)
            } else {
                Box::new(ReferenceGm)
            }
        },
        |_| Box::new(BuiltInManager::default()),
    );
    let mut log = Vec::new();
    start_with(&mut w, &mut seats, &mut log).expect("found");
    let mut out = Vec::new();
    for _ in 0..years {
        run_year_with(&mut w, &mut seats, &mut log).expect("a year");
        let rec = &w.history.last().expect("season").records;
        let (mut a, mut b, mut na, mut nb) = (0.0, 0.0, 0.0, 0.0);
        for (t, r) in rec.iter().enumerate() {
            let p = f64::from(r.w) / f64::from(r.w + r.l);
            if tested(t as u8) {
                a += p;
                na += 1.0;
            } else {
                b += p;
                nb += 1.0;
            }
        }
        out.push(a / na - b / nb);
    }
    out
}

fn mean_se(v: &[f64]) -> (f64, f64) {
    let n = v.len() as f64;
    let m = v.iter().sum::<f64>() / n;
    let var = v.iter().map(|x| (x - m).powi(2)).sum::<f64>() / (n - 1.0);
    (m, (var / n).sqrt())
}

/// Mean over all seasons and its standard error clustered by league run:
/// seasons within one run are autocorrelated, so the run means are the
/// independent observations.
fn clustered_mean_se(runs: &[Vec<f64>]) -> (f64, f64) {
    let means: Vec<f64> = runs
        .iter()
        .map(|r| r.iter().sum::<f64>() / r.len() as f64)
        .collect();
    mean_se(&means)
}

fn preset_scores(cal: &Calibration, preset: Preset, personality: Personality) -> Vec<f64> {
    SEEDS
        .par_iter()
        .map(|&seed| {
            gaps(cal, seed, PRESET_YEARS, &|t| {
                Box::new(SurplusGm::new(
                    seed ^ (u64::from(t) << 32),
                    preset.profile(personality),
                ))
            })
        })
        .flatten()
        .collect()
}

#[test]
#[ignore = "needs data/internal/params.json"]
fn ai_beats_reference() {
    let cal = calibration();
    let v = preset_scores(&cal, Preset::HallOfFame, Personality::BALANCED);
    let (m, se) = mean_se(&v);
    println!(
        "Hall of Fame vs reference: {m:+.4} (se {se:.4}, z {:.2}, n {})",
        m / se,
        v.len()
    );
    assert!(m / se >= Z_REQUIRED, "z {:.2}", m / se);
}

#[test]
#[ignore = "needs data/internal/params.json"]
fn presets_rank_in_order() {
    let cal = calibration();
    let scores: Vec<(Preset, Vec<f64>)> = Preset::ALL
        .iter()
        .map(|&p| (p, preset_scores(&cal, p, Personality::BALANCED)))
        .collect();
    let mut last = f64::NEG_INFINITY;
    for (p, v) in &scores {
        let (m, se) = mean_se(v);
        println!("{p:?}: {m:+.4} (se {se:.4})");
        assert!(m > last, "{p:?} does not beat the preset below it");
        last = m;
    }
    let (lo, hi) = (&scores[0].1, &scores[3].1);
    let diff: Vec<f64> = hi.iter().zip(lo).map(|(a, b)| a - b).collect();
    let (m, se) = mean_se(&diff);
    println!("Hall of Fame minus Rookie: {m:+.4} (z {:.2})", m / se);
    assert!(m / se >= Z_REQUIRED);
}

#[test]
#[ignore = "needs data/internal/params.json"]
fn exploit_bots_gain_no_lasting_edge() {
    let cal = calibration();
    type Maker = fn() -> Box<dyn GmAgent>;
    let bots: [(&str, Maker); 4] = [
        ("hoarder", || Box::new(Hoarder)),
        ("tanker", || Box::new(Tanker)),
        ("stretcher", || Box::new(Stretcher)),
        ("trade spammer", || Box::new(TradeSpammer)),
    ];
    let mut failures = Vec::new();
    for (name, make) in bots {
        let late: Vec<Vec<f64>> = SEEDS[..BOT_SEEDS]
            .par_iter()
            .map(|&seed| {
                let g = gaps(&cal, seed, BOT_YEARS, &|_| make());
                g[BOT_YEARS / 2..].to_vec()
            })
            .collect();
        let (m, se) = clustered_mean_se(&late);
        println!(
            "{name}: seasons 26-50 {m:+.4} (se {se:.4}, z {:.2})",
            m / se
        );
        if m / se >= Z_REQUIRED || m > BOT_MAX_EDGE {
            failures.push(name);
        }
    }
    assert!(failures.is_empty(), "lasting edge: {failures:?}");
}

/// Quick look at one seed, for development: `-- --ignored smoke --nocapture`.
#[test]
#[ignore = "development aid"]
fn smoke() {
    let cal = calibration();
    for preset in Preset::ALL {
        let g = gaps(&cal, 1, 6, &|t| {
            Box::new(SurplusGm::new(
                u64::from(t),
                preset.profile(Personality::BALANCED),
            ))
        });
        let (m, se) = mean_se(&g);
        println!("{preset:?}: {m:+.4} (se {se:.4})");
    }
    let _ = Actor::commissioner();
    let _ = Command::PlaySeason;
}

#[test]
#[ignore = "development aid"]
fn bot_smoke() {
    let cal = calibration();
    type Maker = fn() -> Box<dyn GmAgent>;
    let bots: [(&str, Maker); 4] = [
        ("hoarder", || Box::new(Hoarder)),
        ("tanker", || Box::new(Tanker)),
        ("stretcher", || Box::new(Stretcher)),
        ("trade spammer", || Box::new(TradeSpammer)),
    ];
    for (name, make) in bots {
        let r = std::panic::catch_unwind(|| gaps(&cal, 20_250_101, 50, &|_| make()));
        println!("{name}: {}", if r.is_ok() { "ok" } else { "PANIC" });
    }
}

#[test]
#[ignore = "needs data/internal/params.json"]
fn trading_happens() {
    let cal = calibration();
    let mut w = World::new(1, cal.params, Model::from_calibration(&cal), DEFAULT_POOL);
    let mut seats = Seats::fill(
        Box::new(BuiltInCommissioner {
            burn_in_years: BURN_IN,
        }),
        |_| Box::new(BuiltInOwner),
        |t| {
            if tested(t) {
                Box::new(SurplusGm::new(
                    u64::from(t),
                    Preset::HallOfFame.profile(Personality::BALANCED),
                ))
            } else {
                Box::new(ReferenceGm)
            }
        },
        |_| Box::new(BuiltInManager::default()),
    );
    let mut log = Vec::new();
    start_with(&mut w, &mut seats, &mut log).unwrap();
    for _ in 0..5 {
        run_year_with(&mut w, &mut seats, &mut log).unwrap();
    }
    let n = |f: fn(&world::command::Event) -> bool| log.iter().filter(|e| f(e)).count();
    println!(
        "proposed {} accepted {}",
        n(|e| matches!(e, world::command::Event::TradeProposed { .. })),
        n(|e| matches!(
            e,
            world::command::Event::TradeAnswered { accepted: true, .. }
        ))
    );
}

#[test]
#[ignore = "development aid"]
fn mechanism() {
    let cal = calibration();
    type Maker = fn() -> Box<dyn GmAgent>;
    let bots: [(&str, Maker); 3] = [
        ("reference", || Box::new(ReferenceGm)),
        ("hoarder", || Box::new(Hoarder)),
        ("stretcher", || Box::new(Stretcher)),
    ];
    for (name, make) in bots {
        let mut w = World::new(1, cal.params, Model::from_calibration(&cal), DEFAULT_POOL);
        let mut seats = Seats::fill(
            Box::new(BuiltInCommissioner {
                burn_in_years: BURN_IN,
            }),
            |_| Box::new(BuiltInOwner),
            |t| {
                if tested(t) {
                    make()
                } else {
                    Box::new(ReferenceGm)
                }
            },
            |_| Box::new(BuiltInManager::default()),
        );
        let mut log = Vec::new();
        start_with(&mut w, &mut seats, &mut log).unwrap();
        let (mut pay, mut bud, mut cash, mut n) = (0.0, 0.0, 0.0, 0.0);
        let (mut wt, mut wr) = (0.0, 0.0);
        let (mut rev, mut oth) = (0.0, 0.0);
        for y in 0..50 {
            run_year_with(&mut w, &mut seats, &mut log).unwrap();
            if y >= 25 {
                for t in 0..30 {
                    let r = &w.last_records[t];
                    let p = f64::from(r.w) / f64::from(r.w + r.l);
                    if tested(t as u8) {
                        wt += p / 6.0;
                    } else {
                        wr += p / 24.0;
                    }
                }
                for t in (0..30).filter(|t| tested(*t as u8)) {
                    let f = w.finances[t].last().unwrap();
                    rev += f.revenue;
                    oth += f.other;
                    pay += w.payroll(t);
                    bud += w.clubs[t].budget;
                    cash += w.clubs[t].cash;
                    n += 1.0;
                }
            }
        }
        let rescues = |f: fn(u8) -> bool| {
            log.iter()
                .filter(|e| matches!(e, world::command::Event::ClubRescued { team } if f(*team)))
                .count()
        };
        println!(
            "{name}: rescues tested {} others {}",
            rescues(tested),
            rescues(|t| !tested(t))
        );
        println!(
            "{name}: payroll {:.1} budget {:.1} cash {:.1} edge {:+.4}",
            pay / n,
            bud / n,
            cash / n,
            (wt - wr) / 25.0
        );
        println!("{name}: revenue {:.1} other {:.1}", rev / n, oth / n);
    }
}

/// Development aid: league-wide trade volume per season with every club run
/// by the same preset (the way a real game runs its AI clubs).
#[test]
#[ignore = "development aid"]
fn trade_volume() {
    use world::command::Event;
    let cal = calibration();
    for preset in Preset::ALL {
        let mut totals = vec![(0usize, 0usize); 5];
        let mut per_year: Vec<(usize, usize)> = Vec::new();
        for (i, seed) in [20_250_101_u64, 1, 2, 3, 4].into_iter().enumerate() {
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
                    Box::new(SurplusGm::new(
                        u64::from(t),
                        preset.profile(Personality::BALANCED),
                    ))
                },
                |_| Box::new(BuiltInManager::default()),
            );
            let mut log = Vec::new();
            start_with(&mut w, &mut seats, &mut log).unwrap();
            for y in 0..10 {
                let before = log.len();
                run_year_with(&mut w, &mut seats, &mut log).unwrap();
                let new = &log[before..];
                let prop = new
                    .iter()
                    .filter(|e| matches!(e, Event::TradeProposed { .. }))
                    .count();
                let acc = new
                    .iter()
                    .filter(|e| matches!(e, Event::TradeAnswered { accepted: true, .. }))
                    .count();
                totals[i].0 += prop;
                totals[i].1 += acc;
                if i == 0 {
                    per_year.push((prop, acc));
                }
                if y >= 5 {
                    println!("{preset:?} seed {seed} year {y}: proposed {prop} accepted {acc}");
                }
            }
        }
        println!("{preset:?} per-year (seed 1): {per_year:?}");
        println!("{preset:?} totals per seed over 10 years (proposed, accepted): {totals:?}");
    }
}
