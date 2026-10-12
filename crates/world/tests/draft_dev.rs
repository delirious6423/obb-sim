//! Dev aid for the draft gate (not a gate). Prints, per draft bucket, career
//! length, peak season WAR and WAR per season, and the ageing curve of
//! full-time batters. Same simulation and selection as `draft_gate`.
//!
//!     cargo test --release -p world --test draft_dev -- --ignored --nocapture

use std::collections::BTreeMap;
use std::path::Path;

use calibration::Calibration;
use protocol::Actor;
use world::command::{Command, Event, Line};
use world::driver::run_year;
use world::player::{DEFAULT_POOL, Level, Origin};
use world::rating::{Kind, Model};
use world::state::World;

const SEASONS: usize = 50;
const BURN_IN: u8 = 30;
const SEEDS: [u64; 2] = [1, 2];

const BUCKETS: [(&str, u16, u16); 4] = [
    ("1-10", 1, 10),
    ("11-30", 11, 30),
    ("31-90", 31, 90),
    ("91-300", 91, 300),
];

struct Draftee {
    pick: u16,
    signed: bool,
    peak_quality_sd: f64,
    seasons: u16,
    war: f64,
    peak_war: f64,
    first_age: i32,
    last_age: i32,
    kind: Kind,
}

#[test]
#[ignore = "dev aid; needs data/internal/params.json"]
fn draft_dev() {
    let cal = Calibration::load(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/internal/params.json"),
    )
    .unwrap_or_else(|e| panic!("{e}"));
    // Ageing: (age bucket) -> (sum of WAR per season, seasons) for batters with 400+ PA.
    let mut aging: BTreeMap<i32, (f64, f64)> = BTreeMap::new();
    for seed in SEEDS {
        let model = Model::from_calibration(&cal);
        let mut w = World::new(seed, cal.params, model, DEFAULT_POOL);
        w.execute(
            &Actor::commissioner(),
            Command::Found {
                burn_in_years: BURN_IN,
            },
        )
        .expect("found");
        let first_year = w.year;
        // Per player: (age, war, pa-or-bf) for each season with playing time.
        let mut by_player: BTreeMap<u32, Vec<(i32, f64)>> = BTreeMap::new();
        for _ in 0..SEASONS {
            let mut log: Vec<Event> = Vec::new();
            let year = w.year;
            run_year(&mut w, &mut log).expect("a year");
            for e in &log {
                let Event::SeasonPlayed { lines, .. } = e else {
                    continue;
                };
                for l in lines {
                    let (played, full) = match &l.line {
                        Line::Bat(b) => (b.pa > 0, b.pa >= 400),
                        Line::Pitch(p) => (p.bf > 0, false),
                    };
                    if !played {
                        continue;
                    }
                    let Some(p) = w
                        .players
                        .get(&l.player)
                        .or_else(|| w.retired.get(&l.player).map(|(p, _)| p))
                    else {
                        continue;
                    };
                    let age = year - p.born;
                    by_player.entry(l.player.0).or_default().push((age, l.war));
                    if full && p.kind == Kind::Batter && p.level == Level::Majors {
                        let e = aging.entry(age).or_insert((0.0, 0.0));
                        e.0 += l.war;
                        e.1 += 1.0;
                    }
                }
            }
        }
        let mut draftees: Vec<Draftee> = Vec::new();
        let retired = w.retired.values().map(|(p, y)| (p, Some(*y)));
        for (p, left) in w.players.values().map(|p| (p, None)).chain(retired) {
            let Some(d) = p.drafted else { continue };
            if p.origin == Origin::International || d.pick > 300 {
                continue;
            }
            if d.year > first_year && d.year <= w.year - 15 {
                let seasons_v = by_player.get(&p.id.0).cloned().unwrap_or_default();
                let peak_war = seasons_v.iter().map(|s| s.1).fold(0.0, f64::max);
                draftees.push(Draftee {
                    pick: d.pick,
                    signed: left != Some(d.year),
                    peak_quality_sd: p.peak_quality / w.model.quality_sd(p.kind),
                    seasons: p.career.seasons,
                    war: p.career.war,
                    peak_war,
                    first_age: seasons_v.first().map_or(0, |s| s.0),
                    last_age: seasons_v.last().map_or(0, |s| s.0),
                    kind: p.kind,
                });
            }
        }
        println!("seed {seed}: {} draftees aged 15+ years", draftees.len());
        for (name, lo, hi) in BUCKETS {
            let all: Vec<&Draftee> = draftees
                .iter()
                .filter(|d| (lo..=hi).contains(&d.pick))
                .collect();
            let signed: Vec<&Draftee> = all.iter().copied().filter(|d| d.signed).collect();
            let reached: Vec<&Draftee> = signed.iter().copied().filter(|d| d.seasons > 0).collect();
            let n = reached.len().max(1) as f64;
            let seasons = reached.iter().map(|d| f64::from(d.seasons)).sum::<f64>() / n;
            let war = reached.iter().map(|d| d.war).sum::<f64>() / n;
            let peak = reached.iter().map(|d| d.peak_war).sum::<f64>() / n;
            let peak_q =
                signed.iter().map(|d| d.peak_quality_sd).sum::<f64>() / signed.len().max(1) as f64;
            let batters = reached.iter().filter(|d| d.kind == Kind::Batter).count();
            let mean_first = reached.iter().map(|d| f64::from(d.first_age)).sum::<f64>() / n;
            let mean_last = reached.iter().map(|d| f64::from(d.last_age)).sum::<f64>() / n;
            let mut hist = [0usize; 6];
            for d in &reached {
                let i = match d.seasons {
                    0..=2 => 0,
                    3..=5 => 1,
                    6..=9 => 2,
                    10..=12 => 3,
                    13..=15 => 4,
                    _ => 5,
                };
                hist[i] += 1;
            }
            println!(
                "seed {seed} picks {name:<7} n {:>4} signed {:>4} reached {:>4} ({:.3}) batters {:.2} \
                 seasons {seasons:.2} first-age {mean_first:.1} last-age {mean_last:.1} \
                 WAR/reached {war:.2} WAR/season {:.2} peak-season-WAR {peak:.2} \
                 peak-quality-sd {peak_q:.2}",
                all.len(),
                signed.len(),
                reached.len(),
                reached.len() as f64 / signed.len().max(1) as f64,
                batters as f64 / n,
                war / seasons.max(1e-9),
            );
            println!(
                "seed {seed} picks {name:<7} season histogram 0-2 {} 3-5 {} 6-9 {} 10-12 {} 13-15 {} 16+ {}",
                hist[0], hist[1], hist[2], hist[3], hist[4], hist[5]
            );
        }
    }
    println!("ageing (full-time batters, 400+ PA): age, WAR per season, seasons");
    for (age, (s, n)) in &aging {
        if *n >= 20.0 {
            println!("age {age:>2} {:>6.2} {:>6.0}", s / n, n);
        }
    }
}
