use world::agents::{OwnerObs, draft_pick, free_agency_step, owner_budget_for, set_levels};
use world::state::{Phase, Tactics};

use crate::agent::{CommissionerAgent, FieldManagerAgent, GmAgent, LiveAgent, OwnerAgent};
use crate::model::{
    CommissionerAction, CommissionerObs, FieldManagerObs, GameObs, GmAction, GmStage, GmTurn,
    OwnerAction, ReliefAction, StealAction,
};

/// Follows the league calendar.
#[derive(Debug, Clone, Copy)]
pub struct BuiltInCommissioner {
    pub burn_in_years: u8,
}

impl Default for BuiltInCommissioner {
    fn default() -> Self {
        Self { burn_in_years: 30 }
    }
}

impl CommissionerAgent for BuiltInCommissioner {
    fn act(&mut self, obs: &CommissionerObs) -> CommissionerAction {
        match obs.phase {
            Phase::Unfounded => CommissionerAction::Found {
                burn_in_years: self.burn_in_years,
            },
            Phase::Regular => CommissionerAction::PlaySeason,
            Phase::Playoffs => CommissionerAction::PlayPlayoffs,
            Phase::Settling => CommissionerAction::SettleFinances,
            Phase::Offseason => CommissionerAction::ProgressPlayers,
            Phase::Progressed => CommissionerAction::OpenDraft,
            Phase::Drafting => CommissionerAction::CloseDraft,
            Phase::FreeAgency => CommissionerAction::StartSeason,
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct BuiltInOwner;

impl OwnerAgent for BuiltInOwner {
    fn act(&mut self, obs: &OwnerObs) -> OwnerAction {
        OwnerAction {
            payroll: owner_budget_for(obs),
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct BuiltInGm;

impl GmAgent for BuiltInGm {
    fn act(&mut self, turn: &GmTurn) -> Vec<GmAction> {
        let commands = match turn.stage {
            GmStage::Draft => draft_pick(&turn.obs).into_iter().collect(),
            GmStage::FreeAgency => free_agency_step(&turn.obs).into_iter().collect(),
            GmStage::Roster => set_levels(&turn.obs),
        };
        commands.iter().filter_map(GmAction::from_command).collect()
    }
}

/// Keeps one tactics setting for every season.
#[derive(Debug, Clone, Copy, Default)]
pub struct BuiltInManager {
    pub tactics: Tactics,
}

impl FieldManagerAgent for BuiltInManager {
    fn tactics(&mut self, _: &FieldManagerObs) -> Tactics {
        self.tactics
    }
}

/// The engine's own rules, inside a game.
#[derive(Debug, Clone, Copy, Default)]
pub struct BuiltInLive;

impl LiveAgent for BuiltInLive {
    fn steal(&mut self, _: &GameObs, _: u8) -> StealAction {
        StealAction::Default
    }

    fn relief(&mut self, _: &GameObs, _: bool) -> ReliefAction {
        ReliefAction::Default
    }
}
