//! Commands, events and who may issue what.

use protocol::{Actor, Role};
use season::{BatTotals, PitchTotals};
use season::{Record, Series};
use thiserror::Error;

use crate::player::{Contract, Level, Player, PlayerId};
use crate::state::{ClubState, Finance, LeagueLine, Phase, Rules};

#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    // Commissioner
    Found {
        burn_in_years: u8,
    },
    PlaySeason,
    PlayPlayoffs,
    SettleFinances,
    ProgressPlayers,
    OpenDraft,
    CloseDraft,
    StartSeason,
    // Owner
    SetBudget {
        team: u8,
        payroll: f64,
    },
    // General manager
    DraftPlayer {
        team: u8,
        player: PlayerId,
    },
    PassPick {
        team: u8,
    },
    SignFreeAgent {
        team: u8,
        player: PlayerId,
        salary: f64,
        years: u8,
    },
    ReleasePlayer {
        team: u8,
        player: PlayerId,
    },
    SetLevel {
        team: u8,
        player: PlayerId,
        level: Level,
    },
}

impl Command {
    /// The role allowed to issue the command.
    #[must_use]
    pub fn required_role(&self) -> Role {
        match self {
            Command::Found { .. }
            | Command::PlaySeason
            | Command::PlayPlayoffs
            | Command::SettleFinances
            | Command::ProgressPlayers
            | Command::OpenDraft
            | Command::CloseDraft
            | Command::StartSeason => Role::Commissioner,
            Command::SetBudget { .. } => Role::Owner,
            Command::DraftPlayer { .. }
            | Command::PassPick { .. }
            | Command::SignFreeAgent { .. }
            | Command::ReleasePlayer { .. }
            | Command::SetLevel { .. } => Role::Gm,
        }
    }

    /// The club the command acts for, if it names one.
    #[must_use]
    pub fn team(&self) -> Option<u8> {
        match self {
            Command::SetBudget { team, .. }
            | Command::DraftPlayer { team, .. }
            | Command::PassPick { team }
            | Command::SignFreeAgent { team, .. }
            | Command::ReleasePlayer { team, .. }
            | Command::SetLevel { team, .. } => Some(*team),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Founding {
    pub year: i32,
    pub rules: Rules,
    pub clubs: Vec<ClubState>,
    pub players: Vec<Player>,
    pub free_agents: Vec<PlayerId>,
    pub next_player: u32,
}

/// What a player did in a season, with his value above a replacement player.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Line {
    Bat(BatTotals),
    Pitch(PitchTotals),
}

#[derive(Debug, Clone, PartialEq)]
pub struct SeasonLine {
    pub player: PlayerId,
    pub line: Line,
    pub war: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Injury {
    pub player: PlayerId,
    /// Games still to miss when the season ended.
    pub out_games: u16,
    /// Permanent damage, in quality standard deviations.
    pub loss: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Update {
    pub player: PlayerId,
    pub drift: f64,
    pub z: crate::rating::Z,
    pub quality: f64,
    pub contract: Option<Contract>,
    pub out_games: u16,
    pub outcome: Outcome,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Stays,
    /// Contract ran out: becomes a free agent.
    Expired,
    Retired,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    Founded(Box<Founding>),
    SeasonPlayed {
        year: i32,
        records: Vec<Record>,
        lines: Vec<SeasonLine>,
        injuries: Vec<Injury>,
        league: LeagueLine,
    },
    PlayoffsPlayed {
        year: i32,
        series: Vec<Series>,
        champion: u8,
    },
    FinancesSettled {
        year: i32,
        clubs: Vec<(Finance, ClubFinanceState)>,
    },
    BudgetSet {
        team: u8,
        payroll: f64,
    },
    PlayersProgressed {
        year: i32,
        updates: Vec<Update>,
    },
    DraftOpened {
        year: i32,
        prospects: Vec<Player>,
        order: Vec<u8>,
        amateur_picks: usize,
        next_player: u32,
    },
    PlayerDrafted {
        pick: usize,
        team: u8,
        player: PlayerId,
        /// Signing bonus paid; `None` when the player refused to sign.
        bonus: Option<f64>,
    },
    PickPassed {
        pick: usize,
        team: u8,
    },
    DraftClosed,
    FreeAgentSigned {
        team: u8,
        player: PlayerId,
        contract: Contract,
    },
    PlayerReleased {
        team: u8,
        player: PlayerId,
    },
    LevelSet {
        team: u8,
        player: PlayerId,
        level: Level,
    },
    SeasonStarted {
        year: i32,
    },
}

/// Club money after settlement.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ClubFinanceState {
    pub cash: f64,
    pub revenue_prev: f64,
    pub hype: f64,
    pub win_pct_prev: f64,
}

#[derive(Debug, Error, PartialEq)]
pub enum WorldError {
    #[error("{actor:?} may not issue this command (needs {needed:?})")]
    Permission { actor: Actor, needed: Role },
    #[error("a {needed:?} acts for a club; this actor has none or a different one")]
    WrongTeam { needed: Role },
    #[error("command not allowed in phase {0:?}")]
    Phase(Phase),
    #[error("unknown club {0}")]
    UnknownClub(u8),
    #[error("unknown player {0:?}")]
    UnknownPlayer(PlayerId),
    #[error("{0}")]
    Invalid(String),
}

/// Check the actor may issue the command (role, and club for club roles).
pub fn authorize(actor: &Actor, command: &Command) -> Result<(), WorldError> {
    let needed = command.required_role();
    if actor.role != needed {
        return Err(WorldError::Permission {
            actor: *actor,
            needed,
        });
    }
    if let Some(team) = command.team()
        && actor.team.map(|t| t.0) != Some(u32::from(team))
    {
        return Err(WorldError::WrongTeam { needed });
    }
    Ok(())
}
