use protocol::Actor;
use sim_core::GameParams;
use world::command::{Command, Event};
use world::driver::run_year;
use world::player::DEFAULT_POOL;
use world::rating::Model;
use world::state::{Phase, World};

fn fresh(seed: u64) -> World {
    World::new(
        seed,
        GameParams::reference(),
        Model::generic(),
        DEFAULT_POOL,
    )
}

#[test]
fn founding_then_a_year_runs() {
    let mut w = fresh(1);
    let ev = w
        .execute(&Actor::commissioner(), Command::Found { burn_in_years: 2 })
        .expect("found");
    assert!(matches!(ev[0], Event::Founded(_)));
    assert_eq!(w.phase, Phase::Regular);
    run_year(&mut w, &mut Vec::new()).expect("a year");
    assert_eq!(w.phase, Phase::Regular);
    assert_eq!(w.history.len(), 1);
}

/// Run `years` years, returning the world and every event it produced.
fn run(seed: u64, years: usize) -> (World, Vec<Event>) {
    let mut w = fresh(seed);
    let mut log = w
        .execute(&Actor::commissioner(), Command::Found { burn_in_years: 2 })
        .expect("found");
    for _ in 0..years {
        run_year(&mut w, &mut log).expect("year");
    }
    (w, log)
}

#[test]
fn same_seed_same_world_and_different_seed_differs() {
    let (a, _) = run(7, 2);
    let (b, _) = run(7, 2);
    let (c, _) = run(8, 2);
    assert!(a == b);
    assert!(a != c);
}

#[test]
fn replaying_the_events_rebuilds_the_world() {
    let (w, log) = run(3, 3);
    let r = World::from_events(
        3,
        GameParams::reference(),
        Model::generic(),
        DEFAULT_POOL,
        &log,
    )
    .expect("replay");
    assert!(w == r);
}

#[test]
fn wrong_role_or_phase_is_refused_and_changes_nothing() {
    let mut w = fresh(5);
    assert!(
        w.execute(&Actor::commissioner(), Command::PlaySeason)
            .is_err()
    );
    let gm = Actor::gm(protocol::TeamId(0));
    assert!(w.execute(&gm, Command::Found { burn_in_years: 0 }).is_err());
    assert!(w.phase == Phase::Unfounded);
}

fn founded(seed: u64) -> World {
    let mut w = fresh(seed);
    w.execute(&Actor::commissioner(), Command::Found { burn_in_years: 2 })
        .expect("found");
    w
}

#[test]
fn rosters_stay_consistent_over_several_years() {
    let (w, _) = run(11, 4);
    for (t, c) in w.clubs.iter().enumerate() {
        for kind in [world::rating::Kind::Batter, world::rating::Kind::Pitcher] {
            let majors = c
                .players
                .iter()
                .map(|id| &w.players[id])
                .filter(|p| p.kind == kind && p.level == world::player::Level::Majors)
                .count();
            assert_eq!(majors, 13, "club {t} {kind:?}");
        }
        for id in &c.players {
            assert_eq!(w.players[id].team, Some(t as u8));
        }
    }
    for id in &w.free_agents {
        assert!(w.players[id].team.is_none());
    }
    let on_clubs: usize = w.clubs.iter().map(|c| c.players.len()).sum();
    let unattached = w.players.values().filter(|p| p.team.is_none()).count();
    assert_eq!(on_clubs + unattached, w.players.len());
    assert_eq!(unattached, w.free_agents.len());
}

#[test]
fn roles_and_clubs_are_enforced() {
    let mut w = founded(2);
    let owner = Actor::owner(protocol::TeamId(3));
    let gm0 = Actor::gm(protocol::TeamId(0));
    let budget = Command::SetBudget {
        team: 3,
        payroll: 100.0,
    };
    // Only the owner of that club may set its budget.
    assert!(w.execute(&gm0, budget.clone()).is_err());
    assert!(
        w.execute(&Actor::owner(protocol::TeamId(4)), budget.clone())
            .is_err()
    );
    // Outside the payroll limits is refused; the world is left as it was.
    let before = w.clone();
    let too_much = Command::SetBudget {
        team: 3,
        payroll: 1.0e6,
    };
    assert!(w.execute(&owner, too_much).is_err());
    assert!(w == before);
}

#[test]
fn draft_rules_are_enforced() {
    let mut w = founded(4);
    let c = Actor::commissioner();
    for ph in [
        Command::SettleFinances,
        Command::OpenDraft,
        Command::StartSeason,
    ] {
        assert!(w.execute(&c, ph).is_err(), "wrong phase");
    }
    w.execute(&c, Command::PlaySeason).unwrap();
    w.execute(&c, Command::PlayPlayoffs).unwrap();
    w.execute(&c, Command::SettleFinances).unwrap();
    w.execute(&c, Command::ProgressPlayers).unwrap();
    w.execute(&c, Command::OpenDraft).unwrap();
    let d = w.draft.clone().unwrap();
    let on_clock = d.order[0];
    let other = (on_clock + 1) % 30;
    let player = *d.remaining.iter().next().unwrap();
    let before = w.clone();
    // Not this club's turn.
    let out_of_turn = Command::DraftPlayer {
        team: other,
        player,
    };
    assert!(
        w.execute(&Actor::gm(protocol::TeamId(u32::from(other))), out_of_turn)
            .is_err()
    );
    // The draft cannot close early.
    assert!(w.execute(&c, Command::CloseDraft).is_err());
    assert!(w == before);
}

#[test]
fn free_agents_cannot_be_signed_below_their_asking_price() {
    let mut w = founded(6);
    let c = Actor::commissioner();
    w.execute(&c, Command::PlaySeason).unwrap();
    w.execute(&c, Command::PlayPlayoffs).unwrap();
    w.execute(&c, Command::SettleFinances).unwrap();
    w.execute(&c, Command::ProgressPlayers).unwrap();
    w.execute(&c, Command::OpenDraft).unwrap();
    while let Some(d) = &w.draft {
        let Some(&team) = d.order.get(d.next) else {
            break;
        };
        let gm = Actor::gm(protocol::TeamId(u32::from(team)));
        w.execute(&gm, Command::PassPick { team }).unwrap();
    }
    w.execute(&c, Command::CloseDraft).unwrap();
    let best = w
        .free_agents
        .iter()
        .map(|id| &w.players[id])
        .max_by(|a, b| a.quality.partial_cmp(&b.quality).unwrap())
        .expect("a free agent")
        .id;
    let gm = Actor::gm(protocol::TeamId(0));
    let cheap = Command::SignFreeAgent {
        team: 0,
        player: best,
        salary: 0.01,
        years: 2,
    };
    assert!(w.execute(&gm, cheap).is_err());
    let ask = world::scout::asking_salary(&w, &w.players[&best]);
    let fair = Command::SignFreeAgent {
        team: 0,
        player: best,
        salary: ask,
        years: 2,
    };
    // Whether the club can afford him depends on its budget, but a refusal
    // must be about money, and an acceptance must put him on the roster.
    match w.execute(&gm, fair) {
        Ok(_) => assert_eq!(w.players[&best].team, Some(0)),
        Err(e) => assert!(format!("{e}").contains("budget") || format!("{e}").contains("cap")),
    }
}
