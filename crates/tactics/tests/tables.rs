use sim_core::{Env, GameParams, GameView, LINEUP, MAX_PITCHERS, Player, Team};
use tactics::Tables;

fn team(name: &str) -> Team {
    let rates = GameParams::reference().league;
    let p = |n: usize| Player {
        name: format!("{name} {n}"),
        rates,
    };
    Team::new(name, (0..LINEUP).map(p).collect(), (0..6).map(p).collect()).unwrap()
}

fn view(inning: u8, bat: u8, outs: u8, bases: [bool; 3], runs: [u16; 2]) -> GameView {
    GameView {
        inning,
        bat,
        outs,
        bases,
        runs,
        pitcher: 0,
        pitchers: 6,
        pitches_in_outing: 0,
        runs_in_outing: 0,
        used: [false; MAX_PITCHERS],
    }
}

fn tables(seed: u64) -> Tables {
    let env = Env::new(GameParams::reference()).unwrap();
    let teams: Vec<Team> = (0..4).map(|i| team(&format!("T{i}"))).collect();
    Tables::build(&env, &teams, 6_000, seed)
}

#[test]
fn run_expectancy_looks_like_baseball() {
    let t = tables(1);
    let empty0 = t.run_expectancy(0, 0);
    assert!((0.3..0.7).contains(&empty0), "{empty0}");
    assert!(t.run_expectancy(0b111, 0) > t.run_expectancy(0b001, 0));
    assert!(t.run_expectancy(0b001, 0) > t.run_expectancy(0b001, 2));
    assert!(t.run_expectancy(0b001, 2) > t.run_expectancy(0, 2));
    assert_eq!(t.run_expectancy(0, 3), 0.0);
}

#[test]
fn win_probability_and_leverage_make_sense() {
    let t = tables(2);
    let tied9 = view(9, 1, 1, [false; 3], [3, 3]);
    let up3 = view(8, 1, 1, [false; 3], [3, 6]);
    let blowout = view(3, 1, 1, [false; 3], [0, 9]);
    assert!(t.win_probability(&tied9) > 0.5 && t.win_probability(&tied9) < 0.85);
    assert!(t.win_probability(&up3) > 0.95);
    assert!(t.leverage(&tied9) > 2.0 * t.leverage(&blowout));
    assert!(t.leverage(&tied9) > t.leverage(&up3));
}

#[test]
fn tables_are_deterministic() {
    assert_eq!(tables(3), tables(3));
}

#[test]
fn steals_pay_only_when_safe_often_enough() {
    let t = tables(4);
    let v = view(3, 0, 0, [true, false, false], [0, 0]);
    let hi = t.steal_gain(&v, 1, 0.95).unwrap();
    let lo = t.steal_gain(&v, 1, 0.40).unwrap();
    assert!(hi > 0.0 && lo < 0.0);
    let blocked = view(3, 0, 0, [true, true, false], [0, 0]);
    assert!(t.steal_gain(&blocked, 1, 0.9).is_none());
}
