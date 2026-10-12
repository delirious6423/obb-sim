//! The application shell: one client, the screen on show, the answers
//! received so far, and the requests still in flight.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

use host::api::{
    Action, Advance, Club, DraftView, FinancesView, LeagueView, Overview, OwnerView, PlayerRow,
    PlayerView, Progress, Request, Response, RosterView, SaveInfo, ScheduleView, Standings,
    TradeView,
};
use host::{GameClient, LocalClient, Ticket};

use crate::screens;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    NewGame,
    Home,
    Standings,
    Schedule,
    Roster,
    Lineup,
    Player,
    FreeAgents,
    Trades,
    Draft,
    Finances,
    Owner,
    Commissioner,
    Saves,
    Settings,
    Credits,
}

/// What an in-flight request is for, so its answer lands in the right place.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Tag {
    Overview,
    Standings,
    Schedule(Club),
    BoxScore(u16),
    Roster(Club),
    Player(u32),
    FreeAgents,
    Draft,
    Finances,
    Trades,
    Owner,
    League,
    Saves,
    /// A change to the league (an action, an advance, a save, a load).
    Change,
    /// Starting or loading a game.
    Start,
}

/// The latest answer of each kind.
#[derive(Default)]
pub struct Data {
    pub overview: Option<Overview>,
    pub standings: Option<Standings>,
    pub schedule: BTreeMap<Club, ScheduleView>,
    pub box_score: Option<(u16, Option<String>)>,
    pub rosters: BTreeMap<Club, RosterView>,
    pub player: Option<PlayerView>,
    pub free_agents: Option<Vec<PlayerRow>>,
    pub draft: Option<DraftView>,
    pub finances: Option<FinancesView>,
    pub trades: Option<TradeView>,
    pub owner: Option<OwnerView>,
    pub league: Option<LeagueView>,
    /// The last failed read of the owner's desk or the league office (the
    /// screen shows it instead of a spinner).
    pub owner_error: Option<String>,
    pub league_error: Option<String>,
    pub saves: Option<Vec<SaveInfo>>,
}

/// A line for the status bar.
pub struct Message {
    pub text: String,
    pub error: bool,
}

pub struct App {
    client: LocalClient,
    pending: Vec<(Ticket, Tag)>,
    pub screen: Screen,
    pub started: bool,
    pub data: Data,
    pub message: Option<Message>,
    /// The club whose roster/schedule is on show.
    pub viewing: Club,
    /// The screen the player page's Back button returns to.
    pub returns_to: Screen,
    /// Per-screen state, kept here so a screen survives a tab switch.
    pub state: screens::State,
}

/// The save folder: `BASEBALL_SIM_SAVES` itself, else `~/.baseball-sim/saves`.
fn save_dir() -> Option<PathBuf> {
    let dir = std::env::var_os("BASEBALL_SIM_SAVES")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME")
                .or_else(|| std::env::var_os("USERPROFILE"))
                .map(|h| PathBuf::from(h).join(".baseball-sim").join("saves"))
        })?;
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir)
}

impl App {
    pub fn new() -> Self {
        Self {
            client: LocalClient::start(host::calibration(), save_dir()),
            pending: Vec::new(),
            screen: Screen::NewGame,
            started: false,
            data: Data::default(),
            message: None,
            viewing: 0,
            returns_to: Screen::Roster,
            state: screens::State::default(),
        }
    }

    // ----- asking -----

    pub fn ask(&mut self, req: Request, tag: Tag) {
        // Do not stack identical reads.
        if self.pending.iter().any(|(_, t)| *t == tag) && !matches!(tag, Tag::Change | Tag::Start) {
            return;
        }
        let t = self.client.submit(req);
        self.pending.push((t, tag));
    }

    /// Change the league: an action by the person's club.
    pub fn act(&mut self, a: Action) {
        self.ask(Request::Act(a), Tag::Change);
    }

    pub fn advance(&mut self, a: Advance) {
        self.ask(Request::Advance(a), Tag::Change);
    }

    /// A long request (a change) is running.
    pub fn busy(&self) -> bool {
        self.pending
            .iter()
            .any(|(_, t)| matches!(t, Tag::Change | Tag::Start))
    }

    pub fn progress(&self) -> Progress {
        self.client.progress()
    }

    pub fn cancel(&self) {
        self.client.cancel();
    }

    pub fn go(&mut self, s: Screen) {
        self.screen = s;
        self.fetch();
    }

    /// Ask for what the current screen shows.
    pub fn fetch(&mut self) {
        if !self.started {
            return;
        }
        self.ask(Request::Overview, Tag::Overview);
        let own = self.data.overview.as_ref().map_or(0, |o| o.club);
        match self.screen {
            Screen::Home | Screen::NewGame | Screen::Credits | Screen::Settings => {}
            Screen::Standings => self.ask(Request::Standings, Tag::Standings),
            Screen::Schedule => {
                let c = self.viewing;
                self.ask(Request::Schedule { club: c }, Tag::Schedule(c));
            }
            Screen::Roster => {
                let c = self.viewing;
                self.ask(Request::Roster { club: c }, Tag::Roster(c));
            }
            Screen::Lineup => self.ask(Request::Roster { club: own }, Tag::Roster(own)),
            Screen::Player => {
                if let Some(id) = self.state.player_id {
                    self.ask(Request::Player { id }, Tag::Player(id));
                }
            }
            Screen::FreeAgents => self.ask(Request::FreeAgents, Tag::FreeAgents),
            Screen::Trades => {
                self.ask(Request::Trades, Tag::Trades);
                self.ask(Request::Roster { club: own }, Tag::Roster(own));
            }
            Screen::Draft => self.ask(Request::DraftBoard, Tag::Draft),
            Screen::Finances => self.ask(Request::Finances, Tag::Finances),
            Screen::Owner => {
                self.ask(Request::OwnerDesk, Tag::Owner);
                // The payroll limits the budget slider is bounded by.
                self.ask(Request::Finances, Tag::Finances);
            }
            Screen::Commissioner => self.ask(Request::LeagueOffice, Tag::League),
            Screen::Saves => self.ask(Request::ListSaves, Tag::Saves),
        }
    }

    pub fn open_player(&mut self, id: u32) {
        self.returns_to = opened_from(self.screen, self.returns_to);
        self.state.player_id = Some(id);
        self.go(Screen::Player);
    }

    pub fn own_club(&self) -> Club {
        self.data.overview.as_ref().map_or(0, |o| o.club)
    }

    pub fn say(&mut self, text: impl Into<String>, error: bool) {
        self.message = Some(Message {
            text: text.into(),
            error,
        });
    }

    // ----- answers -----

    fn poll(&mut self, ctx: &egui::Context) {
        let mut done = Vec::new();
        self.pending.retain(|(t, tag)| match self.client.poll(*t) {
            Some(r) => {
                done.push((tag.clone(), r));
                false
            }
            None => true,
        });
        let mut changed = false;
        for (tag, r) in done {
            match (tag, r) {
                (tag, Response::Failed(e)) => {
                    if e.starts_with("no game in progress") || e.contains("league was closed") {
                        self.started = false;
                        self.screen = Screen::NewGame;
                        self.data = Data::default();
                    }
                    // Give a read that failed an empty answer, so no screen
                    // waits on it for ever.
                    match tag {
                        Tag::Saves => self.data.saves = Some(Vec::new()),
                        Tag::BoxScore(g) => self.data.box_score = Some((g, None)),
                        Tag::Owner => self.data.owner_error = Some(e.clone()),
                        Tag::League => self.data.league_error = Some(e.clone()),
                        _ => {}
                    }
                    self.say(e, true);
                }
                (Tag::Start, Response::Done(m)) => {
                    self.started = true;
                    self.data = Data::default();
                    self.state.player_id = None;
                    self.state.game_open = None;
                    self.state.level_filter = None;
                    self.state.trade_give.clear();
                    self.state.trade_get.clear();
                    self.state.rules = None;
                    self.state.owner_budget = None;
                    self.state.owner_ticket = None;
                    self.say(m, false);
                    self.viewing = 0;
                    self.screen = Screen::Home;
                    changed = true;
                }
                (Tag::Change, Response::Done(m)) => {
                    // Players may have moved: drop selections and old box scores.
                    self.state.trade_give.clear();
                    self.state.trade_get.clear();
                    self.data.box_score = None;
                    // The views are read again, so edits in progress are stale.
                    self.state.rules = None;
                    self.state.owner_budget = None;
                    self.state.owner_ticket = None;
                    if !m.is_empty() {
                        self.say(m, false);
                    }
                    changed = true;
                }
                (_, Response::Overview(o)) => {
                    if self.data.overview.is_none() {
                        self.viewing = o.club;
                    }
                    self.data.overview = Some(o);
                }
                (_, Response::Standings(s)) => self.data.standings = Some(s),
                (_, Response::Schedule(s)) => {
                    self.data.schedule.insert(s.club, s);
                }
                (Tag::BoxScore(g), Response::BoxScore(b)) => self.data.box_score = Some((g, b)),
                (_, Response::Roster(r)) => {
                    self.data.rosters.insert(r.club, r);
                }
                (_, Response::Player(p)) => self.data.player = Some(p),
                (_, Response::Players(p)) => self.data.free_agents = Some(p),
                (_, Response::Draft(d)) => self.data.draft = Some(d),
                (_, Response::Finances(f)) => self.data.finances = Some(f),
                (_, Response::Trades(t)) => self.data.trades = Some(t),
                (_, Response::Owner(o)) => {
                    self.data.owner = Some(o);
                    self.data.owner_error = None;
                }
                (_, Response::League(l)) => {
                    self.data.league = Some(l);
                    self.data.league_error = None;
                }
                (_, Response::Saves(s)) => self.data.saves = Some(s),
                _ => {}
            }
        }
        if changed {
            self.fetch();
        }
        if !self.pending.is_empty() {
            ctx.request_repaint_after(Duration::from_millis(40));
        }
    }
}

impl eframe::App for App {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll(ctx);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        screens::shell(self, ui);
    }
}

/// Where the player page returns to. Opening a player from another screen
/// remembers that screen; opening one from the player page keeps the first.
#[must_use]
pub fn opened_from(current: Screen, origin: Screen) -> Screen {
    if current == Screen::Player {
        origin
    } else {
        current
    }
}

#[cfg(test)]
mod tests {
    use super::{Screen, opened_from};

    #[test]
    fn back_returns_to_the_screen_the_player_was_opened_from() {
        assert_eq!(
            opened_from(Screen::FreeAgents, Screen::Roster),
            Screen::FreeAgents
        );
        assert_eq!(opened_from(Screen::Roster, Screen::Home), Screen::Roster);
        assert_eq!(opened_from(Screen::Player, Screen::Trades), Screen::Trades);
    }
}
