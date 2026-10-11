//! Speed benchmark through the public protocol: found a league, play a
//! season, run an offseason. Prints seconds; with `--check` it fails when a
//! step is slower than the target in `TARGETS` (set from profiling; see
//! PLAN.md). Run in release mode: `cargo run --release -p host --example bench`.

use std::time::Instant;

use host::api::{Advance, Difficulty, NewGame, Preset, Request, Response, Stage};
use host::{GameClient, LocalClient};

/// Seconds allowed per step on a CI runner (about 3x the time measured on a
/// development laptop, to leave room for slow shared machines).
const TARGETS: [(&str, f64); 4] = [
    ("found", 4.0),
    ("30 single days", 2.0),
    ("season", 1.5),
    ("offseason", 3.0),
];

fn done(r: Response) {
    if let Response::Failed(e) = r {
        eprintln!("failed: {e}");
        std::process::exit(2);
    }
}

fn main() {
    let check = std::env::args().any(|a| a == "--check");
    let c = LocalClient::start(host::calibration(), None);
    let mut times = Vec::new();
    let t = Instant::now();
    done(c.call(Request::NewGame(NewGame {
        seed: 7,
        club: 3,
        difficulty: Difficulty::Preset(Preset::Veteran),
        burn_in_years: 2,
        roles: Default::default(),
    })));
    times.push(("found", t.elapsed().as_secs_f64()));

    // The way a screen steps through the season one day at a time.
    let t = Instant::now();
    for _ in 0..30 {
        done(c.call(Request::Advance(Advance::SimDays(1))));
    }
    times.push(("30 single days", t.elapsed().as_secs_f64()));

    let t = Instant::now();
    done(c.call(Request::Advance(Advance::SimToEndOfSeason)));
    times.push(("season", t.elapsed().as_secs_f64()));

    let t = Instant::now();
    done(c.call(Request::Advance(Advance::FinishSeason)));
    for _ in 0..400 {
        done(c.call(Request::Advance(Advance::SimDraft)));
        match c.call(Request::Overview) {
            Response::Overview(o) if o.stage == Stage::Draft && o.on_the_clock => {
                done(c.call(Request::Act(host::api::Action::PassPick)));
            }
            Response::Overview(o) if o.stage != Stage::Draft => break,
            _ => {}
        }
    }
    done(c.call(Request::Advance(Advance::StartNextSeason)));
    times.push(("offseason", t.elapsed().as_secs_f64()));

    let mut bad = false;
    for ((name, secs), (_, limit)) in times.iter().zip(TARGETS) {
        println!("{name}: {secs:.2} s (target {limit:.1} s)");
        bad |= check && *secs > limit;
    }
    if bad {
        eprintln!("slower than target");
        std::process::exit(1);
    }
}
