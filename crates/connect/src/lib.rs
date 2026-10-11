//! Agent connectors: one typed observation and one typed action set for each
//! of the four roles, so a built-in policy, a script or an outside program can
//! fill any seat.
//!
//! Observations carry only what the role may see (a general manager gets
//! noisy scouting reports, never true talent). Everything crosses an external
//! seat as JSON, so a seat can be filled by anything that reads and writes it.

mod agent;
mod builtin;
mod driver;
mod live;
mod model;
mod remote;

pub use agent::{CommissionerAgent, FieldManagerAgent, GmAgent, LiveAgent, OwnerAgent};
pub use builtin::{BuiltInCommissioner, BuiltInGm, BuiltInLive, BuiltInManager, BuiltInOwner};
pub use driver::{
    Seats, Violation, commissioner_step, draft_turn, exec, free_agency_round, owner_step, refuse,
    roster_stage, rules_step, run_year_with, set_tactics, start_season, start_with, team_actor,
    trade_round,
};
pub use live::{LiveManager, play_exhibition};
pub use model::{
    CommissionerAction, CommissionerObs, FieldManagerObs, GameObs, GmAction, GmStage, GmTurn,
    OwnerAction, ReliefAction, StealAction,
};
pub use remote::{ExternalAgent, ReferenceTransport, Request, Response, Transport};
