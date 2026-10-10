//! League state. It changes only by applying events, and events come only from commands.
//!
//! - [`authorize`] says whether an [`protocol::Actor`] may issue a command.
//! - [`League::execute`] authorizes, decides, then applies. It is the only way
//!   commands change state, and it is what [`Session::submit`] and log replay call.
//! - `League::apply` (private) checks one event against the state, then applies it.
//!   Events that do not fit are errors, never trusted.
//! - [`Session`] owns a league and its command log, and keeps them in step.

mod permissions;
mod session;
mod state;

pub use permissions::{PermissionError, authorize};
pub use session::Session;
pub use state::{Game, League, LeagueError, Player, Team};
