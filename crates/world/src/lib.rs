//! The living league: players who are born, develop, age and retire; minor
//! leagues; the draft; free agency; club finances.
//!
//! Every change goes through [`World::execute`] as a command that produces
//! events; events are the only thing that mutates state.

pub mod agents;
mod apply;
pub mod command;
mod decide;
pub mod driver;
mod play;
pub mod player;
pub mod rating;
pub mod scout;
pub mod state;
pub mod util;
