//! Events are the only thing that changes the world.

use crate::command::{ClubFinanceState, Event, Founding, Line, Outcome, WorldError};
use crate::player::{Contract, Level, Player, PlayerId};
use crate::state::{
    CAPACITY_MAX, CREDIT, Draft, InSeason, MAJORS_PER_KIND, MINOR_COST, Phase, QUALITY_MAX,
    ROSTER_MAX_PER_KIND, ROSTER_PER_KIND, SeasonRecord, TICKET_RANGE, World,
};

fn invalid<T>(msg: impl Into<String>) -> Result<T, WorldError> {
    Err(WorldError::Invalid(msg.into()))
}

fn expect_phase(w: &World, phase: Phase) -> Result<(), WorldError> {
    if w.phase == phase {
        Ok(())
    } else {
        Err(WorldError::Phase(w.phase))
    }
}

impl World {
    fn club_mut(&mut self, team: u8) -> Result<&mut crate::state::ClubState, WorldError> {
        self.clubs
            .get_mut(usize::from(team))
            .ok_or(WorldError::UnknownClub(team))
    }

    /// Players of `kind` on a club, optionally at one level.
    pub(crate) fn kind_count(
        &self,
        team: u8,
        kind: crate::rating::Kind,
        level: Option<Level>,
    ) -> usize {
        self.clubs[usize::from(team)]
            .players
            .iter()
            .filter_map(|id| self.players.get(id))
            .filter(|p| p.kind == kind && level.is_none_or(|l| p.level == l))
            .count()
    }

    fn player_mut(&mut self, id: PlayerId) -> Result<&mut Player, WorldError> {
        self.players
            .get_mut(&id)
            .ok_or(WorldError::UnknownPlayer(id))
    }

    fn found(&mut self, f: &Founding) -> Result<(), WorldError> {
        expect_phase(self, Phase::Unfounded)?;
        self.year = f.year;
        self.rules = f.rules.clone();
        self.clubs = f.clubs.clone();
        self.players.clear();
        for p in &f.players {
            if let Some(t) = p.team {
                let club = self
                    .clubs
                    .get_mut(usize::from(t))
                    .ok_or(WorldError::UnknownClub(t))?;
                club.players.insert(p.id);
            }
            self.players.insert(p.id, p.clone());
        }
        self.free_agents = f.free_agents.iter().copied().collect();
        self.next_player = f.next_player;
        self.finances = vec![Vec::new(); self.clubs.len()];
        self.phase = Phase::Regular;
        Ok(())
    }

    /// Check an event against the world without changing anything, so that
    /// `apply` never fails half-way through a multi-step event. Never trust
    /// loaded events.
    fn validate(&self, e: &Event) -> Result<(), WorldError> {
        let clubs = self.clubs.len();
        let club_ok = |t: u8| usize::from(t) < clubs;
        match e {
            Event::Founded(f) => {
                if f.clubs.is_empty() || f.clubs.len() > usize::from(u8::MAX) {
                    return invalid("a league needs between 1 and 255 clubs");
                }
                let mut seen = std::collections::BTreeSet::new();
                let mut top = 0;
                for p in &f.players {
                    if !seen.insert(p.id) {
                        return invalid("duplicate player id");
                    }
                    top = top.max(p.id.0 + 1);
                    if let Some(t) = p.team
                        && usize::from(t) >= f.clubs.len()
                    {
                        return Err(WorldError::UnknownClub(t));
                    }
                }
                for id in &f.free_agents {
                    let known = f.players.iter().any(|p| p.id == *id && p.team.is_none());
                    if !known {
                        return invalid("free agent is not an unattached player");
                    }
                }
                if f.next_player < top {
                    return invalid("next player id is already in use");
                }
            }
            Event::DaysPlayed {
                results,
                lines,
                segment_end,
                ..
            } => {
                if results
                    .iter()
                    .any(|r| !club_ok(r.away) || !club_ok(r.home) || r.away == r.home)
                {
                    return invalid("a game names an unknown club");
                }
                let hurt = segment_end.iter().flat_map(|s| {
                    s.out
                        .keys()
                        .copied()
                        .chain(s.hurt.iter().copied())
                        .chain(s.loss.iter().map(|(id, _)| *id))
                });
                for id in lines.iter().map(|(id, _)| *id).chain(hurt) {
                    if !self.players.contains_key(&id) {
                        return Err(WorldError::UnknownPlayer(id));
                    }
                }
                if segment_end
                    .iter()
                    .any(|s| s.loss.iter().any(|(_, l)| !l.is_finite()))
                {
                    return invalid("injury damage must be finite");
                }
            }
            Event::SeasonPlayed {
                lines, injuries, ..
            } => {
                for id in lines
                    .iter()
                    .map(|l| l.player)
                    .chain(injuries.iter().map(|i| i.player))
                {
                    if !self.players.contains_key(&id) {
                        return Err(WorldError::UnknownPlayer(id));
                    }
                }
            }
            Event::PlayoffsPlayed {
                series, champion, ..
            } => {
                if !club_ok(*champion) || series.iter().any(|s| !club_ok(s.a) || !club_ok(s.b)) {
                    return invalid("playoffs name an unknown club");
                }
            }
            Event::FinancesSettled { clubs: c, .. } => {
                let finite = |x: f64| x.is_finite();
                if c.iter().any(|(f, s)| {
                    !(finite(f.revenue)
                        && finite(f.payroll)
                        && finite(f.other)
                        && finite(f.dividend)
                        && finite(s.cash)
                        && finite(s.hype))
                }) {
                    return invalid("finances must be finite numbers");
                }
            }
            Event::TacticsSet { team, .. } => {
                if !club_ok(*team) {
                    return Err(WorldError::UnknownClub(*team));
                }
            }
            Event::TradeProposed { from, to, .. } => {
                if !club_ok(*from) || !club_ok(*to) {
                    return invalid("trade names an unknown club");
                }
            }
            Event::TradeAnswered { .. } => {}
            Event::BudgetSet { team, payroll } => {
                if !club_ok(*team) {
                    return Err(WorldError::UnknownClub(*team));
                }
                if !payroll.is_finite()
                    || *payroll < self.rules.payroll_floor
                    || *payroll > self.rules.payroll_cap
                {
                    return invalid("budget outside the payroll limits");
                }
                if *payroll > self.budget_limit(usize::from(*team)) + 1e-9 {
                    return invalid("budget above what the club's lenders allow");
                }
            }
            Event::TicketSet { team, price } => {
                if !club_ok(*team) {
                    return Err(WorldError::UnknownClub(*team));
                }
                if !price.is_finite() || *price < TICKET_RANGE.0 || *price > TICKET_RANGE.1 {
                    return invalid("ticket price outside the allowed range");
                }
            }
            Event::StadiumBuilt { team, amount } => {
                if !club_ok(*team) {
                    return Err(WorldError::UnknownClub(*team));
                }
                if !amount.is_finite() || *amount <= 0.0 || *amount > 200.0 {
                    return invalid("ballpark project outside the allowed size");
                }
                if self.clubs[usize::from(*team)].cash - amount < -CREDIT {
                    return invalid("the club cannot pay for the ballpark project");
                }
            }
            Event::StanceSet { team, .. } => {
                if !club_ok(*team) {
                    return Err(WorldError::UnknownClub(*team));
                }
            }
            Event::ClubRescued { team } => {
                if !club_ok(*team) {
                    return Err(WorldError::UnknownClub(*team));
                }
                if self.clubs[usize::from(*team)].cash >= -CREDIT {
                    return invalid("the club is not below its credit line");
                }
            }
            Event::RulesSet { rules } => {
                if self.rules_set == Some(self.year) {
                    return invalid("the rules were already changed this year");
                }
                if let Err(m) = rules.check().and_then(|()| rules.check_change(&self.rules)) {
                    return Err(WorldError::Invalid(m));
                }
            }
            Event::PlayersProgressed { updates, era, .. } => {
                if era.iter().flatten().any(|x| !x.is_finite()) {
                    return invalid("era shift must be finite");
                }
                let mut seen = std::collections::BTreeSet::new();
                for u in updates {
                    let p = self
                        .players
                        .get(&u.player)
                        .ok_or(WorldError::UnknownPlayer(u.player))?;
                    if !seen.insert(u.player) {
                        return invalid("player updated twice");
                    }
                    if u.outcome == Outcome::Expired && p.team.is_none() {
                        return invalid("a free agent cannot expire");
                    }
                    if !u.drift.is_finite() || !u.quality.is_finite() {
                        return invalid("progression must be finite");
                    }
                }
            }
            Event::DraftOpened {
                prospects,
                order,
                amateur_picks,
                next_player,
                ..
            } => {
                let mut seen = std::collections::BTreeSet::new();
                for p in prospects {
                    if self.players.contains_key(&p.id)
                        || self.retired.contains_key(&p.id)
                        || !seen.insert(p.id)
                        || p.id.0 >= *next_player
                        || p.team.is_some()
                    {
                        return invalid("draft class has a bad player id");
                    }
                }
                if *amateur_picks > order.len() || order.iter().any(|t| !club_ok(*t)) {
                    return invalid("draft order is not valid");
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// Apply one event. The event is validated first, so a refused event
    /// leaves the world untouched.
    pub fn apply(&mut self, e: &Event) -> Result<(), WorldError> {
        self.validate(e)?;
        match e {
            Event::Founded(f) => self.found(f),
            Event::DaysPlayed { year, to_day, .. } => {
                expect_phase(self, Phase::Regular)?;
                if *year != self.year {
                    return invalid("days do not match the world");
                }
                if usize::from(*to_day) > crate::play::season_days(self) {
                    return invalid("days run past the end of the season");
                }
                let mut st = self.season.take().unwrap_or_else(|| InSeason::start(self));
                if *to_day <= st.day {
                    self.season = Some(st);
                    return invalid("days must move forward");
                }
                st.absorb(e);
                self.season = Some(st);
                Ok(())
            }
            Event::SeasonPlayed {
                year,
                records,
                lines,
                injuries,
                league,
            } => {
                expect_phase(self, Phase::Regular)?;
                if *year != self.year || records.len() != self.clubs.len() {
                    return invalid("season does not match the world");
                }
                if let Some(st) = &self.season
                    && usize::from(st.day) != crate::play::season_days(self)
                {
                    return invalid("the season is not over");
                }
                for l in lines {
                    let (played, _) = match l.line {
                        Line::Bat(b) => (b.pa > 0, ()),
                        Line::Pitch(p) => (p.bf > 0, ()),
                    };
                    let year = self.year;
                    let p = self.player_mut(l.player)?;
                    p.career.war += l.war;
                    if played {
                        p.career.seasons += 1;
                        p.debut.get_or_insert(year);
                    }
                }
                for i in injuries {
                    let p = self.player_mut(i.player)?;
                    p.out_games = i.out_games;
                    p.pending_loss += i.loss;
                }
                self.last_records = records.clone();
                self.history.push(SeasonRecord {
                    year: *year,
                    records: records.clone(),
                    champion: None,
                    series: Vec::new(),
                    league: *league,
                });
                self.phase = Phase::Playoffs;
                Ok(())
            }
            Event::PlayoffsPlayed {
                year,
                series,
                champion,
            } => {
                expect_phase(self, Phase::Playoffs)?;
                let rec = self.history.last_mut().filter(|r| r.year == *year);
                let Some(rec) = rec else {
                    return invalid("no season to attach playoffs to");
                };
                rec.champion = Some(*champion);
                rec.series = series.clone();
                self.phase = Phase::Settling;
                Ok(())
            }
            Event::FinancesSettled { year, clubs } => {
                expect_phase(self, Phase::Settling)?;
                if *year != self.year || clubs.len() != self.clubs.len() {
                    return invalid("finances do not match the world");
                }
                let mut rescued = Vec::new();
                for (i, (fin, st)) in clubs.iter().enumerate() {
                    let ClubFinanceState {
                        cash,
                        revenue_prev,
                        hype,
                        win_pct_prev,
                    } = *st;
                    let c = &mut self.clubs[i];
                    c.cash = cash;
                    c.revenue_prev = revenue_prev;
                    c.spent = 0.0;
                    c.hype = hype;
                    c.win_pct_prev = win_pct_prev;
                    // Ballparks wear: quality drifts back toward ordinary.
                    c.stadium.quality = 1.0 + (c.stadium.quality - 1.0) * 0.9;
                    if fin.rescued {
                        c.stadium.ticket = crate::state::TICKET_REF;
                        c.stance = crate::state::Stance::Balanced;
                        c.budget = c.budget.min(self.rules.payroll_floor);
                        rescued.push(i);
                    }
                    self.finances[i].push(fin.clone());
                }
                for i in rescued {
                    self.receivership(i);
                }
                self.phase = Phase::Offseason;
                Ok(())
            }
            Event::BudgetSet { team, payroll } => {
                if !matches!(
                    self.phase,
                    Phase::Offseason | Phase::Progressed | Phase::Drafting | Phase::FreeAgency
                ) {
                    return Err(WorldError::Phase(self.phase));
                }
                self.club_mut(*team)?.budget = *payroll;
                Ok(())
            }
            Event::TicketSet { team, price } => {
                owner_window(self)?;
                self.club_mut(*team)?.stadium.ticket = *price;
                Ok(())
            }
            Event::StadiumBuilt { team, amount } => {
                owner_window(self)?;
                let c = self.club_mut(*team)?;
                c.cash -= amount;
                c.stadium.quality = (c.stadium.quality + amount / 600.0).min(QUALITY_MAX);
                c.stadium.capacity = (c.stadium.capacity + 0.05 * amount).min(CAPACITY_MAX);
                Ok(())
            }
            Event::ClubRescued { team } => {
                let floor = self.rules.payroll_floor;
                let c = self.club_mut(*team)?;
                c.cash = 0.1 * c.revenue_prev;
                c.stadium.ticket = crate::state::TICKET_REF;
                c.stance = crate::state::Stance::Balanced;
                c.budget = c.budget.min(floor);
                Ok(())
            }
            Event::StanceSet { team, stance } => {
                owner_window(self)?;
                self.club_mut(*team)?.stance = *stance;
                Ok(())
            }
            Event::RulesSet { rules } => {
                if !matches!(self.phase, Phase::Offseason | Phase::Progressed) {
                    return Err(WorldError::Phase(self.phase));
                }
                for c in &mut self.clubs {
                    c.budget = c.budget.clamp(rules.payroll_floor, rules.payroll_cap);
                }
                self.rules = rules.clone();
                self.rules_set = Some(self.year);
                Ok(())
            }
            Event::TradeProposed {
                offer,
                from,
                to,
                give,
                get,
            } => {
                if !matches!(
                    self.phase,
                    Phase::Progressed | Phase::Drafting | Phase::FreeAgency
                ) {
                    return Err(WorldError::Phase(self.phase));
                }
                if *offer != self.next_offer {
                    return invalid("offer ids must count up");
                }
                if let Err(m) = self.check_trade(*from, *to, give, get) {
                    return invalid(m);
                }
                self.offers.push(crate::state::TradeOffer {
                    id: *offer,
                    from: *from,
                    to: *to,
                    give: give.clone(),
                    get: get.clone(),
                });
                self.next_offer += 1;
                Ok(())
            }
            Event::TradeAnswered { offer, accepted } => {
                if !matches!(
                    self.phase,
                    Phase::Progressed | Phase::Drafting | Phase::FreeAgency
                ) {
                    return Err(WorldError::Phase(self.phase));
                }
                let Some(at) = self.offers.iter().position(|o| o.id == *offer) else {
                    return invalid("no such offer");
                };
                let o = self.offers[at].clone();
                if *accepted {
                    if let Err(m) = self.check_trade(o.from, o.to, &o.give, &o.get) {
                        return invalid(m);
                    }
                    for (ids, club) in [(&o.give, o.to), (&o.get, o.from)] {
                        for id in ids {
                            let old = self.player_mut(*id)?.team;
                            if let Some(old) = old {
                                self.club_mut(old)?.players.remove(id);
                            }
                            self.club_mut(club)?.players.insert(*id);
                            self.player_mut(*id)?.team = Some(club);
                        }
                    }
                }
                self.offers.remove(at);
                Ok(())
            }
            Event::TacticsSet { team, tactics } => {
                if self.phase == Phase::Unfounded {
                    return Err(WorldError::Phase(self.phase));
                }
                self.club_mut(*team)?.tactics = *tactics;
                Ok(())
            }
            Event::PlayersProgressed { year, updates, era } => {
                expect_phase(self, Phase::Offseason)?;
                if *year != self.year + 1 {
                    return invalid("progression must advance exactly one year");
                }
                self.era = *era;
                self.season = None;
                self.year = *year;
                for u in updates {
                    let id = u.player;
                    let (team, was_fa) = {
                        let p = self.player_mut(id)?;
                        p.drift = u.drift;
                        p.z = u.z;
                        p.quality = u.quality;
                        p.contract = u.contract;
                        p.out_games = u.out_games;
                        p.pending_loss = 0.0;
                        (p.team, p.team.is_none())
                    };
                    match u.outcome {
                        Outcome::Stays => {
                            if was_fa {
                                self.player_mut(id)?.free_years += 1;
                            }
                        }
                        Outcome::Expired => {
                            let Some(t) = team else {
                                return invalid("a free agent cannot expire");
                            };
                            self.club_mut(t)?.players.remove(&id);
                            let p = self.player_mut(id)?;
                            p.team = None;
                            p.contract = None;
                            p.free_years = 0;
                            self.free_agents.insert(id);
                        }
                        Outcome::Retired => {
                            if let Some(t) = team {
                                self.club_mut(t)?.players.remove(&id);
                            }
                            self.free_agents.remove(&id);
                            let mut p = self
                                .players
                                .remove(&id)
                                .ok_or(WorldError::UnknownPlayer(id))?;
                            p.team = None;
                            p.contract = None;
                            self.retired.insert(id, (p, *year));
                        }
                    }
                }
                self.phase = Phase::Progressed;
                Ok(())
            }
            Event::DraftOpened {
                year,
                prospects,
                order,
                amateur_picks,
                next_player,
            } => {
                expect_phase(self, Phase::Progressed)?;
                if *year != self.year {
                    return invalid("draft year does not match");
                }
                let mut remaining = std::collections::BTreeSet::new();
                for p in prospects {
                    remaining.insert(p.id);
                    self.players.insert(p.id, p.clone());
                }
                self.next_player = *next_player;
                self.draft = Some(Draft {
                    year: *year,
                    order: order.clone(),
                    amateur_picks: *amateur_picks,
                    next: 0,
                    remaining,
                });
                self.phase = Phase::Drafting;
                Ok(())
            }
            Event::PlayerDrafted {
                pick,
                team,
                player,
                bonus,
            } => {
                expect_phase(self, Phase::Drafting)?;
                let year = self.year;
                let draft = self
                    .draft
                    .as_mut()
                    .ok_or(WorldError::Phase(Phase::Drafting))?;
                if draft.next != *pick || draft.order.get(*pick) != Some(team) {
                    return invalid("not this club's pick");
                }
                if !draft.remaining.remove(player) {
                    return invalid("player is not in the draft");
                }
                draft.next += 1;
                let pick_no = (*pick + 1) as u16;
                match bonus {
                    Some(b) => {
                        {
                            let p = self.player_mut(*player)?;
                            p.drafted = Some(crate::player::Drafted {
                                year,
                                pick: pick_no,
                            });
                            p.team = Some(*team);
                            p.level = Level::Single;
                            p.contract = Some(Contract {
                                salary: MINOR_COST,
                                years: 6,
                            });
                        }
                        let c = self.club_mut(*team)?;
                        c.players.insert(*player);
                        c.cash -= *b;
                        c.spent += *b;
                    }
                    None => {
                        let mut p = self
                            .players
                            .remove(player)
                            .ok_or(WorldError::UnknownPlayer(*player))?;
                        p.drafted = Some(crate::player::Drafted {
                            year,
                            pick: pick_no,
                        });
                        self.retired.insert(*player, (p, year));
                    }
                }
                Ok(())
            }
            Event::PickPassed { pick, team } => {
                expect_phase(self, Phase::Drafting)?;
                let draft = self
                    .draft
                    .as_mut()
                    .ok_or(WorldError::Phase(Phase::Drafting))?;
                if draft.next != *pick || draft.order.get(*pick) != Some(team) {
                    return invalid("not this club's pick");
                }
                draft.next += 1;
                Ok(())
            }
            Event::DraftClosed => {
                expect_phase(self, Phase::Drafting)?;
                let draft = self
                    .draft
                    .take()
                    .ok_or(WorldError::Phase(Phase::Drafting))?;
                for id in draft.remaining {
                    self.players.remove(&id);
                }
                self.phase = Phase::FreeAgency;
                Ok(())
            }
            Event::FreeAgentSigned {
                team,
                player,
                contract,
            } => {
                expect_phase(self, Phase::FreeAgency)?;
                if !self.free_agents.remove(player) {
                    return invalid("player is not a free agent");
                }
                let kind = self.player_mut(*player)?.kind;
                let held = self.kind_count(*team, kind, None);
                if held >= ROSTER_MAX_PER_KIND {
                    self.free_agents.insert(*player);
                    return invalid("club is at the roster limit");
                }
                self.club_mut(*team)?.players.insert(*player);
                let p = self.player_mut(*player)?;
                p.team = Some(*team);
                p.contract = Some(*contract);
                p.level = Level::Triple;
                p.free_years = 0;
                Ok(())
            }
            Event::PlayerReleased { team, player } => {
                if !matches!(
                    self.phase,
                    Phase::FreeAgency | Phase::Drafting | Phase::Progressed
                ) {
                    return Err(WorldError::Phase(self.phase));
                }
                if self.player_mut(*player)?.team != Some(*team) {
                    return invalid("player is not on that club");
                }
                // The club still owes the rest of the contract.
                let owed = self
                    .player_mut(*player)?
                    .contract
                    .map_or(0.0, |c| c.salary * f64::from(c.years));
                let club = self.club_mut(*team)?;
                club.players.remove(player);
                club.cash -= owed;
                club.spent += owed;
                let p = self.player_mut(*player)?;
                p.team = None;
                p.contract = None;
                p.free_years = 0;
                self.free_agents.insert(*player);
                Ok(())
            }
            Event::LevelSet {
                team,
                player,
                level,
            } => {
                if !matches!(
                    self.phase,
                    Phase::FreeAgency | Phase::Drafting | Phase::Progressed
                ) {
                    return Err(WorldError::Phase(self.phase));
                }
                let p = self.player_mut(*player)?;
                if p.team != Some(*team) {
                    return invalid("player is not on that club");
                }
                p.level = *level;
                Ok(())
            }
            Event::SeasonStarted { year } => {
                expect_phase(self, Phase::FreeAgency)?;
                if *year != self.year {
                    return invalid("wrong year");
                }
                for t in 0..self.clubs.len() as u8 {
                    for kind in [crate::rating::Kind::Batter, crate::rating::Kind::Pitcher] {
                        let n = self.kind_count(t, kind, Some(Level::Majors));
                        if n != MAJORS_PER_KIND {
                            return invalid(format!(
                                "club {t} has {n} {kind:?}s in the majors, needs {MAJORS_PER_KIND}"
                            ));
                        }
                        for level in Level::MINORS {
                            let n = self.kind_count(t, kind, Some(level));
                            if n > level.capacity() {
                                return invalid(format!(
                                    "club {t} has {n} {kind:?}s at {level:?}, room for {}",
                                    level.capacity()
                                ));
                            }
                        }
                        let all = self.kind_count(t, kind, None);
                        if all > ROSTER_PER_KIND {
                            return invalid(format!(
                                "club {t} holds {all} {kind:?}s, limit {ROSTER_PER_KIND}"
                            ));
                        }
                    }
                }
                self.phase = Phase::Regular;
                self.offers.clear();
                Ok(())
            }
        }
    }
}

/// Owners settle money matters in the offseason.
fn owner_window(w: &World) -> Result<(), WorldError> {
    if matches!(
        w.phase,
        Phase::Offseason | Phase::Progressed | Phase::Drafting | Phase::FreeAgency
    ) {
        Ok(())
    } else {
        Err(WorldError::Phase(w.phase))
    }
}

impl World {
    /// A rescued club does not keep the payroll that broke it: the league
    /// voids its dearest contracts, best paid first, until the payroll is
    /// back at the floor. The players go to free agency and the club owes
    /// nothing more on those deals.
    fn receivership(&mut self, team: usize) {
        let floor = self.rules.payroll_floor;
        let mut paid: Vec<(f64, PlayerId)> = self.clubs[team]
            .players
            .iter()
            .filter_map(|id| Some((self.players.get(id)?.contract?.salary, *id)))
            .collect();
        paid.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)));
        let mut payroll: f64 = paid.iter().map(|(s, _)| s).sum();
        for (salary, id) in paid {
            if payroll <= floor {
                break;
            }
            payroll -= salary;
            self.clubs[team].players.remove(&id);
            if let Some(p) = self.players.get_mut(&id) {
                p.team = None;
                p.contract = None;
                p.free_years = 0;
            }
            self.free_agents.insert(id);
        }
    }
}
