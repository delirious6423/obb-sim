//! The protocol between a front end and the league host.
//!
//! Everything here is plain serialisable data: a front end sends a [`Request`]
//! and gets a [`Response`]. The same types will cross the network in online
//! play. The views are role-filtered: a front end sees the league as the
//! general manager of its club may know it (scouted grades, public stats and
//! prices, its own books), never the hidden true talent.

use serde::{Deserialize, Serialize};

/// A club, by index (0..30).
pub type Club = u8;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NewGame {
    pub seed: u64,
    /// The club the person will run as general manager.
    pub club: Club,
    pub difficulty: Difficulty,
    /// Seasons played out before the first one is shown, so the league has
    /// a past (no games, only offseasons).
    pub burn_in_years: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Preset {
    Rookie,
    Veteran,
    AllStar,
    HallOfFame,
}

/// What the opposing general managers know and how well they decide. Never
/// the dice.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Difficulty {
    Preset(Preset),
    Custom(Levers),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TradeLevel {
    Off,
    Respond,
    Active,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Levers {
    /// Extra scouting error (0 = trusts the reports as they are).
    pub extra_noise: f64,
    /// Weight on the market's view read from prices (0..1).
    pub market_weight: f64,
    /// Seasons of a contract looked at when valuing a player.
    pub horizon: u8,
    pub projects_aging: bool,
    pub picks_years: bool,
    pub trades: TradeLevel,
    pub spends_budget: bool,
    /// Taste, not skill: weight on ceiling over current level (0..1).
    pub youth: f64,
    pub thrift: f64,
    pub trade_margin: f64,
}

impl Difficulty {
    #[must_use]
    pub fn label(&self) -> String {
        match self {
            Difficulty::Preset(p) => format!("{p:?}"),
            Difficulty::Custom(_) => "Custom".into(),
        }
    }
}

/// Where the league stands in its yearly cycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Stage {
    /// The regular season is on (or about to start).
    Season,
    /// The regular season is over; the playoffs are next.
    Playoffs,
    /// Playoffs done: finances, then players age, then the draft.
    Offseason,
    /// The amateur draft is on.
    Draft,
    /// Trades and free agency, then the next season starts.
    FreeAgency,
}

/// What a front end can ask the league to do next.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Advance {
    /// Play this many days of the regular season.
    SimDays(u16),
    /// Play out the regular season.
    SimToEndOfSeason,
    /// Playoffs, finances, ageing, then open the draft.
    FinishSeason,
    /// The other clubs pick until the person is on the clock (or all done,
    /// which closes the draft).
    SimDraft,
    /// The other clubs hold one round of trade talks.
    TradeRound,
    /// The other clubs sign one free agent each.
    FreeAgencyRound,
    /// The other clubs finish trades and free agency, rosters are set, and
    /// the next season starts.
    StartNextSeason,
}

/// One thing the person's own club does. The league checks all of them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Action {
    SetLevel {
        player: u32,
        level: Level,
    },
    Release {
        player: u32,
    },
    Sign {
        player: u32,
        salary: f64,
        years: u8,
    },
    Draft {
        player: u32,
    },
    PassPick,
    ProposeTrade {
        partner: Club,
        give: Vec<u32>,
        get: Vec<u32>,
    },
    RespondTrade {
        offer: u32,
        accept: bool,
    },
    SetTactics {
        smart: bool,
    },
    /// Put the roster in order the way the club's staff would.
    AutoRoster,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Level {
    Majors,
    Triple,
    Double,
    Single,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Position {
    Batter,
    Pitcher,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Request {
    NewGame(NewGame),
    Save {
        slot: String,
    },
    QuickSave,
    Load {
        slot: String,
    },
    ListSaves,
    DeleteSave {
        slot: String,
    },
    Overview,
    Standings,
    /// A club's schedule and results this season.
    Schedule {
        club: Club,
    },
    BoxScore {
        game: u16,
    },
    Roster {
        club: Club,
    },
    Player {
        id: u32,
    },
    FreeAgents,
    DraftBoard,
    Finances,
    Trades,
    Advance(Advance),
    Act(Action),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Response {
    /// Done; the text says what happened (may be empty).
    Done(String),
    /// Refused or failed; the world is unchanged.
    Failed(String),
    Overview(Overview),
    Standings(Standings),
    Schedule(ScheduleView),
    BoxScore(Option<String>),
    Roster(RosterView),
    Player(PlayerView),
    Players(Vec<PlayerRow>),
    Draft(DraftView),
    Finances(FinancesView),
    Trades(TradeView),
    Saves(Vec<SaveInfo>),
}

/// How far a long request has got.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Progress {
    pub running: bool,
    pub label: String,
    pub done: u32,
    pub total: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Overview {
    pub league: String,
    pub year: i32,
    pub stage: Stage,
    pub club: Club,
    pub club_name: String,
    pub day: u16,
    pub days: u16,
    pub record: (u16, u16),
    pub place: String,
    /// Things the person could do now, most useful first.
    pub advance: Vec<(Advance, String)>,
    pub on_the_clock: bool,
    pub open_offers: usize,
    pub cash: f64,
    pub payroll: f64,
    pub budget: f64,
    pub difficulty: String,
    /// Messages since the last overview (roster fixes, deals, notices).
    pub notices: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StandingRow {
    pub club: Club,
    pub name: String,
    pub w: u16,
    pub l: u16,
    pub rs: u32,
    pub ra: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Division {
    pub name: String,
    pub rows: Vec<StandingRow>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Standings {
    pub year: i32,
    pub divisions: Vec<Division>,
    pub champion: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GameRow {
    pub game: u16,
    pub day: u16,
    pub home: bool,
    pub opponent: String,
    /// Runs for and against, once played.
    pub result: Option<(u16, u16)>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScheduleView {
    pub club: Club,
    pub club_name: String,
    pub year: i32,
    pub games: Vec<GameRow>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlayerRow {
    pub id: u32,
    pub name: String,
    pub position: Position,
    pub age: i32,
    pub level: Level,
    pub origin: String,
    /// Scouting grade now and at his peak, 20-80, as the person's club sees
    /// him (noisy).
    pub grade_now: u8,
    pub grade_peak: u8,
    pub salary: f64,
    pub years: u8,
    /// What the market asks (free agents) or the amateur demands.
    pub ask: f64,
    pub ask_years: u8,
    pub demand: f64,
    /// Games he still has to miss.
    pub out_games: u16,
    pub club: Option<Club>,
    /// This season so far, as text.
    pub stat: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RosterView {
    pub club: Club,
    pub club_name: String,
    pub own: bool,
    pub players: Vec<PlayerRow>,
    pub payroll: f64,
    pub budget: f64,
    /// Roster problems that would stop the season starting (own club only).
    pub problems: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlayerView {
    pub row: PlayerRow,
    pub club_name: Option<String>,
    pub drafted: Option<(i32, u16)>,
    pub career_seasons: u16,
    pub career_war: f64,
    pub history: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DraftView {
    pub open: bool,
    pub year: i32,
    /// The person's club is on the clock.
    pub on_the_clock: bool,
    pub pick: usize,
    pub picks: usize,
    pub international: bool,
    /// Largest bonus the pick on the clock will sign for.
    pub ceiling: f64,
    /// Who is on the clock.
    pub clock: Option<String>,
    pub prospects: Vec<PlayerRow>,
    /// The picks made so far: pick number, club, player.
    pub made: Vec<(usize, String, String)>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FinanceRow {
    pub year: i32,
    pub revenue: f64,
    pub payroll: f64,
    pub other: f64,
    pub spent: f64,
    pub dividend: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FinancesView {
    pub club_name: String,
    pub cash: f64,
    pub budget: f64,
    pub payroll: f64,
    pub payroll_floor: f64,
    pub payroll_cap: f64,
    pub revenue_prev: f64,
    pub history: Vec<FinanceRow>,
    /// Every contract on the books.
    pub contracts: Vec<PlayerRow>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OfferView {
    pub id: u32,
    pub from: Club,
    pub from_name: String,
    /// What the other club would send here.
    pub give: Vec<PlayerRow>,
    /// What it asks for.
    pub get: Vec<PlayerRow>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TradeView {
    /// Whether trading is open (the offseason window).
    pub open: bool,
    pub incoming: Vec<OfferView>,
    /// Offers the person's club has out.
    pub outgoing: Vec<OfferView>,
    /// Players on other clubs, as the person's club sees them.
    pub market: Vec<PlayerRow>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SaveInfo {
    pub slot: String,
    pub league: String,
    pub club_name: String,
    pub role: String,
    pub date: String,
    pub difficulty: String,
    pub saved_at_unix: u64,
    pub kind: String,
    /// Set when the save cannot be loaded as it is.
    pub problem: Option<String>,
}
