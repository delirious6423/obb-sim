//! Debt rule, end to end: a club deep in debt goes through a full offseason
//! with built-in general managers, frozen out of every signing but the
//! minimum-salary fills, and starts the season with a roster the rules
//! accept.
//!
//! This lives in `gm` rather than `world` because the built-in seats are in
//! `connect`, which depends on `world`.

use connect::{
    BuiltInCommissioner, BuiltInGm, BuiltInManager, BuiltInOwner, Seats, run_year_with, start_with,
};
use sim_core::GameParams;
use world::command::Event;
use world::player::DEFAULT_POOL;
use world::rating::Model;
use world::state::World;

fn league(seed: u64) -> (World, Seats, Vec<Event>) {
    let mut w = World::new(
        seed,
        GameParams::reference(),
        Model::generic(),
        DEFAULT_POOL,
    );
    let mut seats = Seats::fill(
        Box::new(BuiltInCommissioner { burn_in_years: 2 }),
        |_| Box::new(BuiltInOwner),
        |_| Box::new(BuiltInGm),
        |_| Box::new(BuiltInManager::default()),
    );
    let mut log = Vec::new();
    start_with(&mut w, &mut seats, &mut log).expect("the league is founded");
    (w, seats, log)
}

const SEED: u64 = 7;

#[test]
fn a_club_deep_in_debt_starts_the_season_with_a_roster_in_order() {
    let (mut w, mut seats, mut log) = league(SEED);
    run_year_with(&mut w, &mut seats, &mut log).expect("first year");
    // Far deeper in debt than one season's profit can repay.
    w.clubs[0].cash = -250.0;
    run_year_with(&mut w, &mut seats, &mut log).expect("year in debt");
    assert!(
        w.finances[0].last().is_some_and(|f| !f.rescued),
        "no rescue"
    );
    assert!(w.in_debt(0), "the test needs club 0 still in debt");
    let problems = w.roster_problems(0);
    assert!(problems.is_empty(), "{problems:?}");
}
