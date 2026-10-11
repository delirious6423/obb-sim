//! Turning commands into events.

use protocol::Actor;
use rand_core::RngCore;

use crate::command::{
    ClubFinanceState, Command, Event, Founding, Outcome, Update, WorldError, authorize,
};
use crate::player::{
    Contract, DEFAULT_POOL, Level, Origin, Player, PlayerId, Pool, prospect, slot_bonus, talent_at,
};
use crate::rating::{Kind, Model, normal, uniform};
use crate::scout::asking_salary;
use crate::state::{
    CREDIT, ClubState, Finance, GATE_SHARE, HOME_GAMES, MAJORS_PER_KIND, MIN_SALARY,
    OFFERS_PER_CLUB, Phase, ROSTER_MAX_PER_KIND, ROSTER_PER_KIND, Rules, Stadium, Stance, TEAMS,
    TICKET_RANGE, TICKET_REF, TRADE_MAX_PER_SIDE, World,
};
use crate::util::{stream, tag};

const YEAR_ZERO: i32 = 2030;
/// Share of the major leaguers' average talent offset the era shift removes each year.
const ERA_PULL: f64 = 0.5;
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
            offers: Vec::new(),
            next_offer: 0,
            era: [[0.0; crate::rating::K]; 2],
            season: None,
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

    fn check_trade_window(&self) -> Result<(), WorldError> {
        if matches!(
            self.phase,
            Phase::Progressed | Phase::Drafting | Phase::FreeAgency
        ) {
            Ok(())
        } else {
            Err(WorldError::Phase(self.phase))
        }
    }

    /// Whether `from` and `to` can swap these players right now.
    pub(crate) fn check_trade(
        &self,
        from: u8,
        to: u8,
        give: &[PlayerId],
        get: &[PlayerId],
    ) -> Result<(), String> {
        if from == to {
            return Err("a club cannot trade with itself".into());
        }
        if give.is_empty() && get.is_empty() {
            return Err("a trade moves at least one player".into());
        }
        if give.len() > TRADE_MAX_PER_SIDE || get.len() > TRADE_MAX_PER_SIDE {
            return Err("too many players in the trade".into());
        }
        let mut seen = std::collections::BTreeSet::new();
        for (ids, owner) in [(give, from), (get, to)] {
            for id in ids {
                if !seen.insert(*id) {
                    return Err("a player appears twice".into());
                }
                if !self.clubs[usize::from(owner)].players.contains(id) {
                    return Err(format!("player {} is not on club {owner}", id.0));
                }
            }
        }
        for (club, out, inn) in [(from, give, get), (to, get, give)] {
            for kind in [crate::rating::Kind::Batter, crate::rating::Kind::Pitcher] {
                let of =
                    |ids: &[PlayerId]| ids.iter().filter(|i| self.players[i].kind == kind).count();
                let after = self.kind_count(club, kind, None) + of(inn) - of(out);
                if after > ROSTER_MAX_PER_KIND {
                    return Err(format!("club {club} would hold {after} {kind:?}s"));
                }
                if after < MAJORS_PER_KIND && of(inn) < of(out) {
                    return Err(format!("club {club} would be left with {after} {kind:?}s"));
                }
            }
            let sal = |ids: &[PlayerId]| -> f64 {
                ids.iter()
                    .filter_map(|i| self.players[i].contract.map(|c| c.salary))
                    .sum()
            };
            let after = self.payroll(usize::from(club)) + sal(inn) - sal(out);
            if sal(inn) > sal(out) && after > self.rules.payroll_cap + 1e-9 {
                return Err(format!("club {club} would be over the payroll cap"));
            }
        }
        Ok(())
    }

    /// Owners settle money matters in the offseason.
    fn check_owner_window(&self) -> Result<(), WorldError> {
        if matches!(
            self.phase,
            Phase::Offseason | Phase::Progressed | Phase::Drafting | Phase::FreeAgency
        ) {
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
                Ok(crate::play::play_days(self, u16::MAX))
            }
            Command::PlayDays { days } => {
                self.need(Phase::Regular)?;
                if *days == 0 {
                    return Err(WorldError::Invalid("play at least one day".into()));
                }
                Ok(crate::play::play_days(self, *days))
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
                let mut ev = vec![Event::SeasonStarted { year: self.year }];
                ev.extend(
                    self.clubs
                        .iter()
                        .enumerate()
                        .filter(|(_, c)| c.cash < -CREDIT)
                        .map(|(t, _)| Event::ClubRescued { team: t as u8 }),
                );
                Ok(ev)
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
                let limit = self.budget_limit(usize::from(*team));
                if *payroll > limit + 1e-9 {
                    return Err(WorldError::Invalid(format!(
                        "the club's lenders will not back a payroll budget above {limit:.1}"
                    )));
                }
                Ok(vec![Event::BudgetSet {
                    team: *team,
                    payroll: *payroll,
                }])
            }
            Command::SetTicketPrice { team, price } => {
                self.check_team(*team)?;
                self.check_owner_window()?;
                if !price.is_finite() || *price < TICKET_RANGE.0 || *price > TICKET_RANGE.1 {
                    return Err(WorldError::Invalid(format!(
                        "ticket price must be between {} and {}",
                        TICKET_RANGE.0, TICKET_RANGE.1
                    )));
                }
                Ok(vec![Event::TicketSet {
                    team: *team,
                    price: *price,
                }])
            }
            Command::BuildStadium { team, amount } => {
                self.check_team(*team)?;
                self.check_owner_window()?;
                if !amount.is_finite() || *amount <= 0.0 || *amount > 200.0 {
                    return Err(WorldError::Invalid(
                        "a ballpark project costs more than 0 and at most 200".into(),
                    ));
                }
                if self.clubs[usize::from(*team)].cash - amount < -CREDIT {
                    return Err(WorldError::Invalid(
                        "the club cannot pay for the ballpark project".into(),
                    ));
                }
                Ok(vec![Event::StadiumBuilt {
                    team: *team,
                    amount: *amount,
                }])
            }
            Command::SetStance { team, stance } => {
                self.check_team(*team)?;
                self.check_owner_window()?;
                Ok(vec![Event::StanceSet {
                    team: *team,
                    stance: *stance,
                }])
            }
            Command::SetRules { rules } => {
                if !matches!(self.phase, Phase::Offseason | Phase::Progressed) {
                    return Err(WorldError::Phase(self.phase));
                }
                rules.check().map_err(WorldError::Invalid)?;
                rules
                    .check_change(&self.rules)
                    .map_err(WorldError::Invalid)?;
                Ok(vec![Event::RulesSet {
                    rules: rules.clone(),
                }])
            }
            Command::ProposeTrade {
                team,
                partner,
                give,
                get,
            } => {
                self.check_team(*team)?;
                self.check_team(*partner)?;
                self.check_trade_window()?;
                if self.offers.len() >= 4 * TEAMS
                    || self.offers.iter().filter(|o| o.from == *team).count() >= OFFERS_PER_CLUB
                {
                    return Err(WorldError::Invalid("too many open trade offers".into()));
                }
                self.check_trade(*team, *partner, give, get)
                    .map_err(WorldError::Invalid)?;
                Ok(vec![Event::TradeProposed {
                    offer: self.next_offer,
                    from: *team,
                    to: *partner,
                    give: give.clone(),
                    get: get.clone(),
                }])
            }
            Command::RespondTrade {
                team,
                offer,
                accept,
            } => {
                self.check_team(*team)?;
                self.check_trade_window()?;
                let o = self
                    .offers
                    .iter()
                    .find(|o| o.id == *offer && o.to == *team)
                    .ok_or_else(|| WorldError::Invalid("no such offer for this club".into()))?;
                if *accept {
                    self.check_trade(o.from, o.to, &o.give, &o.get)
                        .map_err(WorldError::Invalid)?;
                }
                Ok(vec![Event::TradeAnswered {
                    offer: *offer,
                    accepted: *accept,
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
                if !(1..=crate::agents::MAX_YEARS).contains(years) {
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
                // A club over its roster limits must be able to cut down, however
                // little cash it has; otherwise a bad signer could stall the league.
                let over_limit = self.kind_count(*team, p.kind, None) > ROSTER_PER_KIND;
                if !over_limit && self.clubs[usize::from(*team)].cash - owed < -CREDIT {
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
        let mut crowd = vec![0.0; n];
        let mut gate = vec![0.0; n];
        let mut win_pct = vec![0.5; n];
        for i in 0..n {
            let r = &self.last_records[i];
            win_pct[i] = f64::from(r.w) / f64::from((r.w + r.l).max(1));
            hype[i] =
                (0.6 * self.clubs[i].hype + 0.4 * (1.0 + 4.0 * (win_pct[i] - 0.5))).clamp(0.4, 1.8);
            // What the market is worth this year; gate receipts are the
            // part the owner can move with the ticket price and the park.
            let market_local = 190.0 * self.clubs[i].market * (0.75 + 0.5 * hype[i]);
            let base = GATE_SHARE * market_local * 1000.0 / (HOME_GAMES * TICKET_REF);
            let st = &self.clubs[i].stadium;
            crowd[i] = st.crowd(base);
            gate[i] = crowd[i] * HOME_GAMES * st.ticket / 1000.0;
            local[i] = (1.0 - GATE_SHARE) * market_local + gate[i];
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
            let st = &self.clubs[i].stadium;
            let other = 80.0 + 10.0 * self.clubs[i].market + 0.1 * st.capacity * st.quality;
            let profit = revenue - payroll - other;
            let cash = self.clubs[i].cash;
            let dividend = ((cash + profit) - 0.35 * revenue).max(0.0) * 0.6;
            let rescued = cash + profit - dividend < -CREDIT;
            out.push((
                Finance {
                    revenue,
                    payroll,
                    other,
                    spent: self.clubs[i].spent,
                    dividend,
                    attendance: crowd[i],
                    gate: gate[i],
                    rescued,
                },
                ClubFinanceState {
                    cash: if rescued {
                        0.1 * revenue
                    } else {
                        cash + profit - dividend
                    },
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
        let mut updates: Vec<Update> = Vec::with_capacity(self.players.len());
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
            let (z, q) = talent_at(
                &self.model,
                &aged,
                age,
                &self.era[usize::from(p.kind == Kind::Pitcher)],
            );
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
        // The era moves so that the average major leaguer stays at the league
        // average: without it the talent mix wanders (nothing in the game
        // pulls the run environment back), and the league drifts away from
        // the real one. This is the whole league adjusting, not a bonus to
        // any club.
        let mut era = self.era;
        for kind in [Kind::Batter, Kind::Pitcher] {
            let k = usize::from(kind == Kind::Pitcher);
            let mut sum = [0.0; crate::rating::K];
            let mut n = 0.0;
            for u in &updates {
                let p = &self.players[&u.player];
                if p.kind == kind && p.level == Level::Majors {
                    for (s, z) in sum.iter_mut().zip(&u.z) {
                        *s += z;
                    }
                    n += 1.0;
                }
            }
            if n == 0.0 {
                continue;
            }
            let shift: crate::rating::Z = sum.map(|s| -ERA_PULL * s / n);
            for (e, d) in era[k].iter_mut().zip(shift) {
                *e += d;
            }
            for u in updates
                .iter_mut()
                .filter(|u| self.players[&u.player].kind == kind)
            {
                for (z, d) in u.z.iter_mut().zip(shift) {
                    *z += d;
                }
                u.quality = self.model.quality(kind, &u.z);
            }
        }
        Event::PlayersProgressed { year, updates, era }
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
                &self.era,
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
                    stadium: Stadium::for_market(markets[t]),
                    stance: Stance::default(),
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
