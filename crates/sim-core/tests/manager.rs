use sim_core::{
    Baseline, Env, GameParams, GameView, LINEUP, Manager, NullSink, Player, ReliefCall, StealCall,
    Team, simulate_game, simulate_game_managed,
};

fn team(name: &str) -> Team {
    let rates = GameParams::reference().league;
    let p = |n: usize| Player {
        name: format!("{name} {n}"),
        rates,
    };
    Team::new(name, (0..LINEUP).map(p).collect(), (0..6).map(p).collect()).unwrap()
}

fn env() -> Env {
    Env::new(GameParams::reference()).unwrap()
}

#[test]
fn the_baseline_manager_changes_nothing() {
    let (a, h) = (team("A"), team("H"));
    for g in 0..200 {
        let plain = simulate_game(&env(), &a, &h, 5, g, &mut NullSink);
        let managed = simulate_game_managed(
            &env(),
            &a,
            &h,
            5,
            g,
            &mut NullSink,
            [&mut Baseline, &mut Baseline],
        );
        assert_eq!(plain, managed);
    }
}

struct Never;
impl Manager for Never {
    fn steal(&mut self, _: &GameView, _: u8) -> StealCall {
        StealCall::Hold
    }
}

struct Always;
impl Manager for Always {
    fn steal(&mut self, _: &GameView, _: u8) -> StealCall {
        StealCall::Go
    }
}

#[test]
fn managers_control_stealing() {
    let (a, h) = (team("A"), team("H"));
    let (mut held, mut sent) = (0, 0);
    for g in 0..300 {
        let n = simulate_game_managed(
            &env(),
            &a,
            &h,
            7,
            g,
            &mut NullSink,
            [&mut Never, &mut Never],
        );
        held += n
            .bat
            .iter()
            .flatten()
            .map(|b| u32::from(b.sb) + u32::from(b.cs))
            .sum::<u32>();
        let n = simulate_game_managed(
            &env(),
            &a,
            &h,
            7,
            g,
            &mut NullSink,
            [&mut Always, &mut Always],
        );
        sent += n
            .bat
            .iter()
            .flatten()
            .map(|b| u32::from(b.sb) + u32::from(b.cs))
            .sum::<u32>();
    }
    assert_eq!(held, 0);
    assert!(sent > 1000, "always going should steal a lot: {sent}");
}

/// Brings in the last pitcher at the first chance, then keeps him.
struct EarlyCloser {
    seen: Vec<GameView>,
}
impl Manager for EarlyCloser {
    fn relief(&mut self, v: &GameView, _: bool) -> ReliefCall {
        if v.pitcher != v.pitchers - 1 && !v.used[usize::from(v.pitchers) - 1] {
            ReliefCall::Bring(v.pitchers - 1)
        } else {
            ReliefCall::Keep
        }
    }
    fn observe(&mut self, v: &GameView) {
        self.seen.push(*v);
    }
}

#[test]
fn managers_choose_relievers_and_see_the_state() {
    let (a, h) = (team("A"), team("H"));
    let mut m = EarlyCloser { seen: vec![] };
    let n = simulate_game_managed(&env(), &a, &h, 3, 1, &mut NullSink, [&mut Baseline, &mut m]);
    // The home closer pitched; the starter was out at once.
    assert!(n.pitch[1][5].used);
    assert!(!m.seen.is_empty());
    assert!(m.seen.iter().all(|v| v.outs < 3 && v.inning >= 1));
    // Pitcher index 1..4 never entered.
    assert!((1..5).all(|i| !n.pitch[1][i].used));
}
