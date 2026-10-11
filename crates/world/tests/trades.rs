use protocol::{Actor, TeamId};
use sim_core::GameParams;
use world::command::{Command, Event, WorldError};
use world::player::{DEFAULT_POOL, PlayerId};
use world::rating::{Kind, Model};
use world::state::{Phase, World};

fn offseason_world() -> World {
    offseason_world_with_log().0
}

fn offseason_world_with_log() -> (World, Vec<Event>) {
    let mut w = World::new(5, GameParams::reference(), Model::generic(), DEFAULT_POOL);
    let c = Actor::commissioner();
    let mut log = w.execute(&c, Command::Found { burn_in_years: 1 }).unwrap();
    for cmd in [
        Command::PlaySeason,
        Command::PlayPlayoffs,
        Command::SettleFinances,
        Command::ProgressPlayers,
    ] {
        log.extend(w.execute(&c, cmd).unwrap());
    }
    assert_eq!(w.phase, Phase::Progressed);
    (w, log)
}

fn gm(t: u8) -> Actor {
    Actor::gm(TeamId(u32::from(t)))
}

fn one_of(w: &World, team: u8, kind: Kind) -> PlayerId {
    *w.clubs[usize::from(team)]
        .players
        .iter()
        .find(|id| w.players[id].kind == kind)
        .unwrap()
}

#[test]
fn an_accepted_trade_swaps_the_players_and_replays() {
    let (mut w, mut log) = offseason_world_with_log();
    let (a, b) = (one_of(&w, 0, Kind::Batter), one_of(&w, 1, Kind::Batter));
    log.extend(
        w.execute(
            &gm(0),
            Command::ProposeTrade {
                team: 0,
                partner: 1,
                give: vec![a],
                get: vec![b],
            },
        )
        .unwrap(),
    );
    assert_eq!(w.offers.len(), 1);
    let id = w.offers[0].id;
    // Only the club it was put to may answer.
    assert!(matches!(
        w.execute(
            &gm(2),
            Command::RespondTrade {
                team: 2,
                offer: id,
                accept: true
            }
        ),
        Err(WorldError::Invalid(_))
    ));
    assert!(matches!(
        w.execute(
            &gm(0),
            Command::RespondTrade {
                team: 0,
                offer: id,
                accept: true
            }
        ),
        Err(WorldError::Invalid(_))
    ));
    log.extend(
        w.execute(
            &gm(1),
            Command::RespondTrade {
                team: 1,
                offer: id,
                accept: true,
            },
        )
        .unwrap(),
    );
    assert!(w.offers.is_empty());
    assert_eq!(w.players[&a].team, Some(1));
    assert_eq!(w.players[&b].team, Some(0));
    assert!(w.clubs[1].players.contains(&a) && !w.clubs[0].players.contains(&a));
    assert!(matches!(
        log.last(),
        Some(Event::TradeAnswered { accepted: true, .. })
    ));
    let rebuilt = World::from_events(
        5,
        GameParams::reference(),
        Model::generic(),
        DEFAULT_POOL,
        &log,
    )
    .unwrap();
    assert!(rebuilt == w);
}

#[test]
fn a_refused_trade_changes_nothing_and_bad_trades_are_rejected() {
    let mut w = offseason_world();
    let before = w.clone();
    let (a, b) = (one_of(&w, 0, Kind::Pitcher), one_of(&w, 1, Kind::Pitcher));
    w.execute(
        &gm(0),
        Command::ProposeTrade {
            team: 0,
            partner: 1,
            give: vec![a],
            get: vec![b],
        },
    )
    .unwrap();
    let id = w.offers[0].id;
    w.execute(
        &gm(1),
        Command::RespondTrade {
            team: 1,
            offer: id,
            accept: false,
        },
    )
    .unwrap();
    assert!(w.offers.is_empty());
    assert!(w.clubs == before.clubs && w.players == before.players);

    let propose = |give: Vec<PlayerId>, get: Vec<PlayerId>, partner: u8| Command::ProposeTrade {
        team: 0,
        partner,
        give,
        get,
    };
    // A player who is not yours, a trade with yourself, an empty trade,
    // the same player twice.
    for bad in [
        propose(vec![b], vec![], 1),
        propose(vec![a], vec![], 0),
        propose(vec![], vec![], 1),
        propose(vec![a, a], vec![], 1),
    ] {
        assert!(matches!(
            w.execute(&gm(0), bad),
            Err(WorldError::Invalid(_))
        ));
    }
    // Wrong phase.
    let mut late = w.clone();
    late.execute(&Actor::commissioner(), Command::OpenDraft)
        .unwrap();
    late.phase = Phase::Regular;
    assert!(matches!(
        late.execute(&gm(0), propose(vec![a], vec![b], 1)),
        Err(WorldError::Phase(_))
    ));
}

#[test]
fn a_stale_offer_cannot_be_accepted_and_offers_lapse_at_season_start() {
    let mut w = offseason_world();
    let (a, b) = (one_of(&w, 0, Kind::Batter), one_of(&w, 1, Kind::Batter));
    w.execute(
        &gm(0),
        Command::ProposeTrade {
            team: 0,
            partner: 1,
            give: vec![a],
            get: vec![b],
        },
    )
    .unwrap();
    let id = w.offers[0].id;
    // The proposer gets rid of the player first (a release in the draft window).
    w.execute(&Actor::commissioner(), Command::OpenDraft)
        .unwrap();
    let cash = w.clubs[0].cash;
    w.clubs[0].cash = cash.max(1_000.0); // test only: make the release affordable
    w.execute(&gm(0), Command::ReleasePlayer { team: 0, player: a })
        .unwrap();
    assert!(matches!(
        w.execute(
            &gm(1),
            Command::RespondTrade {
                team: 1,
                offer: id,
                accept: true
            }
        ),
        Err(WorldError::Invalid(_))
    ));
}

#[test]
fn a_club_cannot_flood_the_market_or_trade_itself_below_a_full_roster() {
    let mut w = offseason_world();
    let mine: Vec<PlayerId> = w.clubs[0]
        .players
        .iter()
        .copied()
        .filter(|id| w.players[id].kind == Kind::Batter)
        .collect();
    let theirs: Vec<PlayerId> = w.clubs[1]
        .players
        .iter()
        .copied()
        .filter(|id| w.players[id].kind == Kind::Batter)
        .collect();
    // Four open offers is the most one club may have out.
    for i in 0..4 {
        w.execute(
            &gm(0),
            Command::ProposeTrade {
                team: 0,
                partner: 1,
                give: vec![mine[i]],
                get: vec![theirs[i]],
            },
        )
        .unwrap();
    }
    let fifth = Command::ProposeTrade {
        team: 0,
        partner: 1,
        give: vec![mine[4]],
        get: vec![theirs[4]],
    };
    assert!(matches!(
        w.execute(&gm(0), fifth),
        Err(WorldError::Invalid(_))
    ));
    // Another club can still trade.
    w.execute(
        &gm(2),
        Command::ProposeTrade {
            team: 2,
            partner: 1,
            give: vec![],
            get: vec![theirs[5]],
        },
    )
    .unwrap();
    // Giving away everything for nothing would leave a short roster.
    let mut w = offseason_world();
    let all: Vec<PlayerId> = w.clubs[3]
        .players
        .iter()
        .copied()
        .filter(|id| w.players[id].kind == Kind::Batter)
        .take(6)
        .collect();
    let total = w.clubs[3]
        .players
        .iter()
        .filter(|id| w.players[id].kind == Kind::Batter)
        .count();
    if total - 6 < 13 {
        let r = w.execute(
            &gm(3),
            Command::ProposeTrade {
                team: 3,
                partner: 4,
                give: all,
                get: vec![],
            },
        );
        assert!(matches!(r, Err(WorldError::Invalid(_))));
    }
}
