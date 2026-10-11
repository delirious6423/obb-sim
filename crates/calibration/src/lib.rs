//! Calibration of the game engine against league-wide rates measured from
//! Retrosheet 2015-2019 (see `tools/build_params.py`).
//!
//! This crate does file I/O, so it lives outside `sim-core`. It never stores a
//! real player: rosters are generated, and only aggregates are read.

mod data;
mod roster;
mod run;

pub use data::{Calibration, DataError, Eight, Talent};
pub use roster::{BULLPEN, ROTATION, Specimen, TeamSpec, generate, teams_by_starter};
pub use run::{Report, TEAMS, Totals, simulate};
