//! A league being played by one person (general manager of one club) with
//! every other seat filled by the built-in AI.

use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use calibration::Calibration;
use connect::{
    BuiltInCommissioner, BuiltInGm, BuiltInManager, BuiltInOwner, GmAction, GmAgent, GmStage,
    GmTurn, Seats, commissioner_step, draft_turn, free_agency_round, roster_stage, set_tactics,
    start_season, start_with, team_actor, trade_round,
};
use gm::SurplusGm;
use protocol::Actor;
use storage::{LocalStore, Manifest, SaveData, SaveKind, SaveStore, SlotId};
use world::agents::GmView;
use world::command::{Command, Event};
use world::player::{DEFAULT_POOL, PlayerId};
use world::rating::Model;
use world::state::{Phase, TEAMS, Tactics, World};

use crate::api::{
    Action, Advance, Difficulty, NewGame, Overview, Preset, Progress, Request, Response, Roles,
    SaveInfo, Stage,
};
use crate::{difficulty, views};

const LEAGUE_NAME: &str = "The Fictional League";
const ENGINE_VERSION: &str = concat!("sim ", env!("CARGO_PKG_VERSION"), " / rules 1");
/// Days played per step of a long simulation: progress is reported and
/// cancellation is checked between steps.
const DAYS_PER_STEP: u16 = 2;

/// Shared between the host and whoever watches it: how far a long request
/// has got, and a request to stop.
#[derive(Debug, Default)]
pub struct Control {
    pub progress: Mutex<Progress>,
    pub cancel: AtomicBool,
}

impl Control {
    fn start(&self, label: &str, total: u32) {
        self.cancel.store(false, Ordering::SeqCst);
        if let Ok(mut p) = self.progress.lock() {
            *p = Progress {
                running: true,
                label: label.into(),
                done: 0,
                total,
            };
        }
    }

    fn step(&self, done: u32) {
        if let Ok(mut p) = self.progress.lock() {
            p.done = done;
        }
    }

    fn finish(&self) {
        if let Ok(mut p) = self.progress.lock() {
            p.running = false;
            p.done = p.total;
        }
    }

    fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::SeqCst)
    }
}

pub struct Host {
    cal: Calibration,
    world: World,
    seats: Seats,
    /// The club the person runs.
    club: u8,
    /// Which seats the person holds (the rest are AI).
    roles: Roles,
    difficulty: Difficulty,
    smart_tactics: bool,
    store: Option<LocalStore>,
    notices: Vec<String>,
    pub control: Arc<Control>,
}

fn ai_seats(w_seed: u64, club: u8, d: &Difficulty, burn_in: u8, roles: Roles) -> Seats {
    let profile = difficulty::profile(d);
    Seats::fill(
        Box::new(BuiltInCommissioner {
            burn_in_years: burn_in,
        }),
        |_| Box::new(BuiltInOwner),
        |t| -> Box<dyn GmAgent> {
            if t == club && roles.gm {
                // The person decides; this seat is skipped by every round.
                Box::new(BuiltInGm)
            } else {
                Box::new(SurplusGm::new(w_seed ^ (u64::from(t) << 32), profile))
            }
        },
        |t| {
            Box::new(BuiltInManager {
                tactics: if t == club {
                    Tactics::Baseline
                } else {
                    Tactics::Tables
                },
            })
        },
    )
}

fn gm_actor(club: u8) -> Actor {
    team_actor(Actor::gm, club)
}

/// The title the person holds, for the save list and the welcome notice.
fn role_title(r: &Roles) -> String {
    let mut parts = Vec::new();
    if r.gm {
        parts.push("General Manager");
    }
    if r.owner {
        parts.push("Owner");
    }
    if r.commissioner {
        parts.push("Commissioner");
    }
    parts.join(" and ")
}

impl Host {
    /// Found a new league.
    ///
    /// # Errors
    /// If the settings are not valid or founding fails.
    pub fn new_game(cal: &Calibration, g: &NewGame) -> Result<Self, String> {
        if usize::from(g.club) >= TEAMS {
            return Err("no such club".into());
        }
        if !g.roles.any() {
            return Err("the person must hold at least one role".into());
        }
        let model = Model::from_calibration(cal);
        let mut world = World::new(g.seed, cal.params, model, DEFAULT_POOL);
        let mut seats = ai_seats(g.seed, g.club, &g.difficulty, g.burn_in_years, g.roles);
        let mut log = Vec::new();
        start_with(&mut world, &mut seats, &mut log).map_err(|e| e.to_string())?;
        let mut h = Self {
            cal: cal.clone(),
            world,
            seats,
            club: g.club,
            roles: g.roles,
            difficulty: g.difficulty.clone(),
            smart_tactics: false,
            store: None,
            notices: Vec::new(),
            control: Arc::default(),
        };
        h.notices.push(format!(
            "Welcome. You run the {} as {}.",
            h.world.clubs[usize::from(g.club)].name,
            role_title(&g.roles)
        ));
        Ok(h)
    }

    /// The club the general manager seat skips in the league-wide rounds.
    /// Without that seat the AI runs the club too.
    fn skip(&self) -> Option<u8> {
        self.roles.gm.then_some(self.club)
    }

    /// Keep saves in this folder.
    ///
    /// # Errors
    /// If the folder cannot be used.
    pub fn with_saves(mut self, dir: impl AsRef<std::path::Path>) -> Result<Self, String> {
        self.store = Some(LocalStore::open(dir).map_err(|e| e.to_string())?);
        Ok(self)
    }

    /// Use another folder for saves (kept when a game is loaded).
    pub fn set_saves(&mut self, store: Option<LocalStore>) {
        self.store = store;
    }

    #[must_use]
    pub fn take_saves(&mut self) -> Option<LocalStore> {
        self.store.take()
    }

    fn stage(&self) -> Stage {
        match self.world.phase {
            Phase::Unfounded | Phase::Regular => Stage::Season,
            Phase::Playoffs => Stage::Playoffs,
            Phase::Settling | Phase::Offseason | Phase::Progressed => Stage::Offseason,
            Phase::Drafting => Stage::Draft,
            Phase::FreeAgency => Stage::FreeAgency,
        }
    }

    fn days(&self) -> u16 {
        season::Schedule::generate(self.world.seed, self.world.year as u32, 5)
            .days
            .len() as u16
    }

    fn date_text(&self) -> String {
        match self.stage() {
            Stage::Season => format!(
                "{} · day {}/{}",
                self.world.year,
                self.world.season.as_ref().map_or(0, |s| s.day),
                self.days()
            ),
            s => format!("{} · {s:?}", self.world.year),
        }
    }

    /// Answer one request. `ctl` reports progress and carries a cancel flag
    /// for the long ones.
    pub fn handle(&mut self, req: Request) -> Response {
        let me = self.club;
        let w = &self.world;
        match req {
            Request::NewGame(g) => match Host::new_game(&self.cal, &g) {
                Ok(mut h) => {
                    h.store = self.store.take();
                    h.control = Arc::clone(&self.control);
                    *self = h;
                    Response::Done("New league founded".into())
                }
                Err(e) => Response::Failed(e),
            },
            Request::Overview => Response::Overview(self.overview()),
            Request::Standings => Response::Standings(views::standings(w)),
            Request::Schedule { club } => {
                if usize::from(club) >= TEAMS {
                    return Response::Failed("no such club".into());
                }
                Response::Schedule(views::schedule(w, club))
            }
            Request::BoxScore { game } => Response::BoxScore(w.box_score(game)),
            Request::Roster { club } => {
                if usize::from(club) >= TEAMS {
                    return Response::Failed("no such club".into());
                }
                Response::Roster(views::roster(w, me, club))
            }
            Request::Player { id } => match views::player(w, me, id) {
                Some(p) => Response::Player(p),
                None => Response::Failed("no such player".into()),
            },
            Request::FreeAgents => {
                let mut rows = views::rows(w, me, w.free_agents.iter().copied());
                rows.sort_by(|a, b| b.grade_now.cmp(&a.grade_now).then(a.id.cmp(&b.id)));
                Response::Players(rows)
            }
            Request::DraftBoard => Response::Draft(views::draft(w, me)),
            Request::Finances => Response::Finances(views::finances(w, me)),
            Request::OwnerDesk => {
                if !self.roles.owner {
                    return Response::Failed("you do not hold the owner role".into());
                }
                let can_act = matches!(
                    w.phase,
                    Phase::Offseason | Phase::Progressed | Phase::Drafting | Phase::FreeAgency
                );
                Response::Owner(views::owner_desk(w, me, self.stage(), can_act))
            }
            Request::LeagueOffice => {
                if !self.roles.commissioner {
                    return Response::Failed("you do not hold the commissioner role".into());
                }
                let can_act = matches!(w.phase, Phase::Offseason | Phase::Progressed);
                Response::League(views::league_office(w, self.stage(), can_act))
            }
            Request::Trades => Response::Trades(views::trades(w, me)),
            Request::Act(a) => self.act(a),
            Request::Advance(a) => self.advance(a),
            Request::Save { slot } => self.save(&slot, SaveKind::Manual),
            Request::QuickSave => self.save("quick", SaveKind::Quick),
            Request::Load { slot } => self.load(&slot),
            Request::ListSaves => self.list_saves(),
            Request::ExportSave { slot, folder } => {
                export_save(self.store.as_ref(), &slot, &folder)
            }
            Request::ImportSave { path, as_name } => {
                import_save(self.store.as_ref(), &path, as_name.as_deref())
            }
            Request::DeleteSave { slot } => {
                let Some(store) = &self.store else {
                    return Response::Failed("no save folder".into());
                };
                match SlotId::new(slot).and_then(|s| store.delete(&s)) {
                    Ok(()) => Response::Done("Deleted".into()),
                    Err(e) => Response::Failed(e.to_string()),
                }
            }
        }
    }

    fn overview(&mut self) -> Overview {
        let w = &self.world;
        let me = usize::from(self.club);
        let st = w.season.as_ref();
        let rec = st
            .map(|s| s.records[me])
            .or_else(|| w.last_records.get(me).copied())
            .unwrap_or_default();
        let standings = views::standings(w);
        let place = standings
            .divisions
            .iter()
            .find_map(|d| {
                d.rows
                    .iter()
                    .position(|r| usize::from(r.club) == me)
                    .map(|i| format!("{} in {}", ordinal(i + 1), d.name))
            })
            .unwrap_or_default();
        let (cash, budget) = (w.clubs[me].cash, w.clubs[me].budget);
        let on_clock = w
            .draft
            .as_ref()
            .is_some_and(|d| d.order.get(d.next) == Some(&self.club));
        let mut advance = Vec::new();
        match self.stage() {
            Stage::Season => {
                if w.phase == Phase::Regular {
                    advance.push((Advance::SimDays(1), "Play one day".to_string()));
                    advance.push((Advance::SimDays(7), "Play one week".to_string()));
                    advance.push((Advance::SimDays(30), "Play one month".to_string()));
                    advance.push((
                        Advance::SimToEndOfSeason,
                        "Play to the end of the season".to_string(),
                    ));
                }
            }
            Stage::Playoffs | Stage::Offseason => {
                let label = if w.phase == Phase::Offseason && self.office_open() {
                    "Close the books, then on to the draft"
                } else {
                    "Play the playoffs, then on to the draft"
                };
                advance.push((Advance::FinishSeason, label.to_string()));
            }
            Stage::Draft => {
                advance.push((Advance::SimDraft, "Let the other clubs pick".to_string()));
            }
            Stage::FreeAgency => {
                advance.push((
                    Advance::TradeRound,
                    "Other clubs hold trade talks".to_string(),
                ));
                advance.push((
                    Advance::FreeAgencyRound,
                    "Other clubs sign free agents".to_string(),
                ));
                advance.push((
                    Advance::StartNextSeason,
                    "Start the next season".to_string(),
                ));
            }
        }
        Overview {
            league: LEAGUE_NAME.into(),
            year: w.year,
            stage: self.stage(),
            club: self.club,
            club_name: w.clubs[me].name.clone(),
            roles: self.roles,
            day: st.map_or(0, |s| s.day),
            days: if self.stage() == Stage::Season {
                self.days()
            } else {
                0
            },
            record: (rec.w, rec.l),
            place,
            advance,
            on_the_clock: on_clock,
            open_offers: w.offers.iter().filter(|o| o.to == self.club).count(),
            cash,
            payroll: w.payroll(me),
            budget,
            difficulty: self.difficulty.label(),
            notices: std::mem::take(&mut self.notices),
        }
    }

    /// Owner actions: refused unless the owner seat is held.
    fn owner_act(&mut self, cmd: Command) -> Response {
        if !self.roles.owner {
            return Response::Failed("you do not hold the owner role".into());
        }
        let actor = team_actor(Actor::owner, self.club);
        match self.world.execute(&actor, cmd) {
            Ok(_) => Response::Done(String::new()),
            Err(e) => Response::Failed(e.to_string()),
        }
    }

    /// Commissioner actions: refused unless the commissioner seat is held.
    fn commissioner_act(&mut self, cmd: Command) -> Response {
        if !self.roles.commissioner {
            return Response::Failed("you do not hold the commissioner role".into());
        }
        match self.commissioner(cmd) {
            Ok(_) => Response::Done(String::new()),
            Err(e) => Response::Failed(e),
        }
    }

    fn act(&mut self, a: Action) -> Response {
        let me = self.club;
        let a = match a {
            Action::SetBudget { payroll } => {
                return self.owner_act(Command::SetBudget { team: me, payroll });
            }
            Action::SetTicketPrice { price } => {
                return self.owner_act(Command::SetTicketPrice { team: me, price });
            }
            Action::BuildStadium { amount } => {
                return self.owner_act(Command::BuildStadium { team: me, amount });
            }
            Action::SetStance { stance } => {
                return self.owner_act(Command::SetStance {
                    team: me,
                    stance: views::stance_world(stance),
                });
            }
            Action::SetRules { rules } => {
                return self.commissioner_act(Command::SetRules {
                    rules: views::rules_world(&rules),
                });
            }
            other => other,
        };
        // Every other action is the general manager's (tactics included: the
        // person's field manager is part of that seat).
        if !self.roles.gm {
            return Response::Failed("you do not hold the general manager role".into());
        }
        let cmd = match a {
            Action::SetLevel { player, level } => Command::SetLevel {
                team: me,
                player: PlayerId(player),
                level: views::wlevel(level),
            },
            Action::Release { player } => Command::ReleasePlayer {
                team: me,
                player: PlayerId(player),
            },
            Action::Sign {
                player,
                salary,
                years,
            } => Command::SignFreeAgent {
                team: me,
                player: PlayerId(player),
                salary,
                years,
            },
            Action::Draft { player } => Command::DraftPlayer {
                team: me,
                player: PlayerId(player),
            },
            Action::PassPick => Command::PassPick { team: me },
            Action::ProposeTrade { partner, give, get } => Command::ProposeTrade {
                team: me,
                partner,
                give: give.into_iter().map(PlayerId).collect(),
                get: get.into_iter().map(PlayerId).collect(),
            },
            Action::RespondTrade { offer, accept } => Command::RespondTrade {
                team: me,
                offer,
                accept,
            },
            Action::SetTactics { smart } => {
                self.smart_tactics = smart;
                self.seats.managers[usize::from(me)] = Box::new(BuiltInManager {
                    tactics: if smart {
                        Tactics::Tables
                    } else {
                        Tactics::Baseline
                    },
                });
                Command::SetTactics {
                    team: me,
                    tactics: if smart {
                        Tactics::Tables
                    } else {
                        Tactics::Baseline
                    },
                }
            }
            Action::AutoRoster => return self.auto_roster(),
            Action::SetBudget { .. }
            | Action::SetTicketPrice { .. }
            | Action::BuildStadium { .. }
            | Action::SetStance { .. }
            | Action::SetRules { .. } => unreachable!("handled above"),
        };
        let actor = match &cmd {
            Command::SetTactics { .. } => team_actor(Actor::field_manager, me),
            _ => gm_actor(me),
        };
        match self.world.execute(&actor, cmd) {
            Ok(_) => Response::Done(String::new()),
            Err(e) => Response::Failed(e.to_string()),
        }
    }

    /// The club's staff put the roster in order.
    fn auto_roster(&mut self) -> Response {
        let me = self.club;
        let turn = GmTurn {
            stage: GmStage::Roster,
            obs: GmView::new(&self.world, me).observe(),
        };
        let mut done = 0;
        for a in BuiltInGm.act(&turn) {
            if matches!(a, GmAction::Release { .. }) {
                // Releasing costs money; the person decides who goes.
                continue;
            }
            if self.world.execute(&gm_actor(me), a.command(me)).is_ok() {
                done += 1;
            }
        }
        Response::Done(format!("{done} roster moves made"))
    }

    fn advance(&mut self, a: Advance) -> Response {
        let ctl = Arc::clone(&self.control);
        let result = match a {
            Advance::SimDays(n) => self.sim_days(n, &ctl),
            Advance::SimToEndOfSeason => self.sim_days(u16::MAX, &ctl),
            Advance::FinishSeason => self.finish_season(&ctl),
            Advance::SimDraft => self.sim_draft(&ctl),
            Advance::TradeRound => {
                if self.world.phase != Phase::FreeAgency {
                    Err("trade talks are held in free agency".into())
                } else {
                    let mut log = Vec::new();
                    let skip = self.skip();
                    let acted = trade_round(&mut self.world, &mut self.seats, &mut log, skip);
                    Ok(if acted {
                        "The other clubs traded".to_string()
                    } else {
                        "Nothing happened".to_string()
                    })
                }
            }
            Advance::FreeAgencyRound => {
                if self.world.phase != Phase::FreeAgency {
                    Err("free agency is not open".into())
                } else {
                    let mut log = Vec::new();
                    let skip = self.skip();
                    let signed =
                        free_agency_round(&mut self.world, &mut self.seats, &mut log, skip);
                    Ok(if signed {
                        "Free agents signed".to_string()
                    } else {
                        "Nobody signed".to_string()
                    })
                }
            }
            Advance::StartNextSeason => self.start_next_season(&ctl),
        };
        ctl.finish();
        match result {
            Ok(m) => Response::Done(m),
            Err(e) => Response::Failed(e),
        }
    }

    fn commissioner(&mut self, cmd: Command) -> Result<Vec<Event>, String> {
        self.world
            .execute(&Actor::commissioner(), cmd)
            .map_err(|e| e.to_string())
    }

    fn sim_days(&mut self, days: u16, ctl: &Control) -> Result<String, String> {
        if self.world.phase != Phase::Regular {
            return Err("the regular season is not on".into());
        }
        let total = self.days();
        let start = self.world.season.as_ref().map_or(0, |s| s.day);
        let want = days.min(total.saturating_sub(start));
        ctl.start("Playing the season", u32::from(want));
        let mut played = 0;
        while played < want {
            if ctl.cancelled() {
                return Ok(format!("Stopped after {played} days"));
            }
            let step = DAYS_PER_STEP.min(want - played);
            self.commissioner(Command::PlayDays { days: step })?;
            played += step;
            ctl.step(u32::from(played));
        }
        if self.world.phase == Phase::Playoffs {
            self.autosave();
            Ok("The regular season is over".into())
        } else {
            Ok(format!("Played {played} days"))
        }
    }

    /// Whether the person holds an office with an offseason window (owner
    /// or commissioner). If so, the offseason stops for the person to act.
    fn office_open(&self) -> bool {
        self.roles.owner || self.roles.commissioner
    }

    /// Owner budgets must sit inside the payroll limits. A rule change made
    /// after the owners acted can leave one outside, so clamp them at close.
    fn clamp_budgets(&mut self) -> Result<(), String> {
        let (floor, cap) = (self.world.rules.payroll_floor, self.world.rules.payroll_cap);
        let mut moved = 0;
        for t in 0..TEAMS as u8 {
            let budget = self.world.clubs[usize::from(t)].budget;
            let fit = budget.clamp(floor, cap);
            if fit != budget {
                self.world
                    .execute(
                        &team_actor(Actor::owner, t),
                        Command::SetBudget {
                            team: t,
                            payroll: fit,
                        },
                    )
                    .map_err(|e| e.to_string())?;
                moved += 1;
            }
        }
        if moved > 0 {
            self.notices.push(format!(
                "Payroll budgets for {moved} clubs were brought within the new limits."
            ));
        }
        Ok(())
    }

    fn finish_season(&mut self, ctl: &Control) -> Result<String, String> {
        ctl.start("Closing the season", 5);
        let mut log = Vec::new();
        let mut n = 0;
        loop {
            match self.world.phase {
                Phase::Playoffs | Phase::Progressed => {
                    commissioner_step(&mut self.world, &mut self.seats, &mut log)
                        .map_err(|e| e.to_string())?;
                }
                Phase::Offseason => {
                    // The person's offseason decisions are in; close the books.
                    if self.office_open() {
                        self.clamp_budgets()?;
                    }
                    commissioner_step(&mut self.world, &mut self.seats, &mut log)
                        .map_err(|e| e.to_string())?;
                }
                Phase::Settling => {
                    commissioner_step(&mut self.world, &mut self.seats, &mut log)
                        .map_err(|e| e.to_string())?;
                    // The AI commissioner sets the rules and the AI owners set
                    // budgets, prices and ballpark spending, unless the person
                    // holds that seat.
                    if !self.roles.commissioner {
                        connect::rules_step(&mut self.world, &mut self.seats, &mut log);
                    }
                    for t in 0..TEAMS as u8 {
                        if self.roles.owner && t == self.club {
                            continue;
                        }
                        connect::owner_step(&mut self.world, &mut self.seats, &mut log, t);
                    }
                    if self.office_open() && self.world.phase == Phase::Offseason {
                        n += 1;
                        ctl.step(n);
                        self.notices
                            .push("The season is over. Your offseason decisions are open.".into());
                        return Ok("The offseason is open".into());
                    }
                }
                Phase::Drafting => break,
                _ => return Err("the regular season is not over".into()),
            }
            n += 1;
            ctl.step(n);
        }
        let (year, champ) = views::last_champion(&self.world).unwrap_or_default();
        self.notices.push(format!("{champ} won the {year}"));
        self.notices.push("The draft is open.".into());
        Ok(format!("{champ} are the champions"))
    }

    fn sim_draft(&mut self, ctl: &Control) -> Result<String, String> {
        if self.world.phase != Phase::Drafting {
            return Err("the draft is not on".into());
        }
        let picks = self.world.draft.as_ref().map_or(0, |d| d.order.len());
        ctl.start("The draft", picks as u32);
        let mut log = Vec::new();
        while let Some(d) = &self.world.draft {
            let Some(&team) = d.order.get(d.next) else {
                break;
            };
            if self.skip() == Some(team) {
                return Ok("You are on the clock".into());
            }
            if ctl.cancelled() {
                return Ok("Stopped".into());
            }
            draft_turn(&mut self.world, &mut self.seats, &mut log, team)
                .map_err(|e| e.to_string())?;
            ctl.step(self.world.draft.as_ref().map_or(0, |d| d.next) as u32);
        }
        commissioner_step(&mut self.world, &mut self.seats, &mut log).map_err(|e| e.to_string())?;
        self.notices
            .push("The draft is over. Trades and free agency are open.".into());
        Ok("The draft is over".into())
    }

    fn start_next_season(&mut self, ctl: &Control) -> Result<String, String> {
        if self.world.phase != Phase::FreeAgency {
            return Err("it is not the offseason window".into());
        }
        ctl.start("Starting the season", 4);
        let skip = self.skip();
        let mut log = Vec::new();
        for _ in 0..3 {
            if !trade_round(&mut self.world, &mut self.seats, &mut log, skip) {
                break;
            }
        }
        ctl.step(1);
        while free_agency_round(&mut self.world, &mut self.seats, &mut log, skip) {}
        ctl.step(2);
        roster_stage(&mut self.world, &mut self.seats, &mut log, skip);
        // Without the general manager seat the AI has already set the roster.
        if self.roles.gm && !self.world.roster_problems(self.club).is_empty() {
            self.auto_roster();
            let mut problems = self.world.roster_problems(self.club);
            if !problems.is_empty() {
                // Staff could not fix it without releasing players; the
                // person must (sign or release) before the season can start.
                let first = problems.remove(0);
                return Err(format!("Your roster is not in order: {first}"));
            }
            self.notices
                .push("Your roster was set by your staff.".into());
        }
        ctl.step(3);
        set_tactics(&mut self.world, &mut self.seats, &mut log);
        start_season(&mut self.world, &mut self.seats, &mut log).map_err(|e| e.to_string())?;
        let t = self.club;
        let tactics = if self.smart_tactics {
            Tactics::Tables
        } else {
            Tactics::Baseline
        };
        if self.world.clubs[usize::from(t)].tactics != tactics {
            let _ = self.world.execute(
                &team_actor(Actor::field_manager, t),
                Command::SetTactics { team: t, tactics },
            );
        }
        self.autosave();
        self.notices
            .push(format!("The {} season begins.", self.world.year));
        Ok(format!("The {} season begins", self.world.year))
    }

    // ---- saves ----------------------------------------------------------

    fn manifest(&self, kind: SaveKind) -> Manifest {
        Manifest {
            format_version: storage::FORMAT_VERSION,
            engine_version: ENGINE_VERSION.into(),
            league_name: LEAGUE_NAME.into(),
            team_name: self.world.clubs[usize::from(self.club)].name.clone(),
            role: role_title(&self.roles),
            in_game_date: self.date_text(),
            difficulty: self.difficulty.label(),
            saved_at_unix: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs()),
            kind,
            thumbnail_png: None,
            checksum_hex: String::new(),
        }
    }

    fn save_data(&self) -> Result<SaveData, String> {
        let snapshot = serde_json::to_vec(&self.world.snapshot()).map_err(|e| e.to_string())?;
        let mut data = SaveData {
            snapshot,
            snapshot_seq: 0,
            log: Vec::new(),
            results: Vec::new(),
            meta: Default::default(),
        };
        data.meta.insert("club".into(), self.club.to_string());
        data.meta.insert(
            "difficulty".into(),
            serde_json::to_string(&self.difficulty).map_err(|e| e.to_string())?,
        );
        data.meta
            .insert("smart_tactics".into(), self.smart_tactics.to_string());
        data.meta.insert(
            "roles".into(),
            serde_json::to_string(&self.roles).map_err(|e| e.to_string())?,
        );
        data.meta.insert("data".into(), DATA_TAG.into());
        Ok(data)
    }

    fn save(&mut self, slot: &str, kind: SaveKind) -> Response {
        let Some(store) = &self.store else {
            return Response::Failed("no save folder".into());
        };
        let result = (|| -> Result<(), String> {
            let id = SlotId::new(slot).map_err(|e| e.to_string())?;
            let data = self.save_data()?;
            store
                .save(&id, self.manifest(kind), &data)
                .map_err(|e| e.to_string())
        })();
        match result {
            Ok(()) => Response::Done(format!("Saved to {slot}")),
            Err(e) => Response::Failed(e),
        }
    }

    /// Save to the rotating autosave slots; a failure is only a notice.
    fn autosave(&mut self) {
        let Some(store) = &self.store else {
            return;
        };
        let r = self.save_data().and_then(|d| {
            store
                .autosave(self.manifest(SaveKind::Auto), &d)
                .map_err(|e| e.to_string())
        });
        if let Err(e) = r {
            self.notices.push(format!("Autosave failed: {e}"));
        }
    }

    fn load(&mut self, slot: &str) -> Response {
        let Some(store) = self.store.take() else {
            return Response::Failed("no save folder".into());
        };
        match Host::from_save(&self.cal, &store, slot) {
            Ok(mut h) => {
                h.store = Some(store);
                h.control = Arc::clone(&self.control);
                *self = h;
                Response::Done("Loaded".into())
            }
            Err(e) => {
                self.store = Some(store);
                Response::Failed(e)
            }
        }
    }

    /// Load a saved league. Loaded data is never trusted: the snapshot is
    /// checked before the league is rebuilt.
    ///
    /// # Errors
    /// If the slot is missing, damaged beyond its backup, from a newer
    /// version, or does not hold a consistent league.
    pub fn from_save(cal: &Calibration, store: &LocalStore, slot: &str) -> Result<Self, String> {
        let loaded = SlotId::new(slot)
            .and_then(|id| store.load(&id))
            .map_err(|e| e.to_string())?;
        let snap: world::state::Snapshot =
            serde_json::from_slice(&loaded.data.snapshot).map_err(|e| e.to_string())?;
        let meta = parse_meta(&loaded.data.meta)?;
        let Meta {
            club,
            difficulty,
            smart,
            roles,
        } = meta;
        let seed = snap.seed;
        let model = Model::from_calibration(cal);
        let world = World::restore(cal.params, model, snap).map_err(|e| e.to_string())?;
        let mut seats = ai_seats(seed, club, &difficulty, 0, roles);
        seats.managers[usize::from(club)] = Box::new(BuiltInManager {
            tactics: if smart {
                Tactics::Tables
            } else {
                Tactics::Baseline
            },
        });
        let mut notices = vec!["Game loaded.".to_string()];
        if loaded.used_backup {
            notices.push("The save was damaged; the previous copy was loaded.".into());
        }
        Ok(Host {
            cal: cal.clone(),
            world,
            seats,
            club,
            roles,
            difficulty,
            smart_tactics: smart,
            store: None,
            notices,
            control: Arc::default(),
        })
    }

    fn list_saves(&self) -> Response {
        let Some(store) = &self.store else {
            return Response::Saves(Vec::new());
        };
        saves_of(store)
    }
}

/// Every save in a folder, for a load screen.
pub(crate) fn saves_of(store: &LocalStore) -> Response {
    match store.list() {
        Ok(list) => Response::Saves(
            list.into_iter()
                .map(|s| {
                    let m = s.manifest;
                    SaveInfo {
                        slot: s.id.as_str().to_string(),
                        league: m
                            .as_ref()
                            .map(|m| m.league_name.clone())
                            .unwrap_or_default(),
                        club_name: m.as_ref().map(|m| m.team_name.clone()).unwrap_or_default(),
                        role: m.as_ref().map(|m| m.role.clone()).unwrap_or_default(),
                        date: m
                            .as_ref()
                            .map(|m| m.in_game_date.clone())
                            .unwrap_or_default(),
                        difficulty: m.as_ref().map(|m| m.difficulty.clone()).unwrap_or_default(),
                        saved_at_unix: m.as_ref().map_or(0, |m| m.saved_at_unix),
                        kind: m
                            .as_ref()
                            .map(|m| format!("{:?}", m.kind))
                            .unwrap_or_default(),
                        problem: s.problem,
                    }
                })
                .collect(),
        ),
        Err(e) => Response::Failed(e.to_string()),
    }
}

/// The calibration data tag this build writes and accepts.
const DATA_TAG: &str = "params-1";

/// The settings a save must carry to be played.
struct Meta {
    club: u8,
    difficulty: Difficulty,
    smart: bool,
    roles: Roles,
}

/// Checks the metadata of a save. Used before a save is loaded and before
/// one is imported, so a bad file is refused before it reaches the folder.
fn parse_meta(meta: &std::collections::BTreeMap<String, String>) -> Result<Meta, String> {
    if let Some(data) = meta.get("data")
        && data != DATA_TAG
    {
        return Err(format!(
            "this save was made with calibration data \"{data}\", but this build uses \"{DATA_TAG}\", so it cannot be loaded here"
        ));
    }
    let club: u8 = meta
        .get("club")
        .and_then(|c| c.parse().ok())
        .ok_or("the save has no club")?;
    let difficulty: Difficulty = meta
        .get("difficulty")
        .and_then(|d| serde_json::from_str(d).ok())
        .ok_or("the save has no difficulty")?;
    let smart = meta.get("smart_tactics").is_some_and(|s| s == "true");
    // Saves from before roles existed were general manager games.
    let roles: Roles = match meta.get("roles") {
        Some(r) => serde_json::from_str(r).map_err(|_| "the save has bad roles")?,
        None => Roles::default(),
    };
    if usize::from(club) >= TEAMS {
        return Err("the save names an unknown club".into());
    }
    if !roles.any() {
        return Err("the save names no role for the person".into());
    }
    Ok(Meta {
        club,
        difficulty,
        smart,
        roles,
    })
}

/// Writes one save as `<folder>/<slot>.bsave`. Never overwrites a file.
pub(crate) fn export_save(store: Option<&LocalStore>, slot: &str, folder: &str) -> Response {
    let Some(store) = store else {
        return Response::Failed("no save folder".into());
    };
    let result = SlotId::new(slot).map_err(|e| e.to_string()).and_then(|id| {
        store
            .export_to(&id, std::path::Path::new(folder))
            .map_err(|e| e.to_string())
    });
    match result {
        Ok(path) => Response::Done(format!("Exported to {}", path.display())),
        Err(e) => Response::Failed(e),
    }
}

/// Imports a `.bsave` file as a new save. Everything is checked before the
/// save folder is touched: the file, the calibration tag, the settings and
/// the league snapshot. The slot is never overwritten.
pub(crate) fn import_save(
    store: Option<&LocalStore>,
    path: &str,
    as_name: Option<&str>,
) -> Response {
    let Some(store) = store else {
        return Response::Failed("no save folder".into());
    };
    let result = (|| -> Result<String, String> {
        let opened =
            storage::OpenedBundle::read(std::path::Path::new(path)).map_err(|e| e.to_string())?;
        parse_meta(&opened.data.meta)?;
        serde_json::from_slice::<world::state::Snapshot>(&opened.data.snapshot)
            .map_err(|e| format!("the file does not hold a league: {e}"))?;
        let mut notice = String::new();
        if opened.manifest.engine_version != ENGINE_VERSION {
            notice = format!(
                " Note: it was saved by \"{}\"; this build is \"{ENGINE_VERSION}\", so the rules may differ.",
                opened.manifest.engine_version
            );
        }
        let id = store
            .import_opened(opened, as_name)
            .map_err(|e| e.to_string())?;
        Ok(format!("Imported as {id}.{notice}"))
    })();
    match result {
        Ok(message) => Response::Done(message),
        Err(e) => Response::Failed(e),
    }
}

impl Host {
    /// For tests: the world, read-only.
    #[must_use]
    pub fn world(&self) -> &World {
        &self.world
    }

    /// The preset behind the current difficulty, if it is one.
    #[must_use]
    pub fn preset(&self) -> Option<Preset> {
        match self.difficulty {
            Difficulty::Preset(p) => Some(p),
            Difficulty::Custom(_) => None,
        }
    }
}

fn ordinal(n: usize) -> String {
    let suffix = match n {
        11..=13 => "th",
        _ => match n % 10 {
            1 => "st",
            2 => "nd",
            3 => "rd",
            _ => "th",
        },
    };
    format!("{n}{suffix}")
}
