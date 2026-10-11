//! Pure simulation core: no I/O, no clock, no global state.
//!
//! - [`play_game`] is the tiny placeholder game the league crate uses for now.
//! - [`simulate_game`] is the real engine: log5 plate appearances, baserunning,
//!   fielding, relief pitching. Parameters and players are passed in as data.

mod engine;
mod events;
mod game;
mod params;
mod rates;
mod render;
mod rng;
mod stats;
mod team;

pub use engine::simulate_game;
pub use events::{Ball, GameEvent, Half, NullSink, PlayLog, PlayRecord, Sink};
pub use game::{GameResult, play_game};
pub use params::{Baserunning, Env, Fielding, GameParams, ParamsError};
pub use rates::{Log5, OUTCOMES, Outcome, Rates, RatesError, pick};
pub use render::{box_score, play_by_play};
pub use rng::GameRng;
pub use stats::{BatLine, GameOutcome, MAX_INNINGS, PitchLine};
pub use team::{LINEUP, MAX_PITCHERS, Player, Team, TeamError};
