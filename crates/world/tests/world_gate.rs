//! Phase 3 exit gate. Ignored by default (needs the git-ignored
//! `data/internal/params.json`, and takes a few minutes):
//!
//!     cargo test --release -p world --test world_gate -- --ignored --nocapture
//!
//! Fifty seasons with fixed-behaviour owners and GMs must show
//!  * no talent inflation or collapse: league K%, BB%, HR% and runs per
//!    team-game stay near the real league and do not drift between the first
//!    and last decade, and the average major leaguer stays near league average;
//!  * no insolvent club;
//!  * draft-slot value curves in the real-world range: chance of reaching the
//!    majors, career WAR and bust rate by pick bucket.
//!
//! The bands below were fixed before the first run of this gate. The draft
//! bands are rough ranges from general knowledge of published draft studies
//! (no data was downloaded for them); they are assumptions, listed under
//! "Known gaps" in PLAN.md.

use std::path::Path;

use calibration::Calibration;
use protocol::Actor;
use world::command::Command;
use world::driver::run_year;
use world::player::{DEFAULT_POOL, Level, Origin};
use world::rating::{Kind, Model};
use world::state::World;

const SEASONS: usize = 50;
const BURN_IN: u8 = 12;
const SEED: u64 = 20_250_101;

/// Season means in the 50-season mean must be within this of the real league.
const RATE_BAND: f64 = 0.15;
const RUNS_BAND: f64 = 0.10;
/// First-decade vs last-decade means.
const DRIFT_RATE: f64 = 0.05;
const DRIFT_RUNS: f64 = 0.06;
const DRIFT_QUALITY_SD: f64 = 0.15;
const QUALITY_BAND_SD: f64 = 0.35;

type Metric = fn(&Season) -> f64;

struct Season {
    k: f64,
    bb: f64,
    hr: f64,
    runs: f64,
    q: [f64; 2],
}

fn mean<T>(v: &[T], f: impl Fn(&T) -> f64) -> f64 {
    v.iter().map(f).sum::<f64>() / v.len() as f64
}

struct Bucket {
    name: &'static str,
    picks: std::ops::RangeInclusive<u16>,
    debut: (f64, f64),
    war: (f64, f64),
    bust_max: Option<f64>,
    bust_min: Option<f64>,
}

const BUCKETS: [Bucket; 4] = [
    Bucket {
        name: "picks 1-10",
        picks: 1..=10,
        debut: (0.60, 0.95),
        war: (6.0, 30.0),
        bust_max: Some(0.45),
        bust_min: None,
    },
    Bucket {
        name: "picks 11-30",
        picks: 11..=30,
        debut: (0.40, 0.80),
        war: (2.0, 15.0),
        bust_max: None,
        bust_min: None,
    },
    Bucket {
        name: "picks 31-90",
        picks: 31..=90,
        debut: (0.20, 0.60),
        war: (0.5, 7.0),
        bust_max: None,
        bust_min: None,
    },
    Bucket {
        name: "picks 91-300",
        picks: 91..=300,
        debut: (0.03, 0.35),
        war: (0.0, 3.0),
        bust_max: None,
        bust_min: Some(0.75),
    },
];

#[test]
#[ignore = "needs data/internal/params.json; run with --release -- --ignored"]
fn world_gate() {
    let cal = Calibration::load(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/internal/params.json"),
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let league = cal.params.league.0;
    let runs_target = cal.run_environment.runs_per_team_game_dh;
    let model = Model::from_calibration(&cal);
    let seed = std::env::var("GATE_SEED")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(SEED);
    let mut w = World::new(seed, cal.params, model, DEFAULT_POOL);
    w.execute(
        &Actor::commissioner(),
        Command::Found {
            burn_in_years: BURN_IN,
        },
    )
    .expect("found");
    let first_year = w.year;
    let mut seasons = Vec::new();
    let mut lowest_cash = f64::MAX;
    for _ in 0..SEASONS {
        run_year(&mut w, &mut Vec::new()).expect("a year");
        let l = w.history.last().expect("season").league;
        let mut q = [0.0; 2];
        let mut n = [0.0; 2];
        for p in w.players.values().filter(|p| p.level == Level::Majors) {
            let k = usize::from(p.kind == Kind::Pitcher);
            q[k] += p.quality / w.model.quality_sd(p.kind);
            n[k] += 1.0;
        }
        for c in &w.clubs {
            lowest_cash = lowest_cash.min(c.cash);
        }
        let pa = f64::from(l.pa);
        seasons.push(Season {
            k: f64::from(l.so) / pa,
            bb: f64::from(l.bb) / pa,
            hr: f64::from(l.hr) / pa,
            runs: f64::from(l.runs) / f64::from(l.games),
            q: [q[0] / n[0], q[1] / n[1]],
        });
    }

    let all = |f: fn(&Season) -> f64| mean(&seasons, f);
    let decade = |range: std::ops::Range<usize>, f: fn(&Season) -> f64| mean(&seasons[range], f);
    let mut failures: Vec<String> = Vec::new();
    let mut check = |name: &str, value: f64, lo: f64, hi: f64| {
        let ok = (lo..=hi).contains(&value);
        println!(
            "{name:<44} {value:>9.4}  [{lo:.4}, {hi:.4}]  {}",
            if ok { "PASS" } else { "FAIL" }
        );
        if !ok {
            failures.push(format!("{name}: {value:.4} outside [{lo:.4}, {hi:.4}]"));
        }
    };

    println!("{SEASONS} seasons from {first_year}");
    let rates: [(&str, Metric, f64); 3] = [
        ("K%", |s| s.k, league[0]),
        ("BB%", |s| s.bb, league[1]),
        ("HR/PA", |s| s.hr, league[6]),
    ];
    for (name, f, target) in rates {
        let m = all(f);
        check(
            &format!("{name} mean vs real"),
            m,
            target * (1.0 - RATE_BAND),
            target * (1.0 + RATE_BAND),
        );
        let (a, b) = (decade(0..10, f), decade(SEASONS - 10..SEASONS, f));
        check(
            &format!("{name} last decade / first decade"),
            b / a,
            1.0 - DRIFT_RATE,
            1.0 + DRIFT_RATE,
        );
    }
    check(
        "runs/team-game mean vs real",
        all(|s| s.runs),
        runs_target * (1.0 - RUNS_BAND),
        runs_target * (1.0 + RUNS_BAND),
    );
    let (a, b) = (
        decade(0..10, |s| s.runs),
        decade(SEASONS - 10..SEASONS, |s| s.runs),
    );
    check(
        "runs last decade / first decade",
        b / a,
        1.0 - DRIFT_RUNS,
        1.0 + DRIFT_RUNS,
    );
    for (i, name) in ["batter", "pitcher"].into_iter().enumerate() {
        let get = move |s: &Season| s.q[i];
        let (a, b) = (
            mean(&seasons[0..10], get),
            mean(&seasons[SEASONS - 10..], get),
        );
        check(
            &format!("major-league {name} quality drift (sd)"),
            b - a,
            -DRIFT_QUALITY_SD,
            DRIFT_QUALITY_SD,
        );
        for (j, s) in seasons.iter().enumerate() {
            if s.q[i].abs() > QUALITY_BAND_SD || j % 10 == 0 {
                check(
                    &format!("{name} quality, season {j} (sd)"),
                    s.q[i],
                    -QUALITY_BAND_SD,
                    QUALITY_BAND_SD,
                );
            }
        }
    }
    check(
        "lowest club cash, ever (millions)",
        lowest_cash,
        0.0,
        f64::MAX,
    );

    // Draft classes old enough to have played out their careers.
    let mut people = Vec::new();
    for p in w.players.values().chain(w.retired.values().map(|(p, _)| p)) {
        let Some(d) = p.drafted else { continue };
        if p.origin == Origin::International || d.pick > 300 {
            continue;
        }
        if d.year > first_year && d.year <= w.year - 15 {
            people.push((d.pick, p.career.seasons > 0, p.career.war));
        }
    }
    println!("{} drafted players measured", people.len());
    let mut last = (1.0, f64::MAX);
    for b in &BUCKETS {
        let here: Vec<_> = people.iter().filter(|p| b.picks.contains(&p.0)).collect();
        assert!(!here.is_empty(), "{} has no players", b.name);
        let n = here.len() as f64;
        let debut = here.iter().filter(|p| p.1).count() as f64 / n;
        let war = here.iter().map(|p| p.2).sum::<f64>() / n;
        let bust = here.iter().filter(|p| !p.1 || p.2 <= 0.0).count() as f64 / n;
        check(
            &format!("{} reach the majors", b.name),
            debut,
            b.debut.0,
            b.debut.1,
        );
        check(
            &format!("{} mean career WAR", b.name),
            war,
            b.war.0,
            b.war.1,
        );
        check(
            &format!("{} bust rate", b.name),
            bust,
            b.bust_min.unwrap_or(0.0),
            b.bust_max.unwrap_or(1.0),
        );
        if debut > last.0 + 1e-9 || war > last.1 + 1e-9 {
            check(
                &format!("{} curve is monotone (1 = yes)", b.name),
                0.0,
                1.0,
                1.0,
            );
        }
        last = (debut, war);
    }
    assert!(failures.is_empty(), "gate failed:\n{}", failures.join("\n"));
}
