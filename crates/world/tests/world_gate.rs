//! Phase 3 exit gates. Ignored by default (they need the git-ignored
//! `data/internal/params.json` and take a minute):
//!
//!     cargo test --release -p world --test world_gate -- --ignored --nocapture
//!
//! `stability_gate`: for each of five seeds, fifty seasons with fixed-behaviour
//! owners and GMs must show no talent inflation or collapse (league K%, BB%,
//! HR% and runs per team-game near the real league and without drift between
//! the first and last decade; the average major leaguer stays near league
//! average) and no insolvent club. Every seed must pass.
//!
//! `draft_gate`: draft-slot value curves against published studies (sources,
//! definitions and ranges are in PLAN.md, "Draft bands"). Every seed must pass.
//!
//! Bands are never edited to make a run pass; a change needs a cited source.

use std::ops::RangeInclusive;
use std::path::Path;

use calibration::Calibration;
use protocol::Actor;
use world::command::Command;
use world::driver::run_year;
use world::player::{DEFAULT_POOL, Level, Origin};
use world::rating::{Kind, Model};
use world::state::World;

const SEASONS: usize = 50;
const BURN_IN: u8 = 30;
const SEEDS: [u64; 5] = [20_250_101, 1, 2, 3, 4];

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

/// A drafted amateur (not international) who has played out his career.
struct Draftee {
    pick: u16,
    signed: bool,
    reached: bool,
    war: f64,
    seasons: u16,
}

struct Run {
    seasons: Vec<Season>,
    lowest_cash: f64,
    draftees: Vec<Draftee>,
}

fn mean<T>(v: &[T], f: impl Fn(&T) -> f64) -> f64 {
    v.iter().map(f).sum::<f64>() / v.len() as f64
}

fn calibration() -> Calibration {
    Calibration::load(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/internal/params.json"),
    )
    .unwrap_or_else(|e| panic!("{e}"))
}

fn simulate(cal: &Calibration, seed: u64) -> Run {
    let model = Model::from_calibration(cal);
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
    // Classes old enough to have played out their careers. A player who
    // refused to sign left the game in his draft year.
    let mut draftees = Vec::new();
    let retired = w.retired.values().map(|(p, y)| (p, Some(*y)));
    for (p, left) in w.players.values().map(|p| (p, None)).chain(retired) {
        let Some(d) = p.drafted else { continue };
        if p.origin == Origin::International || d.pick > 300 {
            continue;
        }
        if d.year > first_year && d.year <= w.year - 15 {
            draftees.push(Draftee {
                pick: d.pick,
                signed: left != Some(d.year),
                reached: p.career.seasons > 0,
                war: p.career.war,
                seasons: p.career.seasons,
            });
        }
    }
    Run {
        seasons,
        lowest_cash,
        draftees,
    }
}

struct Checker {
    failures: Vec<String>,
}

impl Checker {
    fn check(&mut self, label: &str, name: &str, value: f64, lo: f64, hi: f64) {
        let ok = (lo..=hi).contains(&value);
        println!(
            "{label:<9} {name:<44} {value:>9.4}  [{lo:.4}, {hi:.4}]  {}",
            if ok { "PASS" } else { "FAIL" }
        );
        if !ok {
            self.failures.push(format!(
                "{label}: {name}: {value:.4} outside [{lo:.4}, {hi:.4}]"
            ));
        }
    }

    fn finish(self, what: &str) {
        assert!(
            self.failures.is_empty(),
            "{what} failed:\n{}",
            self.failures.join("\n")
        );
    }
}

#[test]
#[ignore = "needs data/internal/params.json; run with --release -- --ignored"]
fn stability_gate() {
    let cal = calibration();
    let league = cal.params.league.0;
    let runs_target = cal.run_environment.runs_per_team_game_dh;
    let mut c = Checker { failures: vec![] };
    for seed in SEEDS {
        let label = format!("seed {seed}");
        let label = label.as_str();
        let run = simulate(&cal, seed);
        let s = &run.seasons;
        let all = |f: Metric| mean(s, f);
        let decade = |r: std::ops::Range<usize>, f: Metric| mean(&s[r], f);
        let rates: [(&str, Metric, f64); 3] = [
            ("K%", |s| s.k, league[0]),
            ("BB%", |s| s.bb, league[1]),
            ("HR/PA", |s| s.hr, league[6]),
        ];
        for (name, f, target) in rates {
            c.check(
                label,
                &format!("{name} mean vs real"),
                all(f),
                target * (1.0 - RATE_BAND),
                target * (1.0 + RATE_BAND),
            );
            let ratio = decade(SEASONS - 10..SEASONS, f) / decade(0..10, f);
            c.check(
                label,
                &format!("{name} last decade / first"),
                ratio,
                1.0 - DRIFT_RATE,
                1.0 + DRIFT_RATE,
            );
        }
        c.check(
            label,
            "runs/team-game mean vs real",
            all(|s| s.runs),
            runs_target * (1.0 - RUNS_BAND),
            runs_target * (1.0 + RUNS_BAND),
        );
        let ratio = decade(SEASONS - 10..SEASONS, |s| s.runs) / decade(0..10, |s| s.runs);
        c.check(
            label,
            "runs last decade / first",
            ratio,
            1.0 - DRIFT_RUNS,
            1.0 + DRIFT_RUNS,
        );
        for (i, name) in ["batter", "pitcher"].into_iter().enumerate() {
            let get = move |s: &Season| s.q[i];
            let drift = mean(&s[SEASONS - 10..], get) - mean(&s[0..10], get);
            c.check(
                label,
                &format!("{name} quality drift (sd)"),
                drift,
                -DRIFT_QUALITY_SD,
                DRIFT_QUALITY_SD,
            );
            let worst = s
                .iter()
                .map(get)
                .fold(0.0f64, |a, b| if b.abs() > a.abs() { b } else { a });
            c.check(
                label,
                &format!("{name} quality, worst season (sd)"),
                worst,
                -QUALITY_BAND_SD,
                QUALITY_BAND_SD,
            );
        }
        c.check(
            label,
            "lowest club cash, ever",
            run.lowest_cash,
            0.0,
            f64::MAX,
        );
    }
    c.finish("stability gate");
}

struct Bucket {
    name: &'static str,
    picks: RangeInclusive<u16>,
    /// Signed draftees who play at least one major-league game.
    reach: (f64, f64),
    /// Mean career WAR per draftee.
    war: (f64, f64),
    /// Never reached, or under 1.5 WAR per major-league season.
    bust: Option<(f64, f64)>,
}

/// Ranges from published studies; see PLAN.md, "Draft bands".
const BUCKETS: [Bucket; 4] = [
    Bucket {
        name: "picks 1-10",
        picks: 1..=10,
        reach: (0.75, 0.87),
        war: (9.5, 12.8),
        bust: Some((0.60, 0.65)),
    },
    Bucket {
        name: "picks 11-30",
        picks: 11..=30,
        reach: (0.58, 0.82),
        war: (4.5, 8.7),
        bust: Some((0.71, 0.85)),
    },
    Bucket {
        name: "picks 31-90",
        picks: 31..=90,
        reach: (0.40, 0.51),
        war: (0.8, 3.3),
        bust: None,
    },
    Bucket {
        name: "picks 91-300",
        picks: 91..=300,
        reach: (0.17, 0.35),
        // No published source found for these two; kept from the first draft.
        war: (0.0, 3.0),
        bust: Some((0.75, 1.0)),
    },
];

#[test]
#[ignore = "needs data/internal/params.json; run with --release -- --ignored"]
fn draft_gate() {
    let cal = calibration();
    let mut c = Checker { failures: vec![] };
    for seed in SEEDS {
        let label = format!("seed {seed}");
        let label = label.as_str();
        let run = simulate(&cal, seed);
        let mut last = (1.0, f64::MAX);
        for b in &BUCKETS {
            let all: Vec<_> = run
                .draftees
                .iter()
                .filter(|p| b.picks.contains(&p.pick))
                .collect();
            assert!(!all.is_empty(), "{} has no players", b.name);
            let signed: Vec<_> = all.iter().filter(|p| p.signed).collect();
            let reach = signed.iter().filter(|p| p.reached).count() as f64 / signed.len() as f64;
            let war = all.iter().map(|p| p.war).sum::<f64>() / all.len() as f64;
            let bust = signed
                .iter()
                .filter(|p| !p.reached || p.war / f64::from(p.seasons) < 1.5)
                .count() as f64
                / signed.len() as f64;
            c.check(
                label,
                &format!("{} reach the majors", b.name),
                reach,
                b.reach.0,
                b.reach.1,
            );
            c.check(
                label,
                &format!("{} mean career WAR", b.name),
                war,
                b.war.0,
                b.war.1,
            );
            if let Some((lo, hi)) = b.bust {
                c.check(label, &format!("{} bust rate", b.name), bust, lo, hi);
            }
            let monotone = reach <= last.0 + 1e-9 && war <= last.1 + 1e-9;
            c.check(
                label,
                &format!("{} curve is monotone", b.name),
                f64::from(u8::from(monotone)),
                1.0,
                1.0,
            );
            last = (reach, war);
        }
    }
    c.finish("draft gate");
}
