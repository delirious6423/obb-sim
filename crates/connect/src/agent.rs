use world::agents::OwnerObs;
use world::state::Tactics;

use crate::model::{
    CommissionerAction, CommissionerObs, FieldManagerObs, GameObs, GmAction, GmTurn, OwnerAction,
    ReliefAction, StealAction,
};

pub trait CommissionerAgent {
    fn act(&mut self, obs: &CommissionerObs) -> CommissionerAction;
}

pub trait OwnerAgent {
    /// The payroll budget for the coming season.
    fn act(&mut self, obs: &OwnerObs) -> OwnerAction;
}

pub trait GmAgent {
    fn act(&mut self, turn: &GmTurn) -> Vec<GmAction>;
}

pub trait FieldManagerAgent {
    /// How the club's games are managed this season.
    fn tactics(&mut self, obs: &FieldManagerObs) -> Tactics;
}

/// A field manager deciding inside games, one decision at a time.
pub trait LiveAgent {
    fn steal(&mut self, obs: &GameObs, from: u8) -> StealAction;
    fn relief(&mut self, obs: &GameObs, half_start: bool) -> ReliefAction;
}
