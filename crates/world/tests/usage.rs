//! Playing-time customs (`Usage`): modern clubs spread playing time over
//! more players than 1980s clubs, and saved customs are checked on load.

use std::collections::BTreeSet;

use protocol::Actor;
use sim_core::GameParams;
use world::command::{Command, Event, Line};
use world::driver::run_year;
use world::player::DEFAULT_POOL;
use world::rating::Model;
use world::state::{Usage, World};

fn founded(seed: u64, usage: Usage) -> World {
    let mut w = World::new(
        seed,
        GameParams::reference(),
        Model::generic(),
        DEFAULT_POOL,
    );
    w.usage = usage;
    w.execute(&Actor::commissioner(), Command::Found { burn_in_years: 2 })
        .expect("found");
    w
}

/// Distinct pitchers who faced a batter, and pitching appearances.
fn pitching(usage: Usage) -> (usize, u32) {
    let mut w = founded(5, usage);
    let mut log = Vec::new();
    run_year(&mut w, &mut log).expect("a year");
    let mut used = BTreeSet::new();
    let mut games = 0;
    for e in &log {
        if let Event::SeasonPlayed { lines, .. } = e {
            for l in lines {
                if let Line::Pitch(p) = &l.line
                    && p.bf > 0
                {
                    used.insert(l.player);
                    games += p.g;
                }
            }
        }
    }
    (used.len(), games)
}

#[test]
fn modern_clubs_use_more_pitchers_and_relievers_than_classic_ones() {
    let (modern_used, modern_games) = pitching(Usage::MODERN);
    let (classic_used, classic_games) = pitching(Usage::CLASSIC);
    assert!(
        modern_used > classic_used,
        "pitchers used: modern {modern_used}, classic {classic_used}"
    );
    assert!(
        modern_games > classic_games,
        "pitching appearances: modern {modern_games}, classic {classic_games}"
    );
}

#[test]
fn old_saves_get_modern_usage_and_bad_usage_is_refused() {
    let w = founded(3, Usage::MODERN);
    let mut json = serde_json::to_value(w.snapshot()).expect("serialise");
    json.as_object_mut().expect("object").remove("usage");
    let old = serde_json::from_value(json).expect("an old save still parses");
    let back = World::restore(GameParams::reference(), Model::generic(), old).expect("restore");
    assert_eq!(back.usage, Usage::MODERN);

    let mut snap = w.snapshot();
    snap.usage.platoon_share = 1.5;
    assert!(World::restore(GameParams::reference(), Model::generic(), snap).is_err());
    let mut snap = w.snapshot();
    snap.usage.bullpen = 0;
    assert!(World::restore(GameParams::reference(), Model::generic(), snap).is_err());
}
