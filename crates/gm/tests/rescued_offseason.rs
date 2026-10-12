//! A rescued club goes through a full offseason with built-in general
//! managers and starts the season with a roster the rules accept.
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
use world::state::{CREDIT, World};

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

#[test]
fn a_rescued_club_starts_the_season_with_a_roster_in_order() {
    for seed in [7] {
        let (mut w, mut seats, mut log) = league(seed);
        run_year_with(&mut w, &mut seats, &mut log).expect("first year");
        // Far below the credit line: this year's settlement rescues club 0,
        // voiding the contracts that broke it.
        w.clubs[0].cash = -10.0 * CREDIT;
        run_year_with(&mut w, &mut seats, &mut log).expect("rescue year");
        assert!(
            w.finances[0].last().is_some_and(|f| f.rescued),
            "seed {seed}: the test needs club 0 rescued"
        );
        let problems = w.roster_problems(0);
        assert!(problems.is_empty(), "seed {seed}: {problems:?}");
    }
}
