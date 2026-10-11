//! World state. Only [`crate::apply`] changes it.

use std::collections::{BTreeMap, BTreeSet};

use season::{Record, Series};
use sim_core::Env;

use crate::player::{Player, PlayerId, Pool};
use crate::rating::Model;

pub const TEAMS: usize = 30;
/// Batters and pitchers each club carries at the major-league level.
pub const MAJORS_PER_KIND: usize = 13;
pub const MIN_SALARY: f64 = 0.75;
pub const MINOR_COST: f64 = 0.06;
/// Players of one kind a club may hold when the season starts, all levels.
pub const ROSTER_PER_KIND: usize = 49;
/// Hard limit of one kind on a club at any time (the offseason runs over
/// the season limit until the GM sets levels).
pub const ROSTER_MAX_PER_KIND: usize = 60;
/// How far below zero a club's cash may be pushed by discretionary spending
/// (bonuses, paying off released contracts).
pub const CREDIT: f64 = 25.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Unfounded,
    /// Rosters set; the season can be played.
    Regular,
    /// Regular season done; playoffs next.
    Playoffs,
    /// Playoffs done; finances next.
    Settling,
    /// Finances settled; players age next.
    Offseason,
    /// Players aged; the draft can open.
    Progressed,
    /// Draft in progress.
    Drafting,
    /// Draft done: free agency, then the next season starts.
    FreeAgency,
}

/// League-wide rules set by the commissioner.
#[derive(Debug, Clone, PartialEq)]
pub struct Rules {
    pub payroll_floor: f64,
    pub payroll_cap: f64,
    pub revenue_sharing: f64,
    /// Millions paid per win above replacement on the open market.
    pub war_price: f64,
    pub draft_rounds: u16,
    pub international_rounds: u16,
}

impl Default for Rules {
    fn default() -> Self {
        Self {
            payroll_floor: 70.0,
            payroll_cap: 240.0,
            revenue_sharing: 0.12,
            war_price: 4.4,
            draft_rounds: 10,
            international_rounds: 5,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ClubState {
    pub name: String,
    /// Market size, 1.0 is average.
    pub market: f64,
    pub cash: f64,
    pub budget: f64,
    pub revenue_prev: f64,
    /// Cash already paid since the last settlement for signing bonuses and
    /// released contracts; shown in the next season's books.
    pub spent: f64,
    pub hype: f64,
    pub win_pct_prev: f64,
    pub players: BTreeSet<PlayerId>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Draft {
    pub year: i32,
    /// Club on the clock for each pick (amateur rounds, then international).
    pub order: Vec<u8>,
    pub amateur_picks: usize,
    pub next: usize,
    pub remaining: BTreeSet<PlayerId>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SeasonRecord {
    pub year: i32,
    pub records: Vec<Record>,
    pub champion: Option<u8>,
    pub series: Vec<Series>,
    pub league: LeagueLine,
}

/// League totals for one season (all plate appearances in the majors).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct LeagueLine {
    pub pa: u32,
    pub ab: u32,
    pub h: u32,
    pub so: u32,
    pub bb: u32,
    pub hr: u32,
    pub runs: u32,
    pub games: u32,
    pub er: u32,
    pub outs: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Finance {
    pub revenue: f64,
    pub payroll: f64,
    pub other: f64,
    /// Bonuses and released contracts paid during the offseason before.
    pub spent: f64,
    pub dividend: f64,
}

#[derive(Debug, Clone)]
pub struct World {
    pub seed: u64,
    pub year: i32,
    pub phase: Phase,
    pub rules: Rules,
    pub pool: Pool,
    pub clubs: Vec<ClubState>,
    pub players: BTreeMap<PlayerId, Player>,
    pub free_agents: BTreeSet<PlayerId>,
    pub retired: BTreeMap<PlayerId, (Player, i32)>,
    pub draft: Option<Draft>,
    pub history: Vec<SeasonRecord>,
    pub finances: Vec<Vec<Finance>>,
    pub next_player: u32,
    /// Records of the regular season just played, until finances settle.
    pub last_records: Vec<Record>,
    pub model: Model,
    pub env: Env,
}

impl PartialEq for World {
    fn eq(&self, other: &Self) -> bool {
        // The model and environment come from the same parameters on both
        // sides; everything that events change is compared.
        self.seed == other.seed
            && self.year == other.year
            && self.phase == other.phase
            && self.rules == other.rules
            && self.pool == other.pool
            && self.clubs == other.clubs
            && self.players == other.players
            && self.free_agents == other.free_agents
            && self.retired == other.retired
            && self.draft == other.draft
            && self.history == other.history
            && self.finances == other.finances
            && self.next_player == other.next_player
            && self.last_records == other.last_records
    }
}

impl World {
    #[must_use]
    pub fn player(&self, id: PlayerId) -> Option<&Player> {
        self.players.get(&id)
    }

    /// Salaries of every contract the club holds, at any level.
    #[must_use]
    pub fn payroll(&self, team: usize) -> f64 {
        self.clubs[team]
            .players
            .iter()
            .filter_map(|id| self.players.get(id))
            .filter_map(|p| p.contract.map(|c| c.salary))
            .sum()
    }
}
