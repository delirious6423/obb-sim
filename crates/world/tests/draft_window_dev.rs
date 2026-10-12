//! Dev aid (not a gate): the draft buckets measured two ways.
//!
//! - "complete": as `draft_gate` measures them (classes 15+ years old, whole
//!   careers).
//! - "window": as the source studies measured them. FanGraphs looked at the
//!   2000-2010 drafts with data through 2015 and Conforti et al. at the
//!   2005-2015 drafts, so each class was seen 5 to 15 years after its draft.
//!   Here each class is cut 5 + (draft year mod 11) years after its draft.
//!
//! Bust uses seasons with playing time (the gate's rule) for "complete", and
//! also, for "window", WAR per 500 AB / 25 G (the FanGraphs table header).
//! Also prints Baseball America's pick ranges (1965-2020 study).
//!
//!     SEEDS="1 2" cargo test --release -p world --test draft_window_dev -- --ignored --nocapture

use std::collections::BTreeMap;
use std::path::Path;

use calibration::Calibration;
use protocol::Actor;
use world::command::{Command, Event, Line};
use world::driver::run_year;
use world::player::{DEFAULT_POOL, Origin};
use world::rating::Model;
use world::state::World;

/// Per season with playing time: (year, WAR, seasons of 500 AB or 25 G).
type History = BTreeMap<u32, Vec<(i32, f64, f64)>>;

struct Row {
    pick: u16,
    signed: bool,
    full: (bool, f64, u16),
    window: (bool, f64, u16, f64),
}

fn report(seed: u64, rows: &[Row]) {
    for (name, lo, hi) in [
        ("1-10", 1, 10),
        ("11-30", 11, 30),
        ("31-90", 31, 90),
        ("91-300", 91, 300),
    ] {
        let all: Vec<&Row> = rows
            .iter()
            .filter(|r| (lo..=hi).contains(&r.pick))
            .collect();
        let signed: Vec<&&Row> = all.iter().filter(|r| r.signed).collect();
        let n = signed.len().max(1) as f64;
        let war = |f: &dyn Fn(&Row) -> f64| {
            all.iter().map(|r| f(r)).sum::<f64>() / all.len().max(1) as f64
        };
        let share = |f: &dyn Fn(&Row) -> bool| signed.iter().filter(|r| f(r)).count() as f64 / n;
        println!(
            "seed {seed} {name:<6} complete: reach {:.3} WAR {:5.2} bust {:.3} | window: reach {:.3} WAR {:5.2} bust {:.3} (per 500 AB/25 G {:.3})",
            share(&|r| r.full.0),
            war(&|r| r.full.1),
            share(&|r| !r.full.0 || r.full.1 / f64::from(r.full.2) < 1.5),
            share(&|r| r.window.0),
            war(&|r| r.window.1),
            share(&|r| !r.window.0 || r.window.1 / f64::from(r.window.2) < 1.5),
            share(&|r| !r.window.0 || r.window.1 / r.window.3.max(1e-9) < 1.5),
        );
    }
    let ranges: [(u16, u16, f64); 6] = [
        (1, 10, 9.8),
        (11, 30, 4.7),
        (31, 50, 2.5),
        (51, 100, 1.7),
        (101, 150, 0.9),
        (151, 200, 0.7),
    ];
    let line: Vec<String> = ranges
        .iter()
        .map(|&(lo, hi, ba)| {
            let s: Vec<&Row> = rows
                .iter()
                .filter(|r| r.signed && (lo..=hi).contains(&r.pick))
                .collect();
            let m = s.iter().map(|r| r.full.1).sum::<f64>() / s.len().max(1) as f64;
            format!("{lo}-{hi} {m:.1} (BA {ba})")
        })
        .collect();
    println!(
        "seed {seed} complete WAR per signed player: {}",
        line.join(", ")
    );
}

#[test]
#[ignore = "dev aid; needs data/internal/params.json"]
fn draft_window_dev() {
    let cal = Calibration::load(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/internal/params.json"),
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let seeds: Vec<u64> = std::env::var("SEEDS")
        .unwrap_or_else(|_| "1 2".into())
        .split_whitespace()
        .map(|s| s.parse().expect("seed"))
        .collect();
    for seed in seeds {
        let model = Model::from_calibration(&cal);
        let mut w = World::new(seed, cal.params, model, DEFAULT_POOL);
        w.execute(&Actor::commissioner(), Command::Found { burn_in_years: 30 })
            .expect("found");
        let first_year = w.year;
        let mut history: History = BTreeMap::new();
        for _ in 0..50 {
            let mut log = Vec::new();
            let year = w.year;
            run_year(&mut w, &mut log).expect("a year");
            for e in &log {
                let Event::SeasonPlayed { lines, .. } = e else {
                    continue;
                };
                for l in lines {
                    let eq = match &l.line {
                        Line::Bat(b) if b.pa > 0 => f64::from(b.ab) / 500.0,
                        Line::Pitch(p) if p.bf > 0 => f64::from(p.g) / 25.0,
                        _ => continue,
                    };
                    history
                        .entry(l.player.0)
                        .or_default()
                        .push((year, l.war, eq));
                }
            }
        }
        let mut rows = Vec::new();
        let retired = w.retired.values().map(|(p, y)| (p, Some(*y)));
        for (p, left) in w.players.values().map(|p| (p, None)).chain(retired) {
            let Some(d) = p.drafted else { continue };
            if p.origin == Origin::International || d.pick > 300 {
                continue;
            }
            if d.year <= first_year || d.year > w.year - 15 {
                continue;
            }
            let cut = d.year + 5 + d.year.rem_euclid(11);
            let h = history.get(&p.id.0).map_or(&[][..], Vec::as_slice);
            let inw: Vec<_> = h.iter().filter(|x| x.0 <= cut).collect();
            rows.push(Row {
                pick: d.pick,
                signed: left != Some(d.year),
                full: (p.career.seasons > 0, p.career.war, p.career.seasons),
                window: (
                    !inw.is_empty(),
                    inw.iter().map(|x| x.1).sum(),
                    inw.len() as u16,
                    inw.iter().map(|x| x.2).sum(),
                ),
            });
        }
        report(seed, &rows);
    }
}
