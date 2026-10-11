use serde::{Deserialize, Serialize};
use sim_core::{GameView, ReliefCall, StealCall};
use world::agents::GmObs;
use world::command::Command;
use world::player::{Level, PlayerId};
use world::state::{Phase, Stance, Tactics, World};

/// What the commissioner sees: where the calendar stands.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CommissionerObs {
    pub year: i32,
    pub phase: Phase,
    /// The draft is open and every pick has been made.
    pub draft_finished: bool,
}

impl CommissionerObs {
    #[must_use]
    pub fn new(w: &World) -> Self {
        Self {
            year: w.year,
            phase: w.phase,
            draft_finished: w.draft.as_ref().is_some_and(|d| d.next >= d.order.len()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum CommissionerAction {
    Found { burn_in_years: u8 },
    PlaySeason,
    PlayPlayoffs,
    SettleFinances,
    ProgressPlayers,
    OpenDraft,
    CloseDraft,
    StartSeason,
}

impl CommissionerAction {
    #[must_use]
    pub fn command(self) -> Command {
        match self {
            Self::Found { burn_in_years } => Command::Found { burn_in_years },
            Self::PlaySeason => Command::PlaySeason,
            Self::PlayPlayoffs => Command::PlayPlayoffs,
            Self::SettleFinances => Command::SettleFinances,
            Self::ProgressPlayers => Command::ProgressPlayers,
            Self::OpenDraft => Command::OpenDraft,
            Self::CloseDraft => Command::CloseDraft,
            Self::StartSeason => Command::StartSeason,
        }
    }
}

/// What an owner does in an offseason. Only the payroll budget is required;
/// the rest is left as it is when absent.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct OwnerAction {
    pub payroll: f64,
    /// Average ticket price, dollars.
    #[serde(default)]
    pub ticket: Option<f64>,
    /// Millions to spend on the ballpark.
    #[serde(default)]
    pub build: Option<f64>,
    /// What the general manager is told the club is trying to do.
    #[serde(default)]
    pub stance: Option<Stance>,
}

impl OwnerAction {
    /// The commands for club `team`, in the order they are tried.
    #[must_use]
    pub fn commands(self, team: u8) -> Vec<Command> {
        let mut v = vec![Command::SetBudget {
            team,
            payroll: self.payroll,
        }];
        if let Some(price) = self.ticket {
            v.push(Command::SetTicketPrice { team, price });
        }
        if let Some(amount) = self.build {
            v.push(Command::BuildStadium { team, amount });
        }
        if let Some(stance) = self.stance {
            v.push(Command::SetStance { team, stance });
        }
        v
    }
}

/// When in the offseason a general manager is being asked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GmStage {
    /// A pick is on the clock: answer with one `draft` or `pass`.
    Draft,
    /// Free agency: answer with at most one signing; asked again after each.
    FreeAgency,
    /// Free agency is over: set levels and release, in any number of actions.
    Roster,
    /// Trading is open: propose trades and answer the offers put to you.
    /// `obs.trade` is filled in.
    Trade,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GmTurn {
    pub stage: GmStage,
    pub obs: GmObs,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum GmAction {
    Draft {
        player: PlayerId,
    },
    Pass,
    Sign {
        player: PlayerId,
        salary: f64,
        years: u8,
    },
    Release {
        player: PlayerId,
    },
    SetLevel {
        player: PlayerId,
        level: Level,
    },
    ProposeTrade {
        partner: u8,
        give: Vec<PlayerId>,
        get: Vec<PlayerId>,
    },
    RespondTrade {
        offer: u32,
        accept: bool,
    },
}

impl GmAction {
    #[must_use]
    pub fn command(self, team: u8) -> Command {
        match self {
            Self::Draft { player } => Command::DraftPlayer { team, player },
            Self::Pass => Command::PassPick { team },
            Self::Sign {
                player,
                salary,
                years,
            } => Command::SignFreeAgent {
                team,
                player,
                salary,
                years,
            },
            Self::Release { player } => Command::ReleasePlayer { team, player },
            Self::SetLevel { player, level } => Command::SetLevel {
                team,
                player,
                level,
            },
            Self::ProposeTrade { partner, give, get } => Command::ProposeTrade {
                team,
                partner,
                give,
                get,
            },
            Self::RespondTrade { offer, accept } => Command::RespondTrade {
                team,
                offer,
                accept,
            },
        }
    }

    /// The action behind a general-manager command, if it is one.
    #[must_use]
    pub fn from_command(c: &Command) -> Option<Self> {
        Some(match c.clone() {
            Command::DraftPlayer { player, .. } => Self::Draft { player },
            Command::PassPick { .. } => Self::Pass,
            Command::SignFreeAgent {
                player,
                salary,
                years,
                ..
            } => Self::Sign {
                player,
                salary,
                years,
            },
            Command::ReleasePlayer { player, .. } => Self::Release { player },
            Command::SetLevel { player, level, .. } => Self::SetLevel { player, level },
            _ => return None,
        })
    }
}

/// What a field manager sees between games.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FieldManagerObs {
    pub team: u8,
    pub year: i32,
    pub tactics: Tactics,
}

/// The state of a game at a decision point; no hidden talent, no dice.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameObs {
    pub inning: u8,
    /// 0 = away (top), 1 = home (bottom).
    pub bat: u8,
    pub outs: u8,
    pub bases: [bool; 3],
    pub runs: [u16; 2],
    pub pitcher: u8,
    pub pitchers: u8,
    pub pitches_in_outing: u16,
    pub runs_in_outing: u8,
    pub used: Vec<bool>,
}

impl GameObs {
    #[must_use]
    pub fn from_view(v: &GameView) -> Self {
        Self {
            inning: v.inning,
            bat: v.bat,
            outs: v.outs,
            bases: v.bases,
            runs: v.runs,
            pitcher: v.pitcher,
            pitchers: v.pitchers,
            pitches_in_outing: v.pitches_in_outing,
            runs_in_outing: v.runs_in_outing,
            used: v.used.to_vec(),
        }
    }

    /// Back to the engine's view (for table-driven policies).
    #[must_use]
    pub fn to_view(&self) -> GameView {
        let mut used = [false; sim_core::MAX_PITCHERS];
        for (u, s) in used.iter_mut().zip(&self.used) {
            *u = *s;
        }
        GameView {
            inning: self.inning,
            bat: self.bat,
            outs: self.outs,
            bases: self.bases,
            runs: self.runs,
            pitcher: self.pitcher,
            pitchers: self.pitchers,
            pitches_in_outing: self.pitches_in_outing,
            runs_in_outing: self.runs_in_outing,
            used,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StealAction {
    Default,
    Go,
    Hold,
}

impl From<StealAction> for StealCall {
    fn from(a: StealAction) -> Self {
        match a {
            StealAction::Default => Self::Default,
            StealAction::Go => Self::Go,
            StealAction::Hold => Self::Hold,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum ReliefAction {
    Default,
    Keep,
    Bring { pitcher: u8 },
}

impl From<ReliefAction> for ReliefCall {
    fn from(a: ReliefAction) -> Self {
        match a {
            ReliefAction::Default => Self::Default,
            ReliefAction::Keep => Self::Keep,
            ReliefAction::Bring { pitcher } => Self::Bring(pitcher),
        }
    }
}
