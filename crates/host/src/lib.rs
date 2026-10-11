//! The league host: runs a league for one person (the general manager of one
//! club) and speaks the [`api`] protocol. Single player is this host running
//! in-process; online play will run the same host behind a network.
//!
//! A front end depends only on [`api`] and [`client`]. It never sees the
//! simulation state.

pub mod api;
pub mod client;
mod data;
mod difficulty;
mod session;
mod views;

pub use client::{GameClient, LocalClient, Ticket};
pub use data::calibration;
pub use difficulty::levers_of;
pub use session::Host;

/// The names of the 30 clubs a new game can be started with (the same for
/// every seed), for a team-pick screen.
#[must_use]
pub fn club_names() -> Vec<String> {
    (0..30).map(::calibration::team_name).collect()
}
