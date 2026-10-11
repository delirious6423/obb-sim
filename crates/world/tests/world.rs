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

fn gm(team: u8) -> Actor {
    Actor::gm(protocol::TeamId(u32::from(team)))
}

fn to_progressed(seed: u64) -> (World, Vec<Event>) {
    let mut w = founded(seed);
    let c = Actor::commissioner();
    w.execute(&c, Command::PlaySeason).unwrap();
    w.execute(&c, Command::PlayPlayoffs).unwrap();
    w.execute(&c, Command::SettleFinances).unwrap();
    let ev = w.execute(&c, Command::ProgressPlayers).unwrap();
    (w, ev)
}

#[test]
fn progression_retires_players_and_expires_contracts() {
    let mut w = founded(21);
    let c = Actor::commissioner();
    w.execute(&c, Command::PlaySeason).unwrap();
    w.execute(&c, Command::PlayPlayoffs).unwrap();
    w.execute(&c, Command::SettleFinances).unwrap();
    let before = w.clone();
    let ev = w.execute(&c, Command::ProgressPlayers).unwrap();
    let Event::PlayersProgressed { updates, .. } = &ev[0] else {
        panic!("expected progression");
    };
    use world::command::Outcome;
    let retired: Vec<_> = updates
        .iter()
        .filter(|u| u.outcome == Outcome::Retired)
        .collect();
    let expired: Vec<_> = updates
        .iter()
        .filter(|u| u.outcome == Outcome::Expired)
        .collect();
    assert!(!retired.is_empty(), "somebody retires in a league of 3,000");
    assert!(!expired.is_empty(), "some contracts run out");
    for u in &retired {
        assert!(!w.players.contains_key(&u.player));
        assert!(w.retired.contains_key(&u.player));
        assert!(!w.free_agents.contains(&u.player));
        assert!(w.clubs.iter().all(|c| !c.players.contains(&u.player)));
    }
    for u in &expired {
        let p = &w.players[&u.player];
        assert!(p.team.is_none() && p.contract.is_none());
        assert!(w.free_agents.contains(&u.player));
        assert!(w.clubs.iter().all(|c| !c.players.contains(&u.player)));
        assert_eq!(before.players[&u.player].contract.unwrap().years, 1);
    }
    // Everyone else is a year older with a year less on the contract.
    for u in updates.iter().filter(|u| u.outcome == Outcome::Stays) {
        if let (Some(old), Some(new)) = (
            before.players[&u.player].contract,
            w.players[&u.player].contract,
        ) {
            assert_eq!(new.years, old.years - 1);
            assert_eq!(new.salary, old.salary);
        }
    }
}

#[test]
fn releasing_a_player_pays_off_the_contract() {
    let (mut w, _) = to_progressed(22);
    let (team, id, owed) = w
        .players
        .values()
        .filter_map(|p| {
            Some((
                p.team?,
                p.id,
                p.contract.map(|c| c.salary * f64::from(c.years))?,
            ))
        })
        .filter(|(t, _, owed)| *owed < w.clubs[usize::from(*t)].cash + world::state::CREDIT)
        .max_by(|a, b| a.2.partial_cmp(&b.2).unwrap())
        .unwrap();
    assert!(owed > 1.0);
    let cash = w.clubs[usize::from(team)].cash;
    let payroll = w.payroll(usize::from(team));
    let ev = w
        .execute(&gm(team), Command::ReleasePlayer { team, player: id })
        .unwrap();
    assert!(matches!(ev[0], Event::PlayerReleased { .. }));
    let c = &w.clubs[usize::from(team)];
    assert!((cash - c.cash - owed).abs() < 1e-9);
    assert!((c.spent - owed).abs() < 1e-9);
    assert!(w.payroll(usize::from(team)) < payroll);
    assert!(w.free_agents.contains(&id));
    assert!(w.players[&id].team.is_none());
}

#[test]
fn a_club_cannot_spend_below_its_credit_line() {
    let (mut w, _) = to_progressed(23);
    let (team, id) = w
        .players
        .values()
        .filter(|p| {
            p.team.is_some()
                && p.contract
                    .is_some_and(|c| c.salary * f64::from(c.years) > 2.0)
        })
        .map(|p| (p.team.unwrap(), p.id))
        .next()
        .unwrap();
    w.clubs[usize::from(team)].cash = -world::state::CREDIT;
    let before = w.clone();
    let err = w
        .execute(&gm(team), Command::ReleasePlayer { team, player: id })
        .unwrap_err();
    assert!(format!("{err}").contains("afford"), "{err}");
    assert!(w == before);
}

#[test]
fn levels_change_only_for_players_on_the_club_and_roster_limits_hold() {
    let (mut w, _) = to_progressed(24);
    let c = Actor::commissioner();
    let on0: Vec<_> = w.clubs[0].players.iter().copied().collect();
    let on1 = *w.clubs[1].players.iter().next().unwrap();
    // Another club's player cannot be moved.
    assert!(
        w.execute(
            &gm(0),
            Command::SetLevel {
                team: 0,
                player: on1,
                level: world::player::Level::Single
            }
        )
        .is_err()
    );
    // A player on the club can.
    let p = on0
        .iter()
        .find(|id| w.players[*id].level == world::player::Level::Majors)
        .copied()
        .unwrap();
    w.execute(
        &gm(0),
        Command::SetLevel {
            team: 0,
            player: p,
            level: world::player::Level::Single,
        },
    )
    .unwrap();
    assert_eq!(w.players[&p].level, world::player::Level::Single);
    // Skipping the draft and free agency would leave 12 in the majors: the
    // season cannot start, and a refused start changes nothing.
    w.execute(&c, Command::OpenDraft).unwrap();
    while let Some(d) = &w.draft {
        let Some(&t) = d.order.get(d.next) else { break };
        w.execute(&gm(t), Command::PassPick { team: t }).unwrap();
    }
    w.execute(&c, Command::CloseDraft).unwrap();
    let before = w.clone();
    assert!(w.execute(&c, Command::StartSeason).is_err());
    assert!(w == before);
    // Too many at one minor-league level is refused as well.
    for id in &on0 {
        w.execute(
            &gm(0),
            Command::SetLevel {
                team: 0,
                player: *id,
                level: world::player::Level::Double,
            },
        )
        .unwrap();
    }
    let err = w.execute(&c, Command::StartSeason).unwrap_err();
    assert!(format!("{err}").contains("club 0"), "{err}");
}

#[test]
fn drafting_signs_a_player_and_an_unaffordable_demand_walks_away() {
    let (mut w, _) = to_progressed(25);
    let c = Actor::commissioner();
    w.execute(&c, Command::OpenDraft).unwrap();
    // The agent's pick signs a drafted amateur to a club contract.
    let team = w.draft.as_ref().unwrap().order[0];
    let cmd = world::agents::draft_pick(&world::agents::GmView::new(&w, team).observe()).unwrap();
    let Command::DraftPlayer { player, .. } = cmd else {
        panic!("expected a pick")
    };
    let ev = w.execute(&gm(team), cmd).unwrap();
    assert!(matches!(ev[0], Event::PlayerDrafted { bonus: Some(_), .. }));
    assert_eq!(w.players[&player].team, Some(team));
    assert_eq!(w.players[&player].drafted.unwrap().pick, 1);
    // Late in the draft a prospect asking for more than the slot refuses.
    while let Some(d) = &w.draft {
        if d.next >= 250 {
            break;
        }
        let t = d.order[d.next];
        w.execute(&gm(t), Command::PassPick { team: t }).unwrap();
    }
    let d = w.draft.clone().unwrap();
    let t = d.order[d.next];
    let slot = world::player::slot_bonus((d.next + 1) as u16);
    let greedy = d
        .remaining
        .iter()
        .find(|id| {
            let p = &w.players[*id];
            p.origin != world::player::Origin::International
                && world::scout::amateur_demand(&w, p) > 1.5 * slot
        })
        .copied()
        .expect("some prospect asks too much for a late pick");
    let ev = w
        .execute(
            &gm(t),
            Command::DraftPlayer {
                team: t,
                player: greedy,
            },
        )
        .unwrap();
    assert!(matches!(ev[0], Event::PlayerDrafted { bonus: None, .. }));
    assert!(w.retired.contains_key(&greedy));
    assert!(!w.players.contains_key(&greedy));
}

#[test]
fn replaying_refuses_events_that_do_not_fit() {
    let (mut w, _) = to_progressed(26);
    let before = w.clone();
    let bad_order = Event::DraftOpened {
        year: w.year,
        prospects: vec![],
        order: vec![200],
        amateur_picks: 1,
        next_player: w.next_player,
    };
    assert!(w.apply(&bad_order).is_err());
    let reused_id = Event::DraftOpened {
        year: w.year,
        prospects: vec![w.players.values().next().unwrap().clone()],
        order: vec![0],
        amateur_picks: 1,
        next_player: w.next_player,
    };
    assert!(w.apply(&reused_id).is_err());
    let budget = Event::BudgetSet {
        team: 0,
        payroll: f64::NAN,
    };
    assert!(w.apply(&budget).is_err());
    assert!(w == before);
    let mut fresh_world = fresh(1);
    let bad = Event::PlayoffsPlayed {
        year: 2030,
        series: vec![],
        champion: 99,
    };
    assert!(fresh_world.apply(&bad).is_err());
}
