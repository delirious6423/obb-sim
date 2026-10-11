use sim_core::{
    Env, GameOutcome, GameView, Manager, NullSink, ReliefCall, StealCall, Team,
    simulate_game_managed,
};

use crate::agent::LiveAgent;
use crate::model::GameObs;

/// Lets a [`LiveAgent`] make the engine's decisions.
pub struct LiveManager<'a>(pub &'a mut dyn LiveAgent);

impl Manager for LiveManager<'_> {
    fn steal(&mut self, view: &GameView, from: u8) -> StealCall {
        self.0.steal(&GameObs::from_view(view), from).into()
    }

    fn relief(&mut self, view: &GameView, half_start: bool) -> ReliefCall {
        self.0.relief(&GameObs::from_view(view), half_start).into()
    }
}

/// Play one game with live agents in the dugouts.
#[must_use]
pub fn play_exhibition(
    env: &Env,
    away: (&Team, &mut dyn LiveAgent),
    home: (&Team, &mut dyn LiveAgent),
    seed: u64,
    game_id: u64,
) -> GameOutcome {
    let (mut ma, mut mh) = (LiveManager(away.1), LiveManager(home.1));
    simulate_game_managed(
        env,
        away.0,
        home.0,
        seed,
        game_id,
        &mut NullSink,
        [&mut ma, &mut mh],
    )
}
