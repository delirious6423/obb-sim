//! Pure simulation core: no I/O, no clock, no global state.

mod game;
mod rng;

pub use game::{GameResult, play_game};
pub use rng::GameRng;
