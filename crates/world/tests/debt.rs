//! Debt: no rescue, and a club in debt is frozen out of new commitments
//! until profit or salary-shedding trades clear it.

use protocol::{Actor, TeamId};
use sim_core::GameParams;
use world::command::{Command, Event, WorldError};
use world::player::{DEFAULT_POOL, PlayerId};
use world::rating::{Kind, Model};
use world::scout::asking_salary;
use world::state::{MAJORS_PER_KIND, MIN_SALARY, Phase, World};

fn founded(seed: u64) -> World {
    let mut w = World::new(
        seed,
        GameParams::reference(),
        Model::generic(),
        DEFAULT_POOL,
    );
    w.execute(&Actor::commissioner(), Command::Found { burn_in_years: 1 })
        .expect("found");
    w
}

fn gm(t: u8) -> Actor {
    Actor::gm(TeamId(u32::from(t)))
}

fn run(w: &mut World, cmds: &[Command]) {
    for c in cmds {
        w.execute(&Actor::commissioner(), c.clone()).unwrap();
    }
}

fn to_progressed(seed: u64) -> World {
    let mut w = founded(seed);
    run(
        &mut w,
        &[
            Command::PlaySeason,
            Command::PlayPlayoffs,
            Command::SettleFinances,
            Command::ProgressPlayers,
        ],
    );
    assert_eq!(w.phase, Phase::Progressed);
    w
}

/// Through a draft in which every club passes, into free agency.
fn to_free_agency(seed: u64) -> World {
    let mut w = to_progressed(seed);
    run(&mut w, &[Command::OpenDraft]);
    while let Some(d) = &w.draft {
        let Some(&team) = d.order.get(d.next) else {
            break;
        };
        w.execute(&gm(team), Command::PassPick { team }).unwrap();
    }
    run(&mut w, &[Command::CloseDraft]);
    assert_eq!(w.phase, Phase::FreeAgency);
    w
}

fn refused(r: Result<Vec<Event>, WorldError>) -> String {
    match r {
        Err(WorldError::Invalid(m)) => m,
        other => panic!("expected a refusal, got {other:?}"),
    }
}

#[test]
fn debt_carries_over_with_no_rescue_and_holds_the_budget_at_the_floor() {
    let mut w = founded(7);
    run(&mut w, &[Command::PlaySeason, Command::PlayPlayoffs]);
    w.clubs[0].cash = -250.0;
    let before = w.payroll(0);
    run(&mut w, &[Command::SettleFinances]);
    let f = w.finances[0][0].clone();
    assert!(!f.rescued);
    assert_eq!(f.dividend, 0.0);
    let expect = -250.0 + f.revenue - f.payroll - f.other;
    assert!((w.clubs[0].cash - expect).abs() < 1e-9);
    assert!(w.clubs[0].cash < 0.0, "test needs the debt to remain");
    // No contract is voided.
    assert!((w.payroll(0) - before).abs() < 1e-9);
    let floor = w.rules.payroll_floor;
    assert!(w.clubs[0].budget <= floor + 1e-9);
    assert!((w.budget_limit(0) - floor).abs() < 1e-9);
    let o = Actor::owner(TeamId(0));
    let m = refused(w.execute(
        &o,
        Command::SetBudget {
            team: 0,
            payroll: floor + 1.0,
        },
    ));
    assert!(m.contains("lenders"), "{m}");
    w.execute(
        &o,
        Command::SetBudget {
            team: 0,
            payroll: floor,
        },
    )
    .unwrap();
    let again = World::restore(GameParams::reference(), Model::generic(), w.snapshot());
    assert!(again.is_ok(), "{:?}", again.err());
}

#[test]
fn a_club_in_debt_pays_no_draft_bonus() {
    let mut w = to_progressed(11);
    run(&mut w, &[Command::OpenDraft]);
    let mut debt_refusals = 0;
    while let Some(d) = &w.draft {
        let Some(&team) = d.order.get(d.next) else {
            break;
        };
        w.clubs[usize::from(team)].cash = -1.0;
        let prospects: Vec<PlayerId> = d.remaining.iter().copied().collect();
        let mut picked = false;
        for p in prospects {
            match w.execute(&gm(team), Command::DraftPlayer { team, player: p }) {
                Ok(ev) => {
                    // Only a player who will not sign (no bonus) may be taken.
                    assert!(matches!(ev[0], Event::PlayerDrafted { bonus: None, .. }));
                    picked = true;
                    break;
                }
                Err(WorldError::Invalid(m)) if m.contains("in debt") => debt_refusals += 1,
                Err(_) => {}
            }
        }
        if !picked {
            w.execute(&gm(team), Command::PassPick { team }).unwrap();
        }
    }
    assert!(debt_refusals > 0);
}

#[test]
fn a_club_in_debt_with_a_full_staff_signs_no_free_agent() {
    let mut w = to_free_agency(11);
    w.clubs[0].cash = -1.0;
    w.clubs[0].budget = w.rules.payroll_cap;
    let (mut dear, mut cheap) = (0, 0);
    let fas: Vec<PlayerId> = w.free_agents.iter().copied().collect();
    for id in fas {
        let p = &w.players[&id];
        let ask = asking_salary(&w, p).max(MIN_SALARY);
        let held = w.clubs[0]
            .players
            .iter()
            .filter(|x| w.players[x].kind == p.kind)
            .count();
        assert!(held >= MAJORS_PER_KIND, "test needs a full staff");
        let m = refused(w.execute(
            &gm(0),
            Command::SignFreeAgent {
                team: 0,
                player: id,
                salary: ask,
                years: 1,
            },
        ));
        assert!(m.contains("in debt"), "{m}");
        if ask > MIN_SALARY + 1e-9 {
            dear += 1;
        } else {
            cheap += 1;
        }
    }
    assert!(dear > 0 && cheap > 0);
}

#[test]
fn a_club_in_debt_may_fill_a_short_staff_at_the_minimum() {
    let mut w = to_free_agency(13);
    // Club 0 cuts its pitchers down to twelve, then falls into debt.
    w.clubs[0].cash = 1000.0;
    let pitchers: Vec<PlayerId> = w.clubs[0]
        .players
        .iter()
        .copied()
        .filter(|id| w.players[id].kind == Kind::Pitcher)
        .collect();
    for id in &pitchers[..pitchers.len() - (MAJORS_PER_KIND - 1)] {
        w.execute(
            &gm(0),
            Command::ReleasePlayer {
                team: 0,
                player: *id,
            },
        )
        .unwrap();
    }
    w.clubs[0].cash = -1.0;
    w.clubs[0].budget = w.rules.payroll_cap;
    let fa = |w: &World, cheap: bool| {
        w.free_agents
            .iter()
            .copied()
            .find(|id| {
                let p = &w.players[id];
                p.kind == Kind::Pitcher && (asking_salary(w, p) <= MIN_SALARY + 1e-9) == cheap
            })
            .unwrap()
    };
    let dear = fa(&w, false);
    let ask = asking_salary(&w, &w.players[&dear]);
    refused(w.execute(
        &gm(0),
        Command::SignFreeAgent {
            team: 0,
            player: dear,
            salary: ask,
            years: 1,
        },
    ));
    let cheap = fa(&w, true);
    w.execute(
        &gm(0),
        Command::SignFreeAgent {
            team: 0,
            player: cheap,
            salary: MIN_SALARY,
            years: 1,
        },
    )
    .unwrap();
    // Thirteen now: the next minimum-salary pitcher is refused.
    let next = fa(&w, true);
    refused(w.execute(
        &gm(0),
        Command::SignFreeAgent {
            team: 0,
            player: next,
            salary: MIN_SALARY,
            years: 1,
        },
    ));
}

#[test]
fn a_club_in_debt_may_shed_salary_by_trade_but_not_take_it_on() {
    let mut w = to_progressed(5);
    let by_salary = |w: &World, team: usize| {
        let mut v: Vec<(f64, PlayerId)> = w.clubs[team]
            .players
            .iter()
            .filter(|id| w.players[id].kind == Kind::Batter)
            .map(|id| (w.players[id].contract.map_or(0.0, |c| c.salary), *id))
            .collect();
        v.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        v
    };
    let mine = by_salary(&w, 0);
    let theirs = by_salary(&w, 1);
    let (dear, cheap) = (*mine.last().unwrap(), theirs[0]);
    assert!(
        dear.0 > cheap.0 + 1e-6,
        "test needs a salary gap {dear:?} {cheap:?}"
    );
    w.clubs[0].cash = -5.0;
    let m = refused(w.execute(
        &gm(0),
        Command::ProposeTrade {
            team: 0,
            partner: 1,
            give: vec![mine[0].1],
            get: vec![theirs.last().unwrap().1],
        },
    ));
    assert!(m.contains("in debt"), "{m}");
    w.execute(
        &gm(0),
        Command::ProposeTrade {
            team: 0,
            partner: 1,
            give: vec![dear.1],
            get: vec![cheap.1],
        },
    )
    .unwrap();
    let offer = w.offers[0].id;
    let before = w.payroll(0);
    w.execute(
        &gm(1),
        Command::RespondTrade {
            team: 1,
            offer,
            accept: true,
        },
    )
    .unwrap();
    assert!(w.payroll(0) < before - 1e-9);
}

/// A club in debt and short of a staff cannot get stuck when no free agent of
/// that kind asks the minimum: the cheapest one may be signed at the minimum
/// (and only he), the built-in general manager does so, and the season
/// starts with every roster in order.
#[test]
fn a_club_in_debt_fills_a_short_staff_even_with_nobody_at_the_minimum() {
    use world::agents::{GmView, free_agency_step, set_levels};
    let mut w = to_free_agency(13);
    w.clubs[0].cash = 1000.0;
    let pitchers: Vec<PlayerId> = w.clubs[0]
        .players
        .iter()
        .copied()
        .filter(|id| w.players[id].kind == Kind::Pitcher)
        .collect();
    for id in &pitchers[..pitchers.len() - (MAJORS_PER_KIND - 2)] {
        w.execute(
            &gm(0),
            Command::ReleasePlayer {
                team: 0,
                player: *id,
            },
        )
        .unwrap();
    }
    w.clubs[0].cash = -1.0;
    // Nobody left in the pool asks the minimum for a pitcher.
    let cheap: Vec<PlayerId> = w
        .free_agents
        .iter()
        .copied()
        .filter(|id| {
            let p = &w.players[id];
            p.kind == Kind::Pitcher && asking_salary(&w, p) <= MIN_SALARY + 1e-9
        })
        .collect();
    for id in &cheap {
        w.free_agents.remove(id);
    }
    let fill = w
        .short_fill(0, Kind::Pitcher)
        .expect("a pitcher to fill with");
    assert!(asking_salary(&w, &w.players[&fill]) > MIN_SALARY + 1e-9);
    // Anyone else at the minimum is refused, and so is the fill above it.
    let other = w
        .free_agents
        .iter()
        .copied()
        .find(|id| *id != fill && w.players[id].kind == Kind::Pitcher)
        .unwrap();
    for (player, salary) in [(other, MIN_SALARY), (fill, MIN_SALARY + 1.0)] {
        refused(w.execute(
            &gm(0),
            Command::SignFreeAgent {
                team: 0,
                player,
                salary,
                years: 1,
            },
        ));
    }
    // The built-in general manager fills both spots at the minimum.
    while let Some(cmd) = free_agency_step(&GmView::new(&w, 0).observe()) {
        w.execute(&gm(0), cmd).unwrap();
    }
    let held = w.clubs[0]
        .players
        .iter()
        .filter(|id| w.players[id].kind == Kind::Pitcher)
        .count();
    assert_eq!(held, MAJORS_PER_KIND);
    assert!(w.in_debt(0));
    for t in 0..w.clubs.len() as u8 {
        for cmd in set_levels(&GmView::new(&w, t).observe()) {
            w.execute(&gm(t), cmd).unwrap();
        }
    }
    assert!(
        w.roster_problems(0).is_empty(),
        "{:?}",
        w.roster_problems(0)
    );
    run(&mut w, &[Command::StartSeason]);
}

/// The lenders' limit depends on cash and on how it is computed, so it is
/// checked when the owner asks (`SetBudget`), not when the event is applied:
/// an old event log replays even if the limit has changed since.
#[test]
fn a_budget_event_replays_whatever_the_lenders_limit() {
    let mut w = to_progressed(9);
    w.clubs[0].cash = -50.0;
    let floor = w.rules.payroll_floor;
    assert!((w.budget_limit(0) - floor).abs() < 1e-9);
    w.apply(&Event::BudgetSet {
        team: 0,
        payroll: floor + 20.0,
    })
    .expect("inside the floor and the cap");
    assert!((w.clubs[0].budget - floor - 20.0).abs() < 1e-9);
    // The floor and the cap still hold.
    assert!(
        w.apply(&Event::BudgetSet {
            team: 0,
            payroll: w.rules.payroll_cap + 1.0,
        })
        .is_err()
    );
}

/// A club in debt pays no dividend, even in a year whose profit clears it.
#[test]
fn a_club_that_starts_the_year_in_debt_pays_no_dividend() {
    let mut w = founded(7);
    run(&mut w, &[Command::PlaySeason, Command::PlayPlayoffs]);
    w.clubs[0].cash = -0.01;
    w.clubs[1].cash = 1000.0;
    run(&mut w, &[Command::SettleFinances]);
    assert_eq!(w.finances[0][0].dividend, 0.0);
    assert!(
        w.finances[1][0].dividend > 0.0,
        "test needs a dividend payer"
    );
}
