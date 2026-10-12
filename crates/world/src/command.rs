//! Commands, events and who may issue what.

use protocol::{Actor, Role};
use season::{BatTotals, PitchTotals};
use season::{Record, Series};
use thiserror::Error;

use crate::player::{Contract, Level, Player, PlayerId};
use crate::state::{ClubState, Finance, LeagueLine, Phase, Rules, Stance, Tactics, Usage};

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Command {
    // Commissioner
    Found {
        burn_in_years: u8,
    },
    PlaySeason,
    /// Play the next `days` days of the regular season.
    PlayDays {
        days: u16,
    },
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
    /// Average ticket price, dollars.
    SetTicketPrice {
        team: u8,
        price: f64,
    },
    /// Spend `amount` (millions) on the ballpark: more seats and better quality.
    BuildStadium {
        team: u8,
        amount: f64,
    },
    /// Tell the general manager what the club is trying to do.
    SetStance {
        team: u8,
        stance: Stance,
    },
    // Commissioner, offseason
    SetRules {
        rules: Rules,
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
    ProposeTrade {
        team: u8,
        partner: u8,
        give: Vec<PlayerId>,
        get: Vec<PlayerId>,
    },
    RespondTrade {
        team: u8,
        offer: u32,
        accept: bool,
    },
    // Field manager
    SetTactics {
        team: u8,
        tactics: Tactics,
    },
    /// The club's playing-time setup (platoons, bullpen, pitch limits and
    /// relief rules); `None` goes back to the league's customs.
    SetUsage {
        team: u8,
        usage: Option<Usage>,
    },
}

impl Command {
    /// The role allowed to issue the command.
    #[must_use]
    pub fn required_role(&self) -> Role {
        match self {
            Command::Found { .. }
            | Command::PlaySeason
            | Command::PlayDays { .. }
            | Command::PlayPlayoffs
            | Command::SettleFinances
            | Command::ProgressPlayers
            | Command::OpenDraft
            | Command::CloseDraft
            | Command::StartSeason
            | Command::SetRules { .. } => Role::Commissioner,
            Command::SetBudget { .. }
            | Command::SetTicketPrice { .. }
            | Command::BuildStadium { .. }
            | Command::SetStance { .. } => Role::Owner,
            Command::DraftPlayer { .. }
            | Command::PassPick { .. }
            | Command::SignFreeAgent { .. }
            | Command::ReleasePlayer { .. }
            | Command::SetLevel { .. }
            | Command::ProposeTrade { .. }
            | Command::RespondTrade { .. } => Role::Gm,
            Command::SetTactics { .. } | Command::SetUsage { .. } => Role::FieldManager,
        }
    }

    /// The club the command acts for, if it names one.
    #[must_use]
    pub fn team(&self) -> Option<u8> {
        match self {
            Command::SetBudget { team, .. }
            | Command::SetTicketPrice { team, .. }
            | Command::BuildStadium { team, .. }
            | Command::SetStance { team, .. }
            | Command::DraftPlayer { team, .. }
            | Command::PassPick { team }
            | Command::SignFreeAgent { team, .. }
            | Command::ReleasePlayer { team, .. }
            | Command::SetLevel { team, .. }
            | Command::ProposeTrade { team, .. }
            | Command::RespondTrade { team, .. }
            | Command::SetTactics { team, .. }
            | Command::SetUsage { team, .. } => Some(*team),
            _ => None,
        }
    }
}

/// The score of one game.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct GameResult {
    /// Game number within the season's schedule.
    pub number: u16,
    pub day: u16,
    pub away: u8,
    pub home: u8,
    pub away_runs: u16,
    pub home_runs: u16,
}

/// Time passing and injuries at the end of a stretch of the season.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SegmentEnd {
    /// Games each hurt player still has to miss.
    pub out: std::collections::BTreeMap<PlayerId, u16>,
    /// Permanent damage dealt this segment.
    pub loss: Vec<(PlayerId, f64)>,
    pub hurt: Vec<PlayerId>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Founding {
    pub year: i32,
    pub rules: Rules,
    pub clubs: Vec<ClubState>,
    pub players: Vec<Player>,
    pub free_agents: Vec<PlayerId>,
    pub next_player: u32,
    /// The era shift the burn-in left behind (see `World::era`). Players'
    /// talent already includes it, so the league must start from it too.
    /// Absent in old saves: zero, as it was then.
    #[serde(default)]
    pub era: [crate::rating::Z; 2],
}

/// What a player did in a season, with his value above a replacement player.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Line {
    Bat(BatTotals),
    Pitch(PitchTotals),
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SeasonLine {
    pub player: PlayerId,
    pub line: Line,
    pub war: f64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Injury {
    pub player: PlayerId,
    /// Games still to miss when the season ended.
    pub out_games: u16,
    /// Permanent damage, in quality standard deviations.
    pub loss: f64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Update {
    pub player: PlayerId,
    pub drift: f64,
    pub z: crate::rating::Z,
    pub quality: f64,
    pub contract: Option<Contract>,
    pub out_games: u16,
    pub outcome: Outcome,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Outcome {
    Stays,
    /// Contract ran out: becomes a free agent.
    Expired,
    Retired,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Event {
    Founded(Box<Founding>),
    /// Some days of the regular season, played: the games, what the players
    /// did in them (as additions), and, at a segment's end, who got hurt.
    DaysPlayed {
        year: i32,
        /// Days played so far after this event.
        to_day: u16,
        results: Vec<GameResult>,
        lines: Vec<(PlayerId, Line)>,
        segment_end: Option<SegmentEnd>,
    },
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
    TicketSet {
        team: u8,
        price: f64,
    },
    StadiumBuilt {
        team: u8,
        amount: f64,
    },
    StanceSet {
        team: u8,
        stance: Stance,
    },
    RulesSet {
        rules: Rules,
    },
    /// Before opening day a club below its credit line gets new owners:
    /// fresh capital, ticket price and stance reset, payroll budget at the floor.
    ClubRescued {
        team: u8,
    },
    PlayersProgressed {
        year: i32,
        updates: Vec<Update>,
        /// The era shift after this progression.
        era: [crate::rating::Z; 2],
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
    TradeProposed {
        offer: u32,
        from: u8,
        to: u8,
        give: Vec<PlayerId>,
        get: Vec<PlayerId>,
    },
    /// The club it was put to answered; an accepted offer moves the players.
    TradeAnswered {
        offer: u32,
        accepted: bool,
    },
    TacticsSet {
        team: u8,
        tactics: Tactics,
    },
    UsageSet {
        team: u8,
        usage: Option<Usage>,
    },
}

/// Club money after settlement.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
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
