//! World state. Only [`crate::apply`] changes it.

use std::collections::{BTreeMap, BTreeSet};

use season::{Record, Series};
use std::sync::{Arc, OnceLock};

use sim_core::Env;
use tactics::{DecisionLog, Tables};

use crate::command::{Event, GameResult, Line};
use crate::player::{Player, PlayerId, Pool};
use crate::rating::{Model, Z};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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

/// A regular season partly played.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct InSeason {
    /// Days played.
    pub day: u16,
    /// Games each hurt player still has to miss, as of the start of the
    /// current segment.
    pub out: BTreeMap<PlayerId, u16>,
    /// The same, as of the start of each segment played or begun so far (to
    /// replay a game with the rosters it was played with).
    pub seg_starts: Vec<BTreeMap<PlayerId, u16>>,
    pub records: Vec<Record>,
    /// What each player has done so far.
    pub lines: BTreeMap<PlayerId, Line>,
    pub loss: BTreeMap<PlayerId, f64>,
    pub hurt: BTreeSet<PlayerId>,
    pub results: Vec<GameResult>,
}

impl InSeason {
    /// The start of a season: nobody has played; last year's injuries linger.
    #[must_use]
    pub fn start(w: &World) -> Self {
        let out: BTreeMap<PlayerId, u16> = w
            .players
            .values()
            .filter(|p| p.team.is_some() && p.out_games > 0)
            .map(|p| (p.id, p.out_games))
            .collect();
        Self {
            day: 0,
            seg_starts: vec![out.clone()],
            out,
            records: vec![Record::default(); w.clubs.len()],
            lines: BTreeMap::new(),
            loss: BTreeMap::new(),
            hurt: BTreeSet::new(),
            results: Vec::new(),
        }
    }

    /// Take in a `DaysPlayed` event.
    pub fn absorb(&mut self, e: &Event) {
        let Event::DaysPlayed {
            to_day,
            results,
            lines,
            segment_end,
            ..
        } = e
        else {
            return;
        };
        for r in results {
            let (a, h) = (usize::from(r.away), usize::from(r.home));
            let (ra, rh) = (u32::from(r.away_runs), u32::from(r.home_runs));
            self.records[a].rs += ra;
            self.records[a].ra += rh;
            self.records[h].rs += rh;
            self.records[h].ra += ra;
            if ra > rh {
                self.records[a].w += 1;
                self.records[h].l += 1;
            } else {
                self.records[h].w += 1;
                self.records[a].l += 1;
            }
        }
        self.results.extend(results.iter().copied());
        for (id, line) in lines {
            match (self.lines.get_mut(id), line) {
                (Some(Line::Bat(a)), Line::Bat(b)) => a.merge(b),
                (Some(Line::Pitch(a)), Line::Pitch(b)) => a.merge(b),
                (Some(_), _) => {}
                (None, l) => {
                    self.lines.insert(*id, *l);
                }
            }
        }
        if let Some(s) = segment_end {
            self.out = s.out.clone();
            self.seg_starts.push(s.out.clone());
            for (id, l) in &s.loss {
                *self.loss.entry(*id).or_insert(0.0) += l;
            }
            self.hurt.extend(s.hurt.iter().copied());
        }
        self.day = *to_day;
    }
}

/// League-wide rules set by the commissioner.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Rules {
    pub payroll_floor: f64,
    pub payroll_cap: f64,
    pub revenue_sharing: f64,
    /// Millions paid per win above replacement on the open market.
    pub war_price: f64,
    pub draft_rounds: u16,
    pub international_rounds: u16,
}

impl Rules {
    /// Whether the change from `old` to `self` is gradual enough: the league
    /// gives clubs notice, so limits move by at most a tenth a year (revenue
    /// sharing by 0.04, draft rounds by 2).
    ///
    /// # Errors
    /// The first limit that moves too far.
    pub fn check_change(&self, old: &Rules) -> Result<(), String> {
        let within = |new: f64, old: f64| (new - old).abs() <= 0.10 * old + 1e-9;
        if !within(self.payroll_floor, old.payroll_floor)
            || !within(self.payroll_cap, old.payroll_cap)
            || !within(self.war_price, old.war_price)
        {
            return Err("payroll limits and price per win may move at most 10% a year".into());
        }
        if (self.revenue_sharing - old.revenue_sharing).abs() > 0.04 + 1e-9 {
            return Err("revenue sharing may move at most 0.04 a year".into());
        }
        if self.draft_rounds.abs_diff(old.draft_rounds) > 2
            || self.international_rounds.abs_diff(old.international_rounds) > 2
        {
            return Err("draft rounds may change by at most 2 a year".into());
        }
        Ok(())
    }

    /// Whether the rules are inside the limits a commissioner may set.
    ///
    /// # Errors
    /// The first limit that is broken.
    pub fn check(&self) -> Result<(), String> {
        let ok = |x: f64, lo: f64, hi: f64| x.is_finite() && (lo..=hi).contains(&x);
        if !ok(self.payroll_floor, 40.0, 100.0) {
            return Err("payroll floor must be between 40 and 100".into());
        }
        if !ok(self.payroll_cap, self.payroll_floor + 40.0, 420.0) {
            return Err("payroll cap must be 40 to 420 and 40 above the floor".into());
        }
        if !ok(self.revenue_sharing, 0.0, 0.4) {
            return Err("revenue sharing must be between 0 and 0.4".into());
        }
        if !ok(self.war_price, 2.0, 9.0) {
            return Err("price per win must be between 2 and 9".into());
        }
        if !(1..=30).contains(&self.draft_rounds) || self.international_rounds > 15 {
            return Err("draft rounds must be 1 to 30 and international rounds up to 15".into());
        }
        Ok(())
    }
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

/// See [`World::budget_limit`].
#[must_use]
pub fn budget_limit_for(
    market: f64,
    st: &Stadium,
    revenue_prev: f64,
    cash: f64,
    floor: f64,
) -> f64 {
    let other = 80.0 + 10.0 * market + 0.1 * st.capacity * st.quality;
    let room = 0.5 * (cash + CREDIT).max(0.0);
    (revenue_prev - other + room).max(floor)
}

/// Average ticket price the demand curve is anchored to, dollars.
pub const TICKET_REF: f64 = 35.0;
/// How fast attendance falls as the ticket price rises (a constant elasticity).
pub const TICKET_ELASTICITY: f64 = 1.5;
/// Share of a club's local revenue that is gate receipts at the reference price.
pub const GATE_SHARE: f64 = 0.55;
/// Home games per season.
pub const HOME_GAMES: f64 = 81.0;
/// Lowest and highest ticket price an owner may set, dollars.
pub const TICKET_RANGE: (f64, f64) = (12.0, 150.0);
/// Largest ballpark, thousands of seats.
pub const CAPACITY_MAX: f64 = 70.0;
/// Best ballpark quality (a demand multiplier).
pub const QUALITY_MAX: f64 = 1.3;

/// A club's ballpark and ticket price.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Stadium {
    /// Seats, thousands.
    pub capacity: f64,
    /// Multiplies demand; 1.0 is an ordinary park, wears down by a tenth of its
    /// distance from 1.0 each year.
    pub quality: f64,
    /// Average ticket price, dollars.
    pub ticket: f64,
}

impl Stadium {
    /// A park sized for a market (1.0 is average).
    #[must_use]
    pub fn for_market(market: f64) -> Self {
        Self {
            capacity: (30.0 + 12.0 * market).clamp(25.0, CAPACITY_MAX),
            quality: 1.0,
            ticket: TICKET_REF,
        }
    }

    /// Average crowd per home game (thousands) given what the market would
    /// bring to a park of average quality at the reference price.
    #[must_use]
    pub fn crowd(&self, base: f64) -> f64 {
        let demand = base * libm::pow(self.ticket / TICKET_REF, -TICKET_ELASTICITY) * self.quality;
        demand.min(self.capacity)
    }
}

impl Default for Stadium {
    fn default() -> Self {
        Self::for_market(1.0)
    }
}

/// What the owner tells the general manager the club is trying to do.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Stance {
    /// Win now: veterans over prospects.
    Contend,
    #[default]
    Balanced,
    /// Build for later: prospects over veterans.
    Rebuild,
}

/// How a club's field manager runs games.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Tactics {
    /// The engine's built-in rules.
    #[default]
    Baseline,
    /// Decides steals and pitching changes with run-expectancy and
    /// win-probability tables.
    Tables,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
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
    pub tactics: Tactics,
    #[serde(default)]
    pub stadium: Stadium,
    #[serde(default)]
    pub stance: Stance,
}

/// A trade one club has put to another and the other has not yet answered.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TradeOffer {
    pub id: u32,
    pub from: u8,
    pub to: u8,
    /// Players `from` gives up (and `to` receives).
    pub give: Vec<PlayerId>,
    /// Players `from` asks for.
    pub get: Vec<PlayerId>,
}

/// Most players on one side of a trade.
pub const TRADE_MAX_PER_SIDE: usize = 6;
/// Open offers one club may have out at a time.
pub const OFFERS_PER_CLUB: usize = 4;

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Draft {
    pub year: i32,
    /// Club on the clock for each pick (amateur rounds, then international).
    pub order: Vec<u8>,
    pub amateur_picks: usize,
    pub next: usize,
    pub remaining: BTreeSet<PlayerId>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SeasonRecord {
    pub year: i32,
    pub records: Vec<Record>,
    pub champion: Option<u8>,
    pub series: Vec<Series>,
    pub league: LeagueLine,
}

/// League totals for one season (all plate appearances in the majors).
#[derive(Debug, Clone, Copy, Default, PartialEq, serde::Serialize, serde::Deserialize)]
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

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Finance {
    pub revenue: f64,
    pub payroll: f64,
    pub other: f64,
    /// Bonuses and released contracts paid during the offseason before.
    pub spent: f64,
    pub dividend: f64,
    /// Average crowd per home game, thousands.
    #[serde(default)]
    pub attendance: f64,
    /// Gate receipts.
    #[serde(default)]
    pub gate: f64,
    /// The books ended below the credit line: the league arranged new
    /// ownership (fresh capital, ticket price and stance reset, payroll
    /// budget at the floor).
    #[serde(default)]
    pub rescued: bool,
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
    /// Trade offers waiting for an answer; they lapse when the season starts.
    pub offers: Vec<TradeOffer>,
    pub next_offer: u32,
    /// Era shift of talent, per kind (batters, pitchers): added to every
    /// player's talent so the average major leaguer stays at the league
    /// average (see `World::progress`). Moved only by progression events.
    pub era: [Z; 2],
    /// The regular season so far, while it is being played.
    pub season: Option<InSeason>,
    pub model: Model,
    pub env: Env,
    /// Run-expectancy and win-probability tables, built from the simulation
    /// the first time a manager needs them.
    pub(crate) tables: Arc<OnceLock<Arc<Tables>>>,
    /// What each club's table manager has decided so far. Diagnostic only:
    /// not part of the league state. Shared by every clone of the world, so
    /// counts from scratch copies (and what-if runs) are included.
    pub(crate) logs: Vec<Arc<DecisionLog>>,
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
            && self.offers == other.offers
            && self.next_offer == other.next_offer
            && self.era == other.era
            && self.season == other.season
    }
}

impl World {
    /// The most payroll the league lets a club budget: what last year's
    /// revenue left after other costs, plus half of the cash it has above
    /// its credit line. Never below the payroll floor.
    #[must_use]
    pub fn budget_limit(&self, team: usize) -> f64 {
        let c = &self.clubs[team];
        budget_limit_for(
            c.market,
            &c.stadium,
            c.revenue_prev,
            c.cash,
            self.rules.payroll_floor,
        )
    }

    /// What is wrong with a club's roster for starting a season (nothing, if
    /// the list is empty): the same rules `SeasonStarted` enforces.
    #[must_use]
    pub fn roster_problems(&self, team: u8) -> Vec<String> {
        use crate::player::Level;
        use crate::rating::Kind;
        let mut out = Vec::new();
        for kind in [Kind::Batter, Kind::Pitcher] {
            let n = self.kind_count(team, kind, Some(Level::Majors));
            if n != MAJORS_PER_KIND {
                out.push(format!(
                    "{n} {kind:?}s in the majors, needs {MAJORS_PER_KIND}"
                ));
            }
            for level in Level::MINORS {
                let n = self.kind_count(team, kind, Some(level));
                if n > level.capacity() {
                    out.push(format!(
                        "{n} {kind:?}s at {level:?}, room for {}",
                        level.capacity()
                    ));
                }
            }
            let all = self.kind_count(team, kind, None);
            if all > ROSTER_PER_KIND {
                out.push(format!(
                    "{all} {kind:?}s on the roster, limit {ROSTER_PER_KIND}"
                ));
            }
        }
        out
    }

    /// The box score of a game of this season, as text (see `play::box_score_text`).
    #[must_use]
    pub fn box_score(&self, number: u16) -> Option<String> {
        crate::play::box_score_text(self, number)
    }

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

/// Everything about a league that events change, in a form that can be
/// stored. The model and environment come from the game parameters and are
/// supplied again on [`World::restore`].
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Snapshot {
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
    pub last_records: Vec<Record>,
    pub offers: Vec<TradeOffer>,
    pub next_offer: u32,
    pub era: [Z; 2],
    pub season: Option<InSeason>,
}

impl World {
    /// The league as it stands, for storage.
    #[must_use]
    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            seed: self.seed,
            year: self.year,
            phase: self.phase,
            rules: self.rules.clone(),
            pool: self.pool,
            clubs: self.clubs.clone(),
            players: self.players.clone(),
            free_agents: self.free_agents.clone(),
            retired: self.retired.clone(),
            draft: self.draft.clone(),
            history: self.history.clone(),
            finances: self.finances.clone(),
            next_player: self.next_player,
            last_records: self.last_records.clone(),
            offers: self.offers.clone(),
            next_offer: self.next_offer,
            era: self.era,
            season: self.season.clone(),
        }
    }

    /// Rebuild a league from a snapshot and the parameters it was made with.
    /// Loaded data is never trusted: the snapshot must be consistent.
    pub fn restore(
        params: sim_core::GameParams,
        model: Model,
        s: Snapshot,
    ) -> Result<Self, crate::command::WorldError> {
        use crate::command::WorldError;
        let bad = |m: &str| Err(WorldError::Invalid(format!("snapshot: {m}")));
        let mut seen = BTreeSet::new();
        for (t, c) in s.clubs.iter().enumerate() {
            for id in &c.players {
                let Some(p) = s.players.get(id) else {
                    return bad("a club holds an unknown player");
                };
                if p.team != Some(t as u8) || !seen.insert(*id) {
                    return bad("a player is on the wrong club or on two clubs");
                }
            }
        }
        for id in &s.free_agents {
            match s.players.get(id) {
                Some(p) if p.team.is_none() && seen.insert(*id) => {}
                _ => return bad("a free agent is unknown, signed or listed twice"),
            }
        }
        if s.players
            .values()
            .any(|p| p.team.is_some() && !seen.contains(&p.id))
        {
            return bad("a signed player is on no roster");
        }
        if s.players.keys().any(|id| id.0 >= s.next_player)
            || s.retired.keys().any(|id| id.0 >= s.next_player)
        {
            return bad("next player id is in use");
        }
        if s.phase != Phase::Unfounded
            && (s.clubs.len() != TEAMS
                || s.finances.len() != s.clubs.len()
                || s.last_records.len() > s.clubs.len()
                || s.year < 0)
        {
            return bad("club counts do not match");
        }
        let known = |mut ids: &mut dyn Iterator<Item = &PlayerId>| {
            (&mut ids).all(|id| s.players.contains_key(id) || s.retired.contains_key(id))
        };
        if let Some(d) = &s.draft
            && (d.order.iter().any(|c| usize::from(*c) >= s.clubs.len())
                || !known(&mut d.remaining.iter()))
        {
            return bad("the draft names an unknown club or player");
        }
        if s.offers.iter().any(|o| {
            usize::from(o.from) >= s.clubs.len()
                || usize::from(o.to) >= s.clubs.len()
                || !known(&mut o.give.iter().chain(&o.get))
        }) {
            return bad("a trade offer names an unknown club or player");
        }
        if s.history.iter().any(|h| {
            h.champion.is_some_and(|c| usize::from(c) >= s.clubs.len())
                || h.series
                    .iter()
                    .any(|x| usize::from(x.a) >= s.clubs.len() || usize::from(x.b) >= s.clubs.len())
        }) {
            return bad("history names an unknown club");
        }
        if let Some(st) = &s.season
            && (st.records.len() != s.clubs.len()
                || !known(
                    &mut st
                        .lines
                        .keys()
                        .chain(st.out.keys())
                        .chain(st.loss.keys())
                        .chain(&st.hurt),
                ))
        {
            return bad("the season under way does not match the clubs");
        }
        if s.clubs.iter().any(|c| {
            let st = &c.stadium;
            !(st.capacity.is_finite()
                && st.quality.is_finite()
                && st.ticket.is_finite()
                && (1.0..=CAPACITY_MAX).contains(&st.capacity)
                && (0.1..=QUALITY_MAX).contains(&st.quality)
                && (TICKET_RANGE.0..=TICKET_RANGE.1).contains(&st.ticket))
        }) || (s.phase != Phase::Unfounded && s.rules.check().is_err())
        {
            return bad("a ballpark or the rules are outside their limits");
        }
        let finite = |x: f64| x.is_finite();
        if s.clubs
            .iter()
            .any(|c| !(finite(c.cash) && finite(c.budget)))
            || s.players
                .values()
                .any(|p| !(finite(p.quality) && p.z.iter().all(|x| finite(*x))))
            || s.era.iter().flatten().any(|x| !finite(*x))
        {
            return bad("numbers must be finite");
        }
        let mut w = World::new(s.seed, params, model, s.pool);
        w.year = s.year;
        w.phase = s.phase;
        w.rules = s.rules;
        w.clubs = s.clubs;
        w.players = s.players;
        w.free_agents = s.free_agents;
        w.retired = s.retired;
        w.draft = s.draft;
        w.history = s.history;
        w.finances = s.finances;
        w.next_player = s.next_player;
        w.last_records = s.last_records;
        w.offers = s.offers;
        w.next_offer = s.next_offer;
        w.era = s.era;
        w.season = s.season;
        Ok(w)
    }
}
