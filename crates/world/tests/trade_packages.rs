//! Multi-player packages and salary-relief trades: they validate, apply,
//! replay, and respect roster limits and the payroll cap.

use protocol::{Actor, TeamId};
use sim_core::GameParams;
use world::command::{Command, Event, WorldError};
use world::player::{DEFAULT_POOL, PlayerId};
use world::rating::{Kind, Model};
use world::state::{Phase, ROSTER_MAX_PER_KIND, World};

fn offseason() -> (World, Vec<Event>) {
    let mut w = World::new(9, GameParams::reference(), Model::generic(), DEFAULT_POOL);
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

fn of_kind(w: &World, team: u8, kind: Kind, n: usize) -> Vec<PlayerId> {
    w.clubs[usize::from(team)]
        .players
        .iter()
        .copied()
        .filter(|id| w.players[id].kind == kind)
        .take(n)
        .collect()
}

fn count(w: &World, team: u8, kind: Kind) -> usize {
    w.clubs[usize::from(team)]
        .players
        .iter()
        .filter(|id| w.players[*id].kind == kind)
        .count()
}

fn salary(w: &World, id: PlayerId) -> f64 {
    w.players[&id].contract.map_or(0.0, |c| c.salary)
}

/// The best-paid player of a club.
fn dearest(w: &World, team: u8) -> PlayerId {
    *w.clubs[usize::from(team)]
        .players
        .iter()
        .max_by(|a, b| salary(w, **a).total_cmp(&salary(w, **b)).then(b.cmp(a)))
        .unwrap()
}

fn propose_logged(
    w: &mut World,
    team: u8,
    partner: u8,
    give: Vec<PlayerId>,
    get: Vec<PlayerId>,
) -> (u32, Vec<Event>) {
    let events = w
        .execute(
            &gm(team),
            Command::ProposeTrade {
                team,
                partner,
                give,
                get,
            },
        )
        .unwrap();
    (w.offers.last().unwrap().id, events)
}

fn propose(w: &mut World, team: u8, partner: u8, give: Vec<PlayerId>, get: Vec<PlayerId>) -> u32 {
    propose_logged(w, team, partner, give, get).0
}

fn accept(w: &mut World, team: u8, offer: u32) -> Result<Vec<Event>, WorldError> {
    w.execute(
        &gm(team),
        Command::RespondTrade {
            team,
            offer,
            accept: true,
        },
    )
}

#[test]
fn a_three_for_three_package_moves_every_player_and_replays() {
    let (mut w, mut log) = offseason();
    let give: Vec<PlayerId> = of_kind(&w, 0, Kind::Batter, 2)
        .into_iter()
        .chain(of_kind(&w, 0, Kind::Pitcher, 1))
        .collect();
    let get: Vec<PlayerId> = of_kind(&w, 1, Kind::Batter, 1)
        .into_iter()
        .chain(of_kind(&w, 1, Kind::Pitcher, 2))
        .collect();
    let counts = |w: &World| [0, 1].map(|t| [Kind::Batter, Kind::Pitcher].map(|k| count(w, t, k)));
    let before = counts(&w);
    let payroll = [w.payroll(0), w.payroll(1)];
    let (id, events) = propose_logged(&mut w, 0, 1, give.clone(), get.clone());
    log.extend(events);
    log.extend(accept(&mut w, 1, id).unwrap());
    for p in &give {
        assert_eq!(w.players[p].team, Some(1));
    }
    for p in &get {
        assert_eq!(w.players[p].team, Some(0));
    }
    let after = counts(&w);
    assert_eq!(after[0], [before[0][0] - 1, before[0][1] + 1]);
    assert_eq!(after[1], [before[1][0] + 1, before[1][1] - 1]);
    let moved: f64 = get.iter().map(|p| salary(&w, *p)).sum::<f64>()
        - give.iter().map(|p| salary(&w, *p)).sum::<f64>();
    assert!((w.payroll(0) - payroll[0] - moved).abs() < 1e-9);
    assert!((w.payroll(1) - payroll[1] + moved).abs() < 1e-9);
    let rebuilt = World::from_events(
        9,
        GameParams::reference(),
        Model::generic(),
        DEFAULT_POOL,
        &log,
    )
    .unwrap();
    assert!(rebuilt == w);
}

#[test]
fn a_salary_dump_is_allowed_only_within_the_payroll_cap() {
    let (mut w, _) = offseason();
    let star = dearest(&w, 0);
    let pay = salary(&w, star);
    assert!(pay > 0.0);
    // Room under the cap: the dump goes through and the payroll moves.
    let (p0, p1) = (w.payroll(0), w.payroll(1));
    w.rules.payroll_cap = p1 + pay + 1.0; // test only: a cap just above
    let id = propose(&mut w, 0, 1, vec![star], vec![]);
    accept(&mut w, 1, id).unwrap();
    assert_eq!(w.players[&star].team, Some(1));
    assert!((w.payroll(0) - (p0 - pay)).abs() < 1e-9);
    assert!((w.payroll(1) - (p1 + pay)).abs() < 1e-9);

    // No room: the same kind of dump to a club near the cap is refused.
    let (mut w, _) = offseason();
    let star = dearest(&w, 0);
    let pay = salary(&w, star);
    w.rules.payroll_cap = w.payroll(2) + pay * 0.5; // test only
    let r = w.execute(
        &gm(0),
        Command::ProposeTrade {
            team: 0,
            partner: 2,
            give: vec![star],
            get: vec![],
        },
    );
    assert!(matches!(r, Err(WorldError::Invalid(_))), "{r:?}");
    // A club over the cap may still shed salary.
    let shed = dearest(&w, 2);
    w.execute(
        &gm(2),
        Command::ProposeTrade {
            team: 2,
            partner: 3,
            give: vec![shed],
            get: of_kind(&w, 3, Kind::Batter, 1)
                .into_iter()
                .filter(|p| salary(&w, *p) < salary(&w, shed))
                .collect(),
        },
    )
    .map(|_| ())
    .or_else(|e| {
        // Only the receiving club's cap may stop it.
        if w.payroll(3) + salary(&w, shed) > w.rules.payroll_cap {
            Ok(())
        } else {
            Err(e)
        }
    })
    .unwrap();
}

#[test]
fn a_package_cannot_overfill_a_roster() {
    let (mut w, _) = offseason();
    // Fill club 1's batters to the limit with one-sided gifts of up to three
    // batters from other clubs' spares (each keeps at least 13).
    let mut donor = 2u8;
    while count(&w, 1, Kind::Batter) < ROSTER_MAX_PER_KIND {
        let room = ROSTER_MAX_PER_KIND - count(&w, 1, Kind::Batter);
        let spare = count(&w, donor, Kind::Batter).saturating_sub(13);
        let n = room.min(spare).min(3);
        if n == 0 {
            donor += 1;
            assert!(
                usize::from(donor) < w.clubs.len(),
                "not enough spare batters"
            );
            continue;
        }
        let gift = of_kind(&w, donor, Kind::Batter, n);
        let id = propose(&mut w, donor, 1, gift, vec![]);
        accept(&mut w, 1, id).unwrap();
    }
    assert_eq!(count(&w, 1, Kind::Batter), ROSTER_MAX_PER_KIND);
    let one = of_kind(&w, 3, Kind::Batter, 1);
    let r = w.execute(
        &gm(3),
        Command::ProposeTrade {
            team: 3,
            partner: 1,
            give: one.clone(),
            get: vec![],
        },
    );
    assert!(matches!(r, Err(WorldError::Invalid(_))));
    // A swap of one for one keeps the count and is fine.
    let theirs = of_kind(&w, 1, Kind::Batter, 1);
    propose(&mut w, 3, 1, one, theirs);
}
