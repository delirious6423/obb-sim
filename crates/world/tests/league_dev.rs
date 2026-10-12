//! Dev aid (not a gate): league-wide realism diagnostics over the whole league,
//! not only the draft buckets. Prints, for seeds 1 and 2 after the same burn-in
//! as `draft_gate`:
//! - career length of every player who ever reached the majors (retired players
//!   only, so careers are complete; active players are counted as censored);
//! - per year: player-seasons by WAR band (>=6, 4-6, 2-4, 0-2, <0), and the
//!   number of players with 6+ WAR seasons;
//! - playing-time share: player-seasons by PA (batters) or BF (pitchers) band;
//! - career WAR histogram of retired reached players, by draft bucket and overall.
//!
//!     cargo test --release -p world --test league_dev -- --ignored --nocapture

use std::collections::BTreeMap;
use std::path::Path;

use calibration::Calibration;
use protocol::Actor;
use world::command::{Command, Event, Line};
use world::driver::run_year;
use world::player::{DEFAULT_POOL, Origin};
use world::rating::Model;
use world::state::World;

const SEASONS: usize = 50;
const BURN_IN: u8 = 30;
const SEEDS: [u64; 2] = [1, 2];

#[derive(Default)]
struct Track {
    seasons: u32,
    war: f64,
    first: i32,
    last: i32,
    pick: Option<u16>,
    intl: bool,
}

#[test]
#[ignore = "dev aid; needs data/internal/params.json"]
fn league_dev() {
    let cal = Calibration::load(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/internal/params.json"),
    )
    .unwrap_or_else(|e| panic!("{e}"));
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
        let mut track: BTreeMap<u32, Track> = BTreeMap::new();
        // Per year: [>=6, 4-6, 2-4, 0-2, <0] player-seasons; 6+ players.
        let mut band = [0u64; 5];
        // Playing time: [0-99, 100-299, 300-499, 500-649, 650+] for batters, BF bands for pitchers
        // [0-99, 100-199, 200-349, 350-449, 450+].
        let mut pa_band = [0u64; 5];
        let mut bf_band = [0u64; 5];
        let mut active_per_year = 0u64;
        let mut six_per_year = 0u64;
        for _ in 0..SEASONS {
            let mut log: Vec<Event> = Vec::new();
            let year = w.year;
            run_year(&mut w, &mut log).expect("a year");
            for e in &log {
                let Event::SeasonPlayed { lines, .. } = e else {
                    continue;
                };
                for l in lines {
                    let (played, volume, is_pitch) = match &l.line {
                        Line::Bat(b) => (b.pa > 0, b.pa, false),
                        Line::Pitch(p) => (p.bf > 0, p.bf, true),
                    };
                    if !played {
                        continue;
                    }
                    if is_pitch {
                        let i = match volume {
                            0..=99 => 0,
                            100..=199 => 1,
                            200..=349 => 2,
                            350..=449 => 3,
                            _ => 4,
                        };
                        bf_band[i] += 1;
                    } else {
                        let i = match volume {
                            0..=99 => 0,
                            100..=299 => 1,
                            300..=499 => 2,
                            500..=649 => 3,
                            _ => 4,
                        };
                        pa_band[i] += 1;
                    }
                    let w_ = l.war;
                    let b = if w_ >= 6.0 {
                        0
                    } else if w_ >= 4.0 {
                        1
                    } else if w_ >= 2.0 {
                        2
                    } else if w_ >= 0.0 {
                        3
                    } else {
                        4
                    };
                    band[b] += 1;
                    if w_ >= 6.0 {
                        six_per_year += 1;
                    }
                    active_per_year += 1;
                    let t = track.entry(l.player.0).or_default();
                    t.seasons += 1;
                    t.war += w_;
                    if t.first == 0 {
                        t.first = year;
                    }
                    t.last = year;
                }
            }
        }
        // Attach draft and origin for every player we saw.
        let mut retired_ids: BTreeMap<u32, ()> = BTreeMap::new();
        for (id, (p, _)) in &w.retired {
            retired_ids.insert(id.0, ());
            if let Some(t) = track.get_mut(&id.0) {
                t.pick = p.drafted.map(|d| d.pick);
                t.intl = p.origin == Origin::International;
            }
        }
        for (id, p) in &w.players {
            if let Some(t) = track.get_mut(&id.0) {
                t.pick = p.drafted.map(|d| d.pick);
                t.intl = p.origin == Origin::International;
            }
        }
        let years = SEASONS as f64;
        println!(
            "seed {seed}: reached players {} (retired {}); player-seasons/yr {:.0}; 6+ WAR seasons/yr {:.1}",
            track.len(),
            retired_ids.len(),
            active_per_year as f64 / years,
            six_per_year as f64 / years
        );
        let per = |x: u64| x as f64 / years;
        println!(
            "seed {seed} seasons/yr by WAR: >=6 {:.1} 4-6 {:.1} 2-4 {:.1} 0-2 {:.1} <0 {:.1}",
            per(band[0]),
            per(band[1]),
            per(band[2]),
            per(band[3]),
            per(band[4])
        );
        let total_b: u64 = pa_band.iter().sum();
        println!(
            "seed {seed} batter seasons by PA: 0-99 {:.1}% 100-299 {:.1}% 300-499 {:.1}% 500-649 {:.1}% 650+ {:.1}%",
            pct(pa_band[0], total_b),
            pct(pa_band[1], total_b),
            pct(pa_band[2], total_b),
            pct(pa_band[3], total_b),
            pct(pa_band[4], total_b)
        );
        let total_p: u64 = bf_band.iter().sum();
        println!(
            "seed {seed} pitcher seasons by BF: 0-99 {:.1}% 100-199 {:.1}% 200-349 {:.1}% 350-449 {:.1}% 450+ {:.1}%",
            pct(bf_band[0], total_p),
            pct(bf_band[1], total_p),
            pct(bf_band[2], total_p),
            pct(bf_band[3], total_p),
            pct(bf_band[4], total_p)
        );
        // Career length and WAR of retired reached players (complete careers).
        let mut len_hist = [0usize; 6];
        let mut war_hist = [0usize; 7];
        let mut n_ret = 0usize;
        let mut len_sum = 0.0;
        let mut war_sum = 0.0;
        let mut b_len: BTreeMap<&str, (f64, usize)> = BTreeMap::new();
        for (id, t) in &track {
            if !retired_ids.contains_key(id) || t.intl {
                continue;
            }
            n_ret += 1;
            len_sum += f64::from(t.seasons);
            war_sum += t.war;
            let i = match t.seasons {
                0..=2 => 0,
                3..=5 => 1,
                6..=9 => 2,
                10..=12 => 3,
                13..=15 => 4,
                _ => 5,
            };
            len_hist[i] += 1;
            let j = match t.war {
                x if x < 0.0 => 0,
                x if x < 1.0 => 1,
                x if x < 2.0 => 2,
                x if x < 5.0 => 3,
                x if x < 10.0 => 4,
                x if x < 20.0 => 5,
                _ => 6,
            };
            war_hist[j] += 1;
            let bucket = match t.pick {
                Some(1..=10) => "1-10",
                Some(11..=30) => "11-30",
                Some(31..=90) => "31-90",
                Some(91..=300) => "91-300",
                _ => "undrafted",
            };
            let e = b_len.entry(bucket).or_insert((0.0, 0));
            e.0 += f64::from(t.seasons);
            e.1 += 1;
        }
        let nf = n_ret.max(1) as f64;
        println!(
            "seed {seed} retired reached (domestic) n {n_ret}: mean seasons {:.2} mean career WAR {:.2}",
            len_sum / nf,
            war_sum / nf
        );
        println!(
            "seed {seed} career length (seasons) 0-2 {} 3-5 {} 6-9 {} 10-12 {} 13-15 {} 16+ {}",
            len_hist[0], len_hist[1], len_hist[2], len_hist[3], len_hist[4], len_hist[5]
        );
        println!(
            "seed {seed} career WAR <0 {} 0-1 {} 1-2 {} 2-5 {} 5-10 {} 10-20 {} 20+ {}",
            war_hist[0],
            war_hist[1],
            war_hist[2],
            war_hist[3],
            war_hist[4],
            war_hist[5],
            war_hist[6]
        );
        for (k, (s, n)) in &b_len {
            println!(
                "seed {seed} retired by bucket {k:<9} n {n:>5} mean seasons {:.2}",
                s / *n as f64
            );
        }
    }
}

fn pct(x: u64, total: u64) -> f64 {
    100.0 * x as f64 / total.max(1) as f64
}
