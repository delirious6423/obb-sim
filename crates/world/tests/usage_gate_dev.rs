//! Dev aid (not a gate): paired seasons, like `manager_gate`, comparing the
//! automatic playing-time setup (the league's modern customs) with the
//! classic role preset (`Usage::roles`) and with the old fixed behaviour
//! (`Usage::LEGACY`). Each season is played from the same rosters with
//! every club on the automatic setup, then with even (or odd) clubs on the
//! other setup. A positive edge means the other setup wins more than the
//! automatic one.
//!
//!     SEEDS="1 2" YEARS=4 cargo test --release -p world --test usage_gate_dev -- --ignored --nocapture

use std::path::Path;

use calibration::Calibration;
use protocol::{Actor, TeamId};
use world::command::{Command, Event};
use world::driver::run_year;
use world::player::DEFAULT_POOL;
use world::rating::Model;
use world::state::{Usage, World};

fn env_list(name: &str, default: &str) -> Vec<u64> {
    std::env::var(name)
        .unwrap_or_else(|_| default.into())
        .split_whitespace()
        .map(|s| s.parse().expect("number"))
        .collect()
}

fn win_pcts(w: &World, usage: &Usage, set: impl Fn(usize) -> bool) -> Vec<f64> {
    let mut w = w.clone();
    for t in 0..w.clubs.len() {
        if set(t) {
            w.execute(
                &Actor::field_manager(TeamId(t as u32)),
                Command::SetUsage {
                    team: t as u8,
                    usage: Some(usage.clone()),
                },
            )
            .expect("set usage");
        }
    }
    let events = w
        .execute(&Actor::commissioner(), Command::PlaySeason)
        .expect("season");
    match events
        .iter()
        .find(|e| matches!(e, Event::SeasonPlayed { .. }))
    {
        Some(Event::SeasonPlayed { records, .. }) => records
            .iter()
            .map(|r| f64::from(r.w) / f64::from(r.w + r.l))
            .collect(),
        _ => panic!("no SeasonPlayed event"),
    }
}

fn stat(v: &[f64]) -> (f64, f64) {
    let n = v.len() as f64;
    let m = v.iter().sum::<f64>() / n;
    let var = v.iter().map(|x| (x - m) * (x - m)).sum::<f64>() / (n - 1.0);
    (m, libm::sqrt(var / n))
}

#[test]
#[ignore = "dev aid; needs data/internal/params.json"]
fn usage_gate_dev() {
    let cal = Calibration::load(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/internal/params.json"),
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let years = env_list("YEARS", "4")[0] as usize;
    let others = [("roles preset", Usage::roles()), ("legacy", Usage::LEGACY)];
    let mut edges: Vec<Vec<f64>> = vec![Vec::new(); others.len()];
    for seed in env_list("SEEDS", "1 2") {
        let mut w = World::new(
            seed,
            cal.params,
            Model::from_calibration(&cal),
            DEFAULT_POOL,
        );
        w.execute(&Actor::commissioner(), Command::Found { burn_in_years: 10 })
            .expect("found");
        for _ in 0..years {
            let base = win_pcts(&w, &Usage::MODERN, |_| false);
            for (k, (_, usage)) in others.iter().enumerate() {
                let even = win_pcts(&w, usage, |t| t % 2 == 0);
                let odd = win_pcts(&w, usage, |t| t % 2 == 1);
                for t in 0..base.len() {
                    let with = if t % 2 == 0 { even[t] } else { odd[t] };
                    edges[k].push(with - base[t]);
                }
            }
            run_year(&mut w, &mut Vec::new()).expect("a year");
        }
    }
    for (k, (name, _)) in others.iter().enumerate() {
        let (m, se) = stat(&edges[k]);
        println!(
            "{name} vs automatic: {m:+.4} win% per club-season (se {se:.4}, z {:.2}, n {})",
            m / se,
            edges[k].len()
        );
    }
}
