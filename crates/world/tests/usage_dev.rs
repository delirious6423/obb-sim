//! Dev aid (not a gate): how clubs use their players, for comparison with
//! real major-league usage (players used per club, qualified players, playing
//! time by bucket, starters' share of innings, injured-list placements, the
//! share of WAR earned by top-300 draftees). Same burn-in as `draft_gate`.
//!
//!     SEEDS="1" YEARS=10 cargo test --release -p world --test usage_dev -- --ignored --nocapture

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use calibration::Calibration;
use protocol::Actor;
use world::command::{Command, Event, Line};
use world::driver::run_year;
use world::player::{DEFAULT_POOL, Origin};
use world::rating::Model;
use world::state::World;

fn env_list(name: &str, default: &str) -> Vec<u64> {
    std::env::var(name)
        .unwrap_or_else(|_| default.into())
        .split_whitespace()
        .map(|s| s.parse().expect("number"))
        .collect()
}

#[derive(Default)]
struct Sums {
    club_years: f64,
    seasons: f64,
    batters_used: f64,
    pitchers_used: f64,
    qualified_bat: f64,
    qualified_pit: f64,
    pa_band: [f64; 5],
    pa_in_band: [f64; 5],
    outs_start: f64,
    outs_all: f64,
    starts: f64,
    relief_apps: f64,
    team_games: f64,
    hurt_majors: f64,
    hurt_all: f64,
    g150: f64,
    war_top300: f64,
    war_late: f64,
    war_intl: f64,
    war_other: f64,
    war_all: f64,
    pit_bf_band: [f64; 4],
}

#[test]
#[ignore = "dev aid; needs data/internal/params.json"]
fn usage_dev() {
    let cal = Calibration::load(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/internal/params.json"),
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let years = env_list("YEARS", "10")[0] as usize;
    for seed in env_list("SEEDS", "1") {
        let model = Model::from_calibration(&cal);
        let mut w = World::new(seed, cal.params, model, DEFAULT_POOL);
        w.execute(&Actor::commissioner(), Command::Found { burn_in_years: 30 })
            .expect("found");
        let mut s = Sums::default();
        let first = w.year;
        for _ in 0..years {
            let mut log = Vec::new();
            run_year(&mut w, &mut log).expect("a year");
            let mut hurt: BTreeSet<u32> = BTreeSet::new();
            for e in &log {
                if let Event::DaysPlayed {
                    segment_end: Some(se),
                    ..
                } = e
                {
                    hurt.extend(se.hurt.iter().map(|p| p.0));
                    s.hurt_all += se.hurt.len() as f64;
                }
            }
            for e in &log {
                let Event::SeasonPlayed {
                    lines,
                    records,
                    league,
                    ..
                } = e
                else {
                    continue;
                };
                s.seasons += 1.0;
                s.club_years += records.len() as f64;
                s.team_games += f64::from(league.games);
                let mut seen: BTreeMap<u32, ()> = BTreeMap::new();
                for l in lines {
                    let played = match &l.line {
                        Line::Bat(b) => b.pa > 0,
                        Line::Pitch(p) => p.bf > 0,
                    };
                    if !played {
                        continue;
                    }
                    seen.insert(l.player.0, ());
                    match &l.line {
                        Line::Bat(b) => {
                            s.batters_used += 1.0;
                            if b.pa >= 502 {
                                s.qualified_bat += 1.0;
                            }
                            if b.g >= 150 {
                                s.g150 += 1.0;
                            }
                            let i = match b.pa {
                                0..=99 => 0,
                                100..=299 => 1,
                                300..=499 => 2,
                                500..=649 => 3,
                                _ => 4,
                            };
                            s.pa_band[i] += 1.0;
                            s.pa_in_band[i] += f64::from(b.pa);
                        }
                        Line::Pitch(p) => {
                            s.pitchers_used += 1.0;
                            if p.outs >= 486 {
                                s.qualified_pit += 1.0;
                            }
                            s.outs_all += f64::from(p.outs);
                            // A pitcher averaging 3+ innings an outing is a starter.
                            if p.outs >= 9 * p.g {
                                s.outs_start += f64::from(p.outs);
                                s.starts += f64::from(p.g);
                            } else {
                                s.relief_apps += f64::from(p.g);
                            }
                            let i = match p.bf {
                                0..=99 => 0,
                                100..=299 => 1,
                                300..=599 => 2,
                                _ => 3,
                            };
                            s.pit_bf_band[i] += 1.0;
                        }
                    }
                    let pl = w
                        .players
                        .get(&l.player)
                        .or_else(|| w.retired.get(&l.player).map(|x| &x.0));
                    let war = l.war;
                    s.war_all += war;
                    match pl {
                        Some(p) if p.origin == Origin::International => s.war_intl += war,
                        Some(p) => match p.drafted {
                            Some(d) if d.pick <= 300 => s.war_top300 += war,
                            Some(_) => s.war_late += war,
                            None => s.war_other += war,
                        },
                        None => s.war_other += war,
                    }
                }
                s.hurt_majors += hurt.iter().filter(|h| seen.contains_key(h)).count() as f64;
            }
        }
        let cy = s.club_years.max(1.0);
        let ny = s.seasons.max(1.0);
        let nb: f64 = s.pa_band.iter().sum();
        let pa: f64 = s.pa_in_band.iter().sum();
        let np: f64 = s.pit_bf_band.iter().sum();
        println!(
            "seed {seed}: per club-year batters used {:.1} pitchers used {:.1}; per season qualified batters {:.1} pitchers {:.1}; 150+ G batters {:.1}",
            s.batters_used / cy,
            s.pitchers_used / cy,
            s.qualified_bat / ny,
            s.qualified_pit / ny,
            s.g150 / ny
        );
        println!(
            "seed {seed}: batter-seasons by PA 0-99 {:.1}% 100-299 {:.1}% 300-499 {:.1}% 500-649 {:.1}% 650+ {:.1}%; PA share of 500+ {:.1}%",
            100.0 * s.pa_band[0] / nb,
            100.0 * s.pa_band[1] / nb,
            100.0 * s.pa_band[2] / nb,
            100.0 * s.pa_band[3] / nb,
            100.0 * s.pa_band[4] / nb,
            100.0 * (s.pa_in_band[3] + s.pa_in_band[4]) / pa
        );
        println!(
            "seed {seed}: pitcher-seasons by BF 0-99 {:.1}% 100-299 {:.1}% 300-599 {:.1}% 600+ {:.1}%; starters' share of innings {:.1}%; IP per start {:.2}; relief apps per team-game {:.2}",
            100.0 * s.pit_bf_band[0] / np,
            100.0 * s.pit_bf_band[1] / np,
            100.0 * s.pit_bf_band[2] / np,
            100.0 * s.pit_bf_band[3] / np,
            100.0 * s.outs_start / s.outs_all.max(1.0),
            s.outs_start / 3.0 / s.starts.max(1.0),
            s.relief_apps / s.team_games.max(1.0)
        );
        println!(
            "seed {seed}: injuries per season {:.0} (to players who played in the majors that year {:.0})",
            s.hurt_all / ny,
            s.hurt_majors / ny
        );
        let debuts = w
            .players
            .values()
            .chain(w.retired.values().map(|x| &x.0))
            .filter(|p| p.debut.is_some_and(|d| d >= first))
            .fold([0.0; 4], |mut a, p| {
                let i = match (p.origin, p.drafted.map(|d| d.pick)) {
                    (Origin::International, _) => 2,
                    (_, Some(1..=300)) => 0,
                    (_, Some(_)) => 1,
                    _ => 3,
                };
                a[i] += 1.0;
                a
            });
        println!(
            "seed {seed}: debuts per season {:.0} (top-300 draftees {:.0}, later picks {:.0}, international {:.0}, other {:.0})",
            debuts.iter().sum::<f64>() / ny,
            debuts[0] / ny,
            debuts[1] / ny,
            debuts[2] / ny,
            debuts[3] / ny
        );
        let wa = s.war_all.max(1e-9);
        println!(
            "seed {seed}: WAR share top-300 draftees {:.1}% later picks {:.1}% international {:.1}% other {:.1}% (league WAR/season {:.0})",
            100.0 * s.war_top300 / wa,
            100.0 * s.war_late / wa,
            100.0 * s.war_intl / wa,
            100.0 * s.war_other / wa,
            s.war_all / ny
        );
    }
}
