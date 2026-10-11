//! Events are the only thing that changes the world.

use crate::command::{ClubFinanceState, Event, Founding, Line, Outcome, WorldError};
use crate::player::{Contract, Level, Player, PlayerId};
use crate::state::{Draft, MAJORS_PER_KIND, MINOR_COST, Phase, SeasonRecord, World};

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

    /// Apply one event. Fails (possibly leaving the world half-changed for
    /// multi-step events) if the event does not fit.
    pub fn apply(&mut self, e: &Event) -> Result<(), WorldError> {
        match e {
            Event::Founded(f) => self.found(f),
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
                    c.hype = hype;
                    c.win_pct_prev = win_pct_prev;
                    self.finances[i].push(fin.clone());
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
            Event::PlayersProgressed { year, updates } => {
                expect_phase(self, Phase::Offseason)?;
                if *year != self.year + 1 {
                    return invalid("progression must advance exactly one year");
                }
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
                for (t, c) in self.clubs.iter().enumerate() {
                    for kind in [crate::rating::Kind::Batter, crate::rating::Kind::Pitcher] {
                        let n = c
                            .players
                            .iter()
                            .filter_map(|id| self.players.get(id))
                            .filter(|p| p.kind == kind && p.level == Level::Majors)
                            .count();
                        if n != MAJORS_PER_KIND {
                            return invalid(format!(
                                "club {t} has {n} {kind:?}s in the majors, needs {MAJORS_PER_KIND}"
                            ));
                        }
                    }
                }
                self.phase = Phase::Regular;
                Ok(())
            }
        }
    }
}
