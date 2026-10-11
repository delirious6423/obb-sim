//! Findings from the phase 0 review: roles and teams are always enforced,
//! and loaded data is validated rather than trusted.

use league::{League, LeagueError, PermissionError, Session};
use protocol::{Actor, Command, CommandLog, Event, GameId, PlayerId, Role, TeamId};

const SEED: u64 = 7;

fn two_team_session() -> Session {
    let mut s = Session::new(SEED);
    for name in ["Harbor City Gulls", "Pine Ridge Foxes"] {
        s.submit(
            Actor::commissioner(),
            Command::CreateTeam { name: name.into() },
        )
        .unwrap();
    }
    for team in [0, 1] {
        s.submit(
            Actor::gm(TeamId(team)),
            Command::SignPlayer {
                team: TeamId(team),
                name: format!("Player {team}"),
                rating: 60,
            },
        )
        .unwrap();
    }
    s.submit(
        Actor::commissioner(),
        Command::ScheduleGame {
            home: TeamId(0),
            away: TeamId(1),
        },
    )
    .unwrap();
    s
}

// --- 1. Session::submit always enforces the role check ---

#[test]
fn submit_rejects_wrong_role_and_logs_nothing() {
    let mut s = two_team_session();
    let (before, len) = (s.league().clone(), s.log().len());
    let err = s
        .submit(Actor::gm(TeamId(0)), Command::PlayGame { game: GameId(0) })
        .unwrap_err();
    assert!(matches!(
        err,
        LeagueError::Permission(PermissionError::RoleNotAllowed { role: Role::Gm, .. })
    ));
    assert_eq!(s.league(), &before);
    assert_eq!(s.log().len(), len);
}

#[test]
fn replay_refuses_a_log_with_an_unauthorized_command() {
    let mut log = CommandLog::new();
    log.append(
        Actor::gm(TeamId(0)),
        Command::CreateTeam {
            name: "Sneaky".into(),
        },
    );
    let err = Session::replay(SEED, &log).unwrap_err();
    assert!(matches!(err, LeagueError::Permission(_)));
}

// --- 2. Agents are bound to a role plus a team ---

#[test]
fn gm_cannot_act_for_another_team() {
    let mut s = two_team_session();
    let (before, len) = (s.league().clone(), s.log().len());
    let err = s
        .submit(
            Actor::gm(TeamId(0)),
            Command::SignPlayer {
                team: TeamId(1),
                name: "Poached Player".into(),
                rating: 80,
            },
        )
        .unwrap_err();
    assert_eq!(
        err,
        LeagueError::Permission(PermissionError::WrongTeam {
            bound: TeamId(0),
            target: TeamId(1),
        })
    );
    assert_eq!(s.league(), &before);
    assert_eq!(s.log().len(), len);
}

#[test]
fn gm_without_a_team_is_rejected() {
    let mut s = two_team_session();
    let unbound = Actor {
        role: Role::Gm,
        team: None,
    };
    let err = s
        .submit(
            unbound,
            Command::SignPlayer {
                team: TeamId(0),
                name: "Nobody".into(),
                rating: 50,
            },
        )
        .unwrap_err();
    assert_eq!(
        err,
        LeagueError::Permission(PermissionError::MissingTeam(Role::Gm))
    );
}

#[test]
fn commissioner_acts_league_wide_but_must_have_no_team() {
    let mut s = two_team_session();
    // League-wide: schedules between any two teams.
    s.submit(
        Actor::commissioner(),
        Command::ScheduleGame {
            home: TeamId(1),
            away: TeamId(0),
        },
    )
    .unwrap();
    // A commissioner bound to a team is malformed.
    let bound = Actor {
        role: Role::Commissioner,
        team: Some(TeamId(0)),
    };
    let err = s
        .submit(bound, Command::PlayGame { game: GameId(0) })
        .unwrap_err();
    assert_eq!(
        err,
        LeagueError::Permission(PermissionError::CommissionerHasTeam(TeamId(0)))
    );
}

// --- 3. from_events and deserialization validate ---

fn team_created(id: u32) -> Event {
    Event::TeamCreated {
        team: TeamId(id),
        name: format!("Team {id}"),
    }
}

#[test]
fn from_events_rejects_events_that_do_not_fit() {
    // Skipped id.
    assert!(matches!(
        League::from_events(SEED, &[team_created(1)]),
        Err(LeagueError::NonSequentialId { kind: "team", .. })
    ));
    // Player for a team that does not exist.
    let ghost = Event::PlayerSigned {
        team: TeamId(0),
        player: PlayerId(0),
        name: "Ghost".into(),
        rating: 50,
    };
    assert_eq!(
        League::from_events(SEED, &[ghost]),
        Err(LeagueError::UnknownTeam(TeamId(0)))
    );
    // Tied result.
    let signed = |team: u32, player: u32| Event::PlayerSigned {
        team: TeamId(team),
        player: PlayerId(player),
        name: format!("Player {player}"),
        rating: 50,
    };
    let events = [
        team_created(0),
        team_created(1),
        signed(0, 0),
        signed(1, 1),
        Event::GameScheduled {
            game: GameId(0),
            home: TeamId(0),
            away: TeamId(1),
        },
        Event::GamePlayed {
            game: GameId(0),
            home_runs: 3,
            away_runs: 3,
        },
    ];
    assert!(matches!(
        League::from_events(SEED, &events),
        Err(LeagueError::TiedResult { .. })
    ));
    // Same game played twice.
    let mut twice = events[..5].to_vec();
    let played = Event::GamePlayed {
        game: GameId(0),
        home_runs: 4,
        away_runs: 2,
    };
    twice.push(played.clone());
    twice.push(played);
    assert_eq!(
        League::from_events(SEED, &twice),
        Err(LeagueError::AlreadyPlayed(GameId(0)))
    );
}

fn league_json(s: &Session) -> serde_json::Value {
    serde_json::to_value(s.league()).unwrap()
}

#[test]
fn deserializing_a_good_league_works() {
    let s = two_team_session();
    let back: League = serde_json::from_value(league_json(&s)).unwrap();
    assert_eq!(&back, s.league());
}

#[test]
fn deserializing_bad_data_is_rejected() {
    let s = two_team_session();
    // Each edit must hit a field that exists (checked by the assert on the
    // original value) and must be refused as corrupt, not for some other reason.
    let cases: [(&str, serde_json::Value, serde_json::Value, &str); 6] = [
        ("/next_team", 2.into(), 99.into(), "id counters"),
        ("/players/0/team", 0.into(), 42.into(), "unknown team"),
        ("/players/0/rating", 60.into(), 0.into(), "rating"),
        ("/games/0/away", 1.into(), 42.into(), "unknown team"),
        (
            "/games/0/result",
            serde_json::Value::Null,
            serde_json::json!([2, 2]),
            "tie",
        ),
        ("/teams/0/id", 0.into(), 5.into(), "match their keys"),
    ];
    for (path, original, bad, expect) in cases {
        let mut v = league_json(&s);
        assert_eq!(v.pointer(path), Some(&original), "{path} not as expected");
        *v.pointer_mut(path).unwrap() = bad;
        let err = serde_json::from_value::<League>(v).unwrap_err().to_string();
        assert!(err.contains("corrupt league data"), "{path}: {err}");
        assert!(err.contains(expect), "{path}: {err}");
    }
}

// --- Review gap: played games need players on both sides ---

#[test]
fn game_played_by_an_empty_roster_is_refused() {
    let mut events = vec![
        team_created(0),
        team_created(1),
        Event::GameScheduled {
            game: GameId(0),
            home: TeamId(0),
            away: TeamId(1),
        },
    ];
    events.push(Event::GamePlayed {
        game: GameId(0),
        home_runs: 3,
        away_runs: 1,
    });
    assert_eq!(
        League::from_events(SEED, &events),
        Err(LeagueError::EmptyRoster(TeamId(0)))
    );
}

#[test]
fn loaded_league_with_a_played_game_and_no_players_is_refused() {
    let events = [
        team_created(0),
        team_created(1),
        Event::GameScheduled {
            game: GameId(0),
            home: TeamId(0),
            away: TeamId(1),
        },
    ];
    let league = League::from_events(SEED, &events).unwrap();
    let mut v = serde_json::to_value(&league).unwrap();
    assert_eq!(v.pointer("/games/0/result"), Some(&serde_json::Value::Null));
    v["games"]["0"]["result"] = serde_json::json!([3, 1]);
    let err = serde_json::from_value::<League>(v).unwrap_err().to_string();
    assert!(err.contains("no players"), "{err}");
}
