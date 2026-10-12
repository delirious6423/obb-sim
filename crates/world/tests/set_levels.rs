//! The roster AI's cuts: the worst players go, unless paying one off would put
//! the club below zero cash.

use world::agents::{GmObs, Seen, set_levels};
use world::command::Command;
use world::player::{Level, Origin, PlayerId};
use world::rating::Kind;
use world::state::{Phase, Stance};

fn seen(id: u32, now: f64, owed: f64) -> Seen {
    Seen {
        id: PlayerId(id),
        kind: Kind::Batter,
        age: 30,
        level: Level::Single,
        origin: Origin::College,
        now,
        peak: now,
        ask: 0.0,
        ask_years: 0,
        demand: 0.0,
        owed,
        salary: 0.0,
        years: 0,
    }
}

/// A full roster of batters plus two: the worst (id 0) is owed `owed`, the
/// rest nothing.
fn obs(cash: f64, owed: f64) -> GmObs {
    let room: usize = [Level::Majors, Level::Triple, Level::Double, Level::Single]
        .iter()
        .map(|l| l.capacity())
        .sum();
    let roster = (0..room as u32 + 2)
        .map(|i| seen(i, f64::from(i), if i == 0 { owed } else { 0.0 }))
        .collect();
    GmObs {
        team: 0,
        year: 2030,
        phase: Phase::FreeAgency,
        cash,
        budget: 150.0,
        payroll: 150.0,
        payroll_cap: 250.0,
        war_price: 5.0,
        stance: Stance::default(),
        roster,
        free_agents: Vec::new(),
        draft: None,
        trade: None,
    }
}

fn released(cmds: &[Command]) -> Vec<u32> {
    let mut ids: Vec<u32> = cmds
        .iter()
        .filter_map(|c| match c {
            Command::ReleasePlayer { player, .. } => Some(player.0),
            _ => None,
        })
        .collect();
    ids.sort_unstable();
    ids
}

#[test]
fn the_worst_players_go_when_the_club_can_pay_them_off() {
    assert_eq!(released(&set_levels(&obs(100.0, 50.0))), vec![0, 1]);
}

#[test]
fn a_contract_the_club_cannot_pay_off_stays() {
    assert_eq!(released(&set_levels(&obs(30.0, 50.0))), vec![1, 2]);
}

#[test]
fn the_roster_is_cut_to_size_even_when_nothing_is_affordable() {
    // Cash below zero: nobody can be paid off, so the cheapest go.
    assert_eq!(released(&set_levels(&obs(-5.0, 50.0))), vec![1, 2]);
}
