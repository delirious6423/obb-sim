//! Phase 4a exit gate (ignored by default: needs the git-ignored
//! `data/internal/params.json`):
//!
//!     cargo test --release -p world --test manager_gate -- --ignored --nocapture
//!
//! `manager_gate`: the table-driven field manager beats the built-in baseline
//! manager. For each of five seeds and eight league years, the same season is
//! played from the same rosters three times: nobody uses tables, even-numbered
//! clubs do, odd-numbered clubs do. A club's edge is its win percentage with
//! tables minus its win percentage in the all-baseline season. The mean edge
//! over all club-seasons must be positive at z >= 2.

use std::path::Path;

use calibration::Calibration;
use protocol::{Actor, TeamId};
use world::command::{Command, Event};
use world::driver::run_year;
use world::player::DEFAULT_POOL;
use world::rating::Model;
use world::state::{Tactics, World};

const SEEDS: [u64; 5] = [20_250_101, 1, 2, 3, 4];
const BURN_IN: u8 = 10;
const YEARS: usize = 8;
const Z_REQUIRED: f64 = 2.0;

fn calibration() -> Calibration {
    Calibration::load(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/internal/params.json"),
    )
    .unwrap_or_else(|e| panic!("{e}"))
}

fn win_pcts(w: &World, use_tables: impl Fn(usize) -> bool) -> Vec<f64> {
    let mut w = w.clone();
    for t in 0..w.clubs.len() {
        if use_tables(t) {
            w.execute(
                &Actor::field_manager(TeamId(t as u32)),
                Command::SetTactics {
                    team: t as u8,
                    tactics: Tactics::Tables,
                },
            )
            .expect("set tactics");
        }
    }
    let events = w
        .execute(&Actor::commissioner(), Command::PlaySeason)
        .expect("season");
    match &events[0] {
        Event::SeasonPlayed { records, .. } => records
            .iter()
            .map(|r| f64::from(r.w) / f64::from(r.w + r.l))
            .collect(),
        e => panic!("unexpected {e:?}"),
    }
}

#[test]
#[ignore = "needs data/internal/params.json"]
fn manager_gate() {
    let cal = calibration();
    let mut edges: Vec<f64> = Vec::new();
    let mut controls: Vec<f64> = Vec::new();
    for seed in SEEDS {
        let mut w = World::new(
            seed,
            cal.params,
            Model::from_calibration(&cal),
            DEFAULT_POOL,
        );
        w.execute(
            &Actor::commissioner(),
            Command::Found {
                burn_in_years: BURN_IN,
            },
        )
        .expect("found");
        for _ in 0..YEARS {
            let base = win_pcts(&w, |_| false);
            let even = win_pcts(&w, |t| t % 2 == 0);
            let odd = win_pcts(&w, |t| t % 2 == 1);
            for t in 0..base.len() {
                let with = if t % 2 == 0 { even[t] } else { odd[t] };
                edges.push(with - base[t]);
                let without = if t % 2 == 0 { odd[t] } else { even[t] };
                controls.push(without - base[t]);
            }
            run_year(&mut w, &mut Vec::new()).expect("a year");
        }
    }
    let stat = |v: &[f64]| {
        let n = v.len() as f64;
        let m = v.iter().sum::<f64>() / n;
        let sd = (v.iter().map(|x| (x - m).powi(2)).sum::<f64>() / (n - 1.0)).sqrt();
        (m, sd / n.sqrt())
    };
    let (m, se) = stat(&edges);
    let (cm, cse) = stat(&controls);
    println!(
        "manager edge: {:+.4} win% per club-season (se {:.4}, z {:.2}, n {}); \
         clubs without tables facing tables: {:+.4} (se {:.4})",
        m,
        se,
        m / se,
        edges.len(),
        cm,
        cse
    );
    assert!(m / se >= Z_REQUIRED, "z {:.2} < {Z_REQUIRED}", m / se);
}
