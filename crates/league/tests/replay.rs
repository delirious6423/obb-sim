use league::{League, LeagueError, Session};
use protocol::{Actor, Command, Event, GameId, TeamId};

const SEED: u64 = 2024;

fn sign(team: u32, name: &str, rating: u8) -> (Actor, Command) {
    (
        Actor::gm(TeamId(team)),
        Command::SignPlayer {
            team: TeamId(team),
            name: name.to_owned(),
            rating,
        },
    )
}

fn commish(command: Command) -> (Actor, Command) {
    (Actor::commissioner(), command)
}

/// Fictional teams and players only.
fn script() -> Vec<(Actor, Command)> {
    vec![
        commish(Command::CreateTeam {
            name: "Harbor City Gulls".into(),
        }),
        commish(Command::CreateTeam {
            name: "Pine Ridge Foxes".into(),
        }),
        sign(0, "Alden Marsh", 72),
        sign(0, "Teo Brandt", 64),
        sign(1, "Nico Vale", 58),
        sign(1, "Orrin Pike", 61),
        commish(Command::ScheduleGame {
            home: TeamId(0),
            away: TeamId(1),
        }),
        commish(Command::ScheduleGame {
            home: TeamId(1),
            away: TeamId(0),
        }),
        commish(Command::PlayGame { game: GameId(0) }),
        commish(Command::PlayGame { game: GameId(1) }),
    ]
}

fn run(commands: Vec<(Actor, Command)>) -> Session {
    let mut session = Session::new(SEED);
    for (actor, command) in commands {
        session
            .submit(actor, command)
            .expect("script command accepted");
    }
    session
}

#[test]
fn replay_rebuilds_identical_state() {
    let live = run(script());
    let replayed = Session::replay(SEED, live.log()).expect("replay");
    assert_eq!(replayed.league(), live.league());
    assert_eq!(replayed.log(), live.log());
    assert!(live.league().games().all(|g| g.result.is_some()));
}

#[test]
fn replay_of_events_rebuilds_identical_state() {
    let mut session = Session::new(SEED);
    let mut events: Vec<Event> = Vec::new();
    for (actor, command) in script() {
        events.extend(session.submit(actor, command).expect("accepted"));
    }
    assert_eq!(
        &League::from_events(SEED, &events).expect("valid events"),
        session.league()
    );
}

#[test]
fn snapshot_plus_log_tail_rebuilds_identical_state() {
    let mut session = Session::new(SEED);
    let mut snapshot = None;
    for (i, (actor, command)) in script().into_iter().enumerate() {
        session.submit(actor, command).expect("accepted");
        if i == 5 {
            snapshot = Some(session.snapshot());
        }
    }
    let snapshot = snapshot.expect("snapshot taken");
    assert_eq!(snapshot.applied, 6);
    let resumed = Session::resume(snapshot, session.log()).expect("resume");
    assert_eq!(resumed.league(), session.league());
}

#[test]
fn snapshot_survives_a_serialization_round_trip() {
    let live = run(script());
    let snapshot = live.snapshot();
    let json = serde_json::to_string(&snapshot).expect("serialize");
    let back: protocol::Snapshot<League> = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(back, snapshot);
    let resumed = Session::resume(back, live.log()).expect("resume from deserialized snapshot");
    assert_eq!(resumed.league(), live.league());
}

#[test]
fn log_survives_a_serialization_round_trip() {
    let live = run(script());
    let json = serde_json::to_string(live.log()).expect("serialize");
    let log: protocol::CommandLog = serde_json::from_str(&json).expect("deserialize");
    let replayed = Session::replay(SEED, &log).expect("replay");
    assert_eq!(replayed.league(), live.league());
}

#[test]
fn rejected_commands_change_nothing_and_are_not_logged() {
    let mut session = run(script());
    let before_league = session.league().clone();
    let before_len = session.log().len();

    let err = session
        .submit(Actor::commissioner(), Command::PlayGame { game: GameId(0) })
        .unwrap_err();
    assert_eq!(err, LeagueError::AlreadyPlayed(GameId(0)));

    let (actor, command) = sign(9, "Ghost Player", 50);
    let err = session.submit(actor, command).unwrap_err();
    assert_eq!(err, LeagueError::UnknownTeam(TeamId(9)));

    assert_eq!(session.league(), &before_league);
    assert_eq!(session.log().len(), before_len);
}

#[test]
fn game_result_does_not_depend_on_play_order() {
    let setup: Vec<_> = script().into_iter().take(8).collect();

    let mut forward = run(setup.clone());
    forward
        .submit(Actor::commissioner(), Command::PlayGame { game: GameId(0) })
        .unwrap();
    forward
        .submit(Actor::commissioner(), Command::PlayGame { game: GameId(1) })
        .unwrap();

    let mut reverse = run(setup);
    reverse
        .submit(Actor::commissioner(), Command::PlayGame { game: GameId(1) })
        .unwrap();
    reverse
        .submit(Actor::commissioner(), Command::PlayGame { game: GameId(0) })
        .unwrap();

    assert_eq!(forward.league(), reverse.league());
}

#[test]
fn snapshot_ahead_of_log_is_refused() {
    let live = run(script());
    let mut snapshot = live.snapshot();
    snapshot.applied += 1;
    let err = Session::resume(snapshot, live.log()).unwrap_err();
    assert!(matches!(err, LeagueError::SnapshotAheadOfLog { .. }));
}
