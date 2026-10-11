//! Turning commands into events.

use protocol::Actor;
use rand_core::RngCore;

use crate::command::{
    ClubFinanceState, Command, Event, Founding, Outcome, Update, WorldError, authorize,
};
use crate::player::{
    Contract, DEFAULT_POOL, Origin, Player, PlayerId, Pool, prospect, slot_bonus, talent_at,
};
use crate::rating::{Model, normal, uniform};
use crate::scout::asking_salary;
use crate::state::{CREDIT, ClubState, Finance, MIN_SALARY, Phase, Rules, TEAMS, World};
use crate::util::{stream, tag};

const YEAR_ZERO: i32 = 2030;
const AMATEURS: usize = 600;
const INTERNATIONALS: usize = 200;
/// International signings use a flat slot (millions).
const INTERNATIONAL_SLOT: f64 = 1.0;

const CITIES: [&str; 15] = [
    "Harbor City",
    "Pine Ridge",
    "Stone Valley",
    "Red Mesa",
    "Lake Aurel",
    "Copper Bay",
    "Iron Hollow",
    "Silver Fork",
    "Cedar Point",
    "Maple Run",
    "Granite Falls",
    "North Haven",
    "Amber Coast",
    "Willow Creek",
    "Falcon Rock",
];
const MASCOTS: [&str; 15] = [
    "Gulls", "Foxes", "Herons", "Badgers", "Marlins", "Wolves", "Otters", "Rams", "Kestrels",
    "Bison", "Lynx", "Pelicans", "Stags", "Cranes", "Mariners",
];

impl World {
    /// A world before founding. `model` and `params` come from the data (or
    /// are generic, in tests).
    #[must_use]
    pub fn new(seed: u64, params: sim_core::GameParams, model: Model, pool: Pool) -> Self {
        Self {
            seed,
            year: YEAR_ZERO,
            phase: Phase::Unfounded,
            rules: Rules::default(),
            pool,
            clubs: Vec::new(),
            players: Default::default(),
            free_agents: Default::default(),
            retired: Default::default(),
            draft: None,
            history: Vec::new(),
            finances: Vec::new(),
            next_player: 0,
            last_records: Vec::new(),
            model,
            env: sim_core::Env::new(params).expect("valid parameters"),
            tables: Default::default(),
            logs: (0..TEAMS).map(|_| Default::default()).collect(),
        }
    }

    #[must_use]
    pub fn with_default_pool(seed: u64, params: sim_core::GameParams, model: Model) -> Self {
        Self::new(seed, params, model, DEFAULT_POOL)
    }

    /// Check, decide and apply a command. Commands that make several events
    /// are applied to a copy first, so a failure leaves the world untouched.
    pub fn execute(&mut self, actor: &Actor, command: Command) -> Result<Vec<Event>, WorldError> {
        authorize(actor, &command)?;
        let events = self.decide(&command)?;
        if events.len() <= 1 {
            for e in &events {
                self.apply(e)?;
            }
        } else {
            let mut scratch = self.clone();
            for e in &events {
                scratch.apply(e)?;
            }
            *self = scratch;
        }
        Ok(events)
    }

    /// Rebuild a world from its events.
    pub fn from_events(
        seed: u64,
        params: sim_core::GameParams,
        model: Model,
        pool: Pool,
        events: &[Event],
    ) -> Result<Self, WorldError> {
        let mut w = Self::new(seed, params, model, pool);
        for e in events {
            w.apply(e)?;
        }
        Ok(w)
    }

    fn need(&self, phase: Phase) -> Result<(), WorldError> {
        if self.phase == phase {
            Ok(())
        } else {
            Err(WorldError::Phase(self.phase))
        }
    }

    fn check_team(&self, team: u8) -> Result<(), WorldError> {
        if usize::from(team) < self.clubs.len() {
            Ok(())
        } else {
            Err(WorldError::UnknownClub(team))
        }
    }

    pub(crate) fn decide(&self, command: &Command) -> Result<Vec<Event>, WorldError> {
        match command {
            Command::Found { burn_in_years } => {
                self.need(Phase::Unfounded)?;
                Ok(vec![self.found_event(*burn_in_years)])
            }
            Command::PlaySeason => {
                self.need(Phase::Regular)?;
                Ok(vec![crate::play::play_regular(self)])
            }
            Command::PlayPlayoffs => {
                self.need(Phase::Playoffs)?;
                let clubs = crate::play::playoff_clubs(self);
                let po = season::play_playoffs(
                    &self.env,
                    &clubs,
                    &self.last_records,
                    self.seed,
                    self.year as u32,
                );
                Ok(vec![Event::PlayoffsPlayed {
                    year: self.year,
                    series: po.series,
                    champion: po.champion as u8,
                }])
            }
            Command::SettleFinances => {
                self.need(Phase::Settling)?;
                Ok(vec![self.settle()])
            }
            Command::ProgressPlayers => {
                self.need(Phase::Offseason)?;
                Ok(vec![self.progress()])
            }
            Command::OpenDraft => {
                self.need(Phase::Progressed)?;
                Ok(vec![self.open_draft()])
            }
            Command::CloseDraft => {
                self.need(Phase::Drafting)?;
                let d = self.draft.as_ref().ok_or(WorldError::Phase(self.phase))?;
                if d.next < d.order.len() {
                    return Err(WorldError::Invalid("the draft is not finished".into()));
                }
                Ok(vec![Event::DraftClosed])
            }
            Command::StartSeason => {
                self.need(Phase::FreeAgency)?;
                Ok(vec![Event::SeasonStarted { year: self.year }])
            }
            Command::SetBudget { team, payroll } => {
                self.check_team(*team)?;
                if !payroll.is_finite()
                    || *payroll < self.rules.payroll_floor
                    || *payroll > self.rules.payroll_cap
                {
                    return Err(WorldError::Invalid(format!(
                        "budget must be between {} and {}",
                        self.rules.payroll_floor, self.rules.payroll_cap
                    )));
                }
                Ok(vec![Event::BudgetSet {
                    team: *team,
                    payroll: *payroll,
                }])
            }
            Command::SetTactics { team, tactics } => {
                self.check_team(*team)?;
                if self.phase == Phase::Unfounded {
                    return Err(WorldError::Phase(self.phase));
                }
                Ok(vec![Event::TacticsSet {
                    team: *team,
                    tactics: *tactics,
                }])
            }
            Command::DraftPlayer { team, player } => {
                self.need(Phase::Drafting)?;
                self.check_team(*team)?;
                let d = self.draft.as_ref().ok_or(WorldError::Phase(self.phase))?;
                if d.order.get(d.next) != Some(team) {
                    return Err(WorldError::Invalid("not this club's pick".into()));
                }
                if !d.remaining.contains(player) {
                    return Err(WorldError::Invalid("player is not in the draft".into()));
                }
                let p = self
                    .player(*player)
                    .ok_or(WorldError::UnknownPlayer(*player))?;
                let intl_round = d.next >= d.amateur_picks;
                if intl_round != (p.origin == Origin::International) {
                    return Err(WorldError::Invalid("wrong pool for this round".into()));
                }
                let slot = if intl_round {
                    INTERNATIONAL_SLOT
                } else {
                    slot_bonus((d.next + 1) as u16)
                };
                let demand = crate::scout::amateur_demand(self, p);
                let bonus = (demand <= 1.5 * slot).then_some(demand.max(0.1));
                if let Some(b) = bonus
                    && self.clubs[usize::from(*team)].cash - b < -CREDIT
                {
                    return Err(WorldError::Invalid("club cannot afford the bonus".into()));
                }
                Ok(vec![Event::PlayerDrafted {
                    pick: d.next,
                    team: *team,
                    player: *player,
                    bonus,
                }])
            }
            Command::PassPick { team } => {
                self.need(Phase::Drafting)?;
                let d = self.draft.as_ref().ok_or(WorldError::Phase(self.phase))?;
                if d.order.get(d.next) != Some(team) {
                    return Err(WorldError::Invalid("not this club's pick".into()));
                }
                Ok(vec![Event::PickPassed {
                    pick: d.next,
                    team: *team,
                }])
            }
            Command::SignFreeAgent {
                team,
                player,
                salary,
                years,
            } => {
                self.need(Phase::FreeAgency)?;
                self.check_team(*team)?;
                if !self.free_agents.contains(player) {
                    return Err(WorldError::Invalid("player is not a free agent".into()));
                }
                let p = self
                    .player(*player)
                    .ok_or(WorldError::UnknownPlayer(*player))?;
                if !(1..=7).contains(years) {
                    return Err(WorldError::Invalid("contracts run 1 to 7 years".into()));
                }
                if !salary.is_finite() || *salary < asking_salary(self, p).max(MIN_SALARY) - 1e-9 {
                    return Err(WorldError::Invalid(
                        "offer is below the asking salary".into(),
                    ));
                }
                let after = self.payroll(usize::from(*team)) + salary;
                let budget = self.clubs[usize::from(*team)].budget;
                if after > budget + 1e-9 && *salary > MIN_SALARY + 1e-9 {
                    return Err(WorldError::Invalid(
                        "over the owner's payroll budget".into(),
                    ));
                }
                if after > self.rules.payroll_cap + 1e-9 && *salary > MIN_SALARY + 1e-9 {
                    return Err(WorldError::Invalid("over the payroll cap".into()));
                }
                Ok(vec![Event::FreeAgentSigned {
                    team: *team,
                    player: *player,
                    contract: Contract {
                        salary: *salary,
                        years: *years,
                    },
                }])
            }
            Command::ReleasePlayer { team, player } => {
                self.check_team(*team)?;
                let p = self
                    .player(*player)
                    .ok_or(WorldError::UnknownPlayer(*player))?;
                let owed = p.contract.map_or(0.0, |c| c.salary * f64::from(c.years));
                if self.clubs[usize::from(*team)].cash - owed < -CREDIT {
                    return Err(WorldError::Invalid(
                        "club cannot afford to pay off the contract".into(),
                    ));
                }
                Ok(vec![Event::PlayerReleased {
                    team: *team,
                    player: *player,
                }])
            }
            Command::SetLevel {
                team,
                player,
                level,
            } => {
                self.check_team(*team)?;
                self.player(*player)
                    .ok_or(WorldError::UnknownPlayer(*player))?;
                Ok(vec![Event::LevelSet {
                    team: *team,
                    player: *player,
                    level: *level,
                }])
            }
        }
    }

    // ---- finances ---------------------------------------------------

    fn settle(&self) -> Event {
        let n = self.clubs.len();
        let mut hype = vec![0.0; n];
        let mut local = vec![0.0; n];
        let mut win_pct = vec![0.5; n];
        for i in 0..n {
            let r = &self.last_records[i];
            win_pct[i] = f64::from(r.w) / f64::from((r.w + r.l).max(1));
            hype[i] =
                (0.6 * self.clubs[i].hype + 0.4 * (1.0 + 4.0 * (win_pct[i] - 0.5))).clamp(0.4, 1.8);
            local[i] = 190.0 * self.clubs[i].market * (0.75 + 0.5 * hype[i]);
        }
        let pool = self.rules.revenue_sharing * local.iter().sum::<f64>();
        let mut appearances = vec![0u32; n];
        if let Some(rec) = self.history.last() {
            for s in &rec.series {
                appearances[usize::from(s.a)] += 1;
                appearances[usize::from(s.b)] += 1;
            }
        }
        let mut out = Vec::with_capacity(n);
        for i in 0..n {
            let revenue = local[i] * (1.0 - self.rules.revenue_sharing)
                + pool / n as f64
                + 85.0
                + 6.0 * f64::from(appearances[i]);
            let payroll = self.payroll(i);
            let other = 80.0 + 10.0 * self.clubs[i].market;
            let profit = revenue - payroll - other;
            let cash = self.clubs[i].cash;
            let dividend = ((cash + profit) - 0.35 * revenue).max(0.0) * 0.6;
            out.push((
                Finance {
                    revenue,
                    payroll,
                    other,
                    spent: self.clubs[i].spent,
                    dividend,
                },
                ClubFinanceState {
                    cash: cash + profit - dividend,
                    revenue_prev: revenue,
                    hype: hype[i],
                    win_pct_prev: win_pct[i],
                },
            ));
        }
        Event::FinancesSettled {
            year: self.year,
            clubs: out,
        }
    }

    // ---- ageing -------------------------------------------------------

    fn progress(&self) -> Event {
        let year = self.year + 1;
        let mut rng = stream(self.seed, tag::PROGRESS, year as u64, 0);
        let sdq = |p: &Player| self.model.quality_sd(p.kind);
        let mut updates = Vec::with_capacity(self.players.len());
        for p in self.players.values() {
            // Young players' development is much less predictable.
            let age_now = p.age(year);
            let youth = (f64::from(26 - age_now) / 8.0).clamp(0.0, 1.0);
            let luck = (0.06 + 0.55 * youth) * normal(&mut rng);
            let retire_roll = uniform(&mut rng);
            let drift = (p.drift + p.pending_loss + luck).clamp(-1.5, 3.0);
            let mut aged = p.clone();
            aged.drift = drift;
            let age = p.age(year);
            let (z, q) = talent_at(&self.model, &aged, age);
            let mut contract = p.contract;
            let mut outcome = Outcome::Stays;
            if let Some(c) = &mut contract {
                c.years = c.years.saturating_sub(1);
                if c.years == 0 {
                    outcome = Outcome::Expired;
                }
            }
            // Retirement: more likely when old and poor, or when nobody signs him.
            let hazard = if age >= 30 {
                let base = 1.0 / (1.0 + libm::exp(-(f64::from(age) - 37.0) / 2.2));
                let skill = (q / sdq(p)).clamp(-3.0, 3.0);
                (base * libm::exp(-0.7 * skill)).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let unsigned_too_long = p.team.is_none() && p.free_years >= 1;
            if retire_roll < hazard || age >= 42 || unsigned_too_long {
                outcome = Outcome::Retired;
            }
            updates.push(Update {
                player: p.id,
                drift,
                z,
                quality: q,
                contract,
                out_games: p.out_games.saturating_sub(162),
                outcome,
            });
        }
        Event::PlayersProgressed { year, updates }
    }

    // ---- the draft -----------------------------------------------------

    fn open_draft(&self) -> Event {
        let year = self.year;
        let mut rng = stream(self.seed, tag::DRAFT_CLASS, year as u64, 0);
        let mut next = self.next_player;
        let mut prospects = Vec::with_capacity(AMATEURS + INTERNATIONALS);
        for i in 0..AMATEURS + INTERNATIONALS {
            let origin = if i >= AMATEURS {
                Origin::International
            } else if uniform(&mut rng) < 0.5 {
                Origin::HighSchool
            } else {
                Origin::College
            };
            prospects.push(prospect(
                &self.model,
                &mut rng,
                PlayerId(next),
                year,
                origin,
                self.pool,
            ));
            next += 1;
        }
        // Worst record picks first; with no record yet, a shuffle.
        let n = self.clubs.len();
        let mut clubs: Vec<u8> = (0..n as u8).collect();
        if self.last_records.len() == n {
            clubs.sort_by_key(|c| {
                let r = &self.last_records[usize::from(*c)];
                (r.w, r.run_diff(), *c)
            });
        } else {
            let mut r = stream(self.seed, tag::ORDER, year as u64, 0);
            for i in (1..n).rev() {
                let j = (r.next_u64() % (i as u64 + 1)) as usize;
                clubs.swap(i, j);
            }
        }
        let amateur_picks = usize::from(self.rules.draft_rounds) * n;
        let mut order = Vec::new();
        for _ in 0..self.rules.draft_rounds {
            order.extend(&clubs);
        }
        for _ in 0..self.rules.international_rounds {
            order.extend(&clubs);
        }
        Event::DraftOpened {
            year,
            prospects,
            order,
            amateur_picks,
            next_player: next,
        }
    }

    // ---- founding ------------------------------------------------------

    fn found_event(&self, burn_in: u8) -> Event {
        let mut rng = stream(self.seed, tag::FOUND, 0, 0);
        let mut markets: Vec<f64> = (0..TEAMS)
            .map(|_| libm::exp(0.3 * normal(&mut rng)))
            .collect();
        let mean = markets.iter().sum::<f64>() / TEAMS as f64;
        for m in &mut markets {
            *m /= mean;
        }
        let rules = Rules::default();
        let clubs: Vec<ClubState> = (0..TEAMS)
            .map(|t| {
                let expected = 190.0 * markets[t] + 85.0;
                ClubState {
                    name: format!("{} {}", CITIES[t % 15], MASCOTS[(t / 15 + t) % 15]),
                    market: markets[t],
                    cash: 0.2 * expected,
                    budget: 0.0,
                    revenue_prev: expected,
                    spent: 0.0,
                    hype: 1.0,
                    win_pct_prev: 0.5,
                    players: Default::default(),
                    tactics: Default::default(),
                }
            })
            .collect();
        let mut scratch = self.clone();
        scratch.rules = rules.clone();
        scratch.clubs = clubs;
        scratch.finances = vec![Vec::new(); TEAMS];
        scratch.year = YEAR_ZERO - i32::from(burn_in);
        scratch.phase = Phase::Offseason;
        crate::driver::burn_in(&mut scratch, burn_in);
        debug_assert_eq!(scratch.phase, Phase::Regular);
        // The burn-in earns no revenue, so the founders restore each club's
        // opening cash rather than hand over whatever the draft left.
        for c in &mut scratch.clubs {
            c.cash = 0.2 * (190.0 * c.market + 85.0);
        }
        Event::Founded(Box::new(Founding {
            year: scratch.year,
            rules,
            clubs: scratch.clubs.clone(),
            players: scratch.players.values().cloned().collect(),
            free_agents: scratch.free_agents.iter().copied().collect(),
            next_player: scratch.next_player,
        }))
    }
}
