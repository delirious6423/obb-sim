//! Field-manager intelligence: run-expectancy and win-probability tables
//! measured from the simulation itself, and a manager that decides with them.
//!
//! The tables are counts over simulated games, so they are exact for whatever
//! environment they were built from and need no outside data.

mod manager;
mod tables;

pub use manager::{DecisionLog, DecisionStats, TableManagerMaker};
pub use tables::Tables;
