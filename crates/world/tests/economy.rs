//! Owner and commissioner levers: ticket price, ballpark, stance, rules.

use protocol::{Actor, TeamId};
use sim_core::GameParams;
use world::command::{Command, Event};
use world::player::DEFAULT_POOL;
use world::rating::Model;
use world::state::{CREDIT, Phase, Rules, Stance, TICKET_RANGE, World};

fn founded(seed: u64) -> World {
    let mut w = World::new(
        seed,
        GameParams::reference(),
        Model::generic(),
        DEFAULT_POOL,
    );
    w.execute(&Actor::commissioner(), Command::Found { burn_in_years: 2 })
        .expect("found");
    w
}

fn owner(team: u8) -> Actor {
    Actor::owner(TeamId(u32::from(team)))
}

/// Through the season and the books, into the offseason.
fn offseason(seed: u64) -> World {
    let mut w = founded(seed);
    let c = Actor::commissioner();
    w.execute(&c, Command::PlaySeason).unwrap();
    w.execute(&c, Command::PlayPlayoffs).unwrap();
    w.execute(&c, Command::SettleFinances).unwrap();
    assert_eq!(w.phase, Phase::Offseason);
    w
}

/// Settle a season with every club charging `price`; returns (attendance, gate) per club.
fn settled_with(price: f64, seed: u64) -> Vec<(f64, f64)> {
    let mut w = founded(seed);
    for c in &mut w.clubs {
        c.stadium.ticket = price;
    }
    let c = Actor::commissioner();
    w.execute(&c, Command::PlaySeason).unwrap();
    w.execute(&c, Command::PlayPlayoffs).unwrap();
    w.execute(&c, Command::SettleFinances).unwrap();
    w.finances
        .iter()
        .map(|f| (f[0].attendance, f[0].gate))
        .collect()
}

#[test]
fn a_higher_price_draws_a_smaller_crowd_and_the_park_caps_it() {
    let cheap = settled_with(20.0, 5);
    let dear = settled_with(60.0, 5);
    for (a, b) in cheap.iter().zip(&dear) {
        assert!(a.0 >= b.0, "crowd must not grow with price");
    }
    // At twice the reference price demand is far below any capacity, so
    // the crowd is strictly smaller there.
    assert!(cheap.iter().zip(&dear).all(|(a, b)| a.0 > b.0));
    let w = founded(5);
    for (c, (crowd, _)) in w.clubs.iter().zip(&cheap) {
        assert!(*crowd <= c.stadium.capacity + 1e-9);
    }
}

#[test]
fn at_the_reference_price_gate_receipts_are_the_market_share_of_local_revenue() {
    // Default parks hold the default crowd except in the largest markets,
    // so the league's gate is close to the old model's gate share.
    let g = settled_with(35.0, 6);
    let total: f64 = g.iter().map(|x| x.1).sum();
    let per_club = total / g.len() as f64;
    assert!(
        (60.0..130.0).contains(&per_club),
        "gate per club {per_club}"
    );
}

#[test]
fn owner_levers_work_only_in_the_offseason_for_the_owner() {
    let mut w = founded(7);
    let cmd = Command::SetTicketPrice {
        team: 3,
        price: 40.0,
    };
    // Season: closed.
    assert!(w.execute(&owner(3), cmd.clone()).is_err());
    let mut w = offseason(7);
    // Wrong role, wrong club.
    assert!(w.execute(&Actor::gm(TeamId(3)), cmd.clone()).is_err());
    assert!(w.execute(&owner(4), cmd.clone()).is_err());
    let ev = w.execute(&owner(3), cmd).unwrap();
    assert!(matches!(ev[0], Event::TicketSet { team: 3, .. }));
    assert_eq!(w.clubs[3].stadium.ticket, 40.0);
    for bad in [TICKET_RANGE.0 - 1.0, TICKET_RANGE.1 + 1.0, f64::NAN] {
        let before = w.clone();
        assert!(
            w.execute(
                &owner(3),
                Command::SetTicketPrice {
                    team: 3,
                    price: bad
                }
            )
            .is_err()
        );
        assert!(w == before);
    }
    let ev = w
        .execute(
            &owner(3),
            Command::SetStance {
                team: 3,
                stance: Stance::Rebuild,
            },
        )
        .unwrap();
    assert!(matches!(ev[0], Event::StanceSet { .. }));
    assert_eq!(w.clubs[3].stance, Stance::Rebuild);
}

#[test]
fn a_ballpark_project_costs_cash_and_adds_seats_and_quality() {
    let mut w = offseason(8);
    let before = w.clubs[2].clone();
    w.execute(
        &owner(2),
        Command::BuildStadium {
            team: 2,
            amount: 60.0,
        },
    )
    .unwrap();
    let after = &w.clubs[2];
    assert!((before.cash - after.cash - 60.0).abs() < 1e-9);
    assert!(after.stadium.capacity > before.stadium.capacity);
    assert!(after.stadium.quality > before.stadium.quality);
    // A club at its credit line cannot start a project.
    w.clubs[2].cash = -CREDIT + 1.0;
    let snapshot = w.clone();
    assert!(
        w.execute(
            &owner(2),
            Command::BuildStadium {
                team: 2,
                amount: 5.0
            }
        )
        .is_err()
    );
    assert!(w == snapshot);
    assert!(
        w.execute(
            &owner(1),
            Command::BuildStadium {
                team: 1,
                amount: -3.0
            }
        )
        .is_err()
    );
}

#[test]
fn the_commissioner_sets_rules_within_limits_and_budgets_follow() {
    let mut w = offseason(9);
    let c = Actor::commissioner();
    let mut rules: Rules = w.rules.clone();
    rules.payroll_cap = 220.0;
    rules.payroll_floor = 75.0;
    // Owners may not.
    assert!(
        w.execute(
            &owner(0),
            Command::SetRules {
                rules: rules.clone()
            }
        )
        .is_err()
    );
    w.execute(
        &c,
        Command::SetRules {
            rules: rules.clone(),
        },
    )
    .unwrap();
    assert_eq!(w.rules, rules);
    assert!(w.clubs.iter().all(|c| (75.0..=220.0).contains(&c.budget)));
    // A move of more than a tenth is refused even inside the absolute limits.
    let before = w.clone();
    let mut jump = w.rules.clone();
    jump.payroll_cap = 150.0;
    assert!(w.execute(&c, Command::SetRules { rules: jump }).is_err());
    assert!(w == before);
    // Out of limits is refused and changes nothing.
    let before = w.clone();
    let mut bad = rules;
    bad.revenue_sharing = 0.9;
    assert!(w.execute(&c, Command::SetRules { rules: bad }).is_err());
    assert!(w == before);
}

#[test]
fn rules_cannot_change_in_the_season() {
    let mut w = founded(10);
    let rules = w.rules.clone();
    assert!(
        w.execute(&Actor::commissioner(), Command::SetRules { rules })
            .is_err()
    );
}

#[test]
fn economy_events_survive_json_and_replay() {
    let mut w = offseason(11);
    let mut log = Vec::new();
    for (a, cmd) in [
        (
            owner(5),
            Command::SetTicketPrice {
                team: 5,
                price: 52.0,
            },
        ),
        (
            owner(5),
            Command::BuildStadium {
                team: 5,
                amount: 30.0,
            },
        ),
        (
            owner(5),
            Command::SetStance {
                team: 5,
                stance: Stance::Contend,
            },
        ),
    ] {
        log.extend(w.execute(&a, cmd).unwrap());
    }
    let json = serde_json::to_string(&log).unwrap();
    let back: Vec<Event> = serde_json::from_str(&json).unwrap();
    assert_eq!(log, back);
}

#[test]
fn rules_change_at_most_once_a_year() {
    let mut w = offseason(12);
    let c = Actor::commissioner();
    let mut rules = w.rules.clone();
    rules.payroll_cap = 250.0;
    w.execute(
        &c,
        Command::SetRules {
            rules: rules.clone(),
        },
    )
    .unwrap();
    // A second change in the same year is refused, however small, so the
    // yearly limit cannot be dodged by repeating it.
    rules.payroll_cap = 260.0;
    let before = w.clone();
    assert!(w.execute(&c, Command::SetRules { rules }).is_err());
    assert!(w == before);
    // The year of the change survives a snapshot.
    let snap = w.snapshot();
    assert_eq!(snap.rules_set, Some(w.year));
}

#[test]
fn a_rescued_club_loses_the_contracts_that_broke_it() {
    let mut w = founded(7);
    let c = Actor::commissioner();
    w.execute(&c, Command::PlaySeason).unwrap();
    w.execute(&c, Command::PlayPlayoffs).unwrap();
    // Far below the credit line, so settlement must rescue club 0.
    w.clubs[0].cash = -10.0 * CREDIT;
    let others: Vec<f64> = (1..w.clubs.len()).map(|t| w.payroll(t)).collect();
    let before = w.payroll(0);
    assert!(
        before > w.rules.payroll_floor,
        "test needs a payroll to cut"
    );
    w.execute(&c, Command::SettleFinances).unwrap();
    assert!(w.finances[0][0].rescued);
    assert!(w.payroll(0) <= w.rules.payroll_floor + 1e-9);
    assert!(w.payroll(0) < before);
    // The voided players are free agents, nobody else is touched.
    for (t, p) in others.iter().enumerate() {
        assert!((w.payroll(t + 1) - p).abs() < 1e-9);
    }
    let again = World::restore(GameParams::reference(), Model::generic(), w.snapshot());
    assert!(again.is_ok(), "{:?}", again.err());
}

/// A rescue makes the crowd of that year useless to the owner: it was drawn
/// at the old price. The owner's view reports no crowd for it, while a club
/// that was not rescued still reports its real crowd.
#[test]
fn a_rescued_year_reports_no_crowd_to_the_owner() {
    use world::agents::{OwnerObs, last_crowd};
    let mut w = founded(7);
    let c = Actor::commissioner();
    w.execute(&c, Command::PlaySeason).unwrap();
    w.execute(&c, Command::PlayPlayoffs).unwrap();
    w.clubs[0].cash = -10.0 * CREDIT;
    w.execute(&c, Command::SettleFinances).unwrap();
    assert!(w.finances[0][0].rescued);
    assert!(w.finances[0][0].attendance > 0.0, "test needs a real crowd");

    let rescued = OwnerObs::new(&w, 0);
    assert_eq!(rescued.attendance_prev, 0.0);
    assert_eq!(rescued.gate_prev, 0.0);
    assert!(!rescued.sold_out());
    assert_eq!(last_crowd(&w, 0), (0.0, 0.0));

    let healthy = OwnerObs::new(&w, 1);
    assert!(!w.finances[1][0].rescued);
    assert_eq!(healthy.attendance_prev, w.finances[1][0].attendance);
    assert_eq!(healthy.gate_prev, w.finances[1][0].gate);
}
