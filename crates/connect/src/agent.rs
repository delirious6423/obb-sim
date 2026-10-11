use world::agents::{LeagueObs, OwnerObs};
use world::state::{Rules, Tactics};

use crate::model::{
    CommissionerAction, CommissionerObs, FieldManagerObs, GameObs, GmAction, GmTurn, OwnerAction,
    ReliefAction, StealAction,
};

pub trait CommissionerAgent {
    fn act(&mut self, obs: &CommissionerObs) -> CommissionerAction;

    /// New league rules for the coming season, asked once each offseason
    /// before players progress. `None` leaves the rules as they are.
    fn rules(&mut self, _obs: &LeagueObs) -> Option<Rules> {
        None
    }
}

pub trait OwnerAgent {
    /// The payroll budget for the coming season.
    fn act(&mut self, obs: &OwnerObs) -> OwnerAction;
}

pub trait GmAgent {
    fn act(&mut self, turn: &GmTurn) -> Vec<GmAction>;

    /// Whether to be asked at the trade stage (it costs a look at the whole
    /// league's market, so agents that never trade say no).
    fn trades(&self) -> bool {
        false
    }
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
