//! The screens. Each is a module with `pub fn show(app: &mut App, ui: &mut egui::Ui)`
//! that draws from `app.data` and asks for changes through `app.act`,
//! `app.advance` or `app.ask`. Screens never touch the simulation.

use host::api::{Advance, Level, Levers, Preset, Stage};

use crate::app::{App, Screen};
use crate::widgets::{Col, Sort};

mod credits;
mod draft;
mod finances;
mod free_agents;
mod home;
mod lineup;
mod new_game;
mod player;
mod roster;
mod saves;
mod schedule;
mod settings;
mod standings;
mod trades;

/// Per-screen state that must survive switching tabs.
pub struct State {
    pub player_id: Option<u32>,
    pub seed_text: String,
    pub pick_club: u8,
    pub preset: Preset,
    pub custom: bool,
    pub levers: Levers,
    pub burn_in: u8,
    pub clubs: Vec<String>,
    pub roster_sort: Sort,
    pub fa_sort: Sort,
    pub draft_sort: Sort,
    pub fa_filter: String,
    pub game_open: Option<u16>,
    pub save_name: String,
    pub trade_partner: u8,
    pub trade_give: Vec<u32>,
    pub trade_get: Vec<u32>,
    pub level_filter: Option<Level>,
    pub smart_tactics: bool,
    pub contract_sort: Sort,
}

impl Default for State {
    fn default() -> Self {
        Self {
            player_id: None,
            seed_text: "2026".into(),
            pick_club: 0,
            preset: Preset::Veteran,
            custom: false,
            levers: host::levers_of(Preset::Veteran),
            burn_in: 12,
            clubs: host::club_names(),
            roster_sort: Sort::by(Col::Now, true),
            fa_sort: Sort::by(Col::Now, true),
            draft_sort: Sort::by(Col::Peak, true),
            fa_filter: String::new(),
            game_open: None,
            save_name: String::new(),
            trade_partner: 1,
            trade_give: Vec::new(),
            trade_get: Vec::new(),
            level_filter: None,
            smart_tactics: false,
            contract_sort: Sort::by(Col::Salary, true),
        }
    }
}

const TABS: &[(Screen, &str)] = &[
    (Screen::Home, "Home"),
    (Screen::Standings, "Standings"),
    (Screen::Schedule, "Schedule"),
    (Screen::Roster, "Roster"),
    (Screen::Lineup, "Depth chart"),
    (Screen::FreeAgents, "Free agents"),
    (Screen::Trades, "Trades"),
    (Screen::Draft, "Draft"),
    (Screen::Finances, "Finances"),
    (Screen::Saves, "Saves"),
    (Screen::Settings, "Settings"),
    (Screen::Credits, "Credits"),
];

/// The whole window: top bar, sim bar, status line and the screen.
pub fn shell(app: &mut App, ui: &mut egui::Ui) {
    egui::Panel::top("tabs").show(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            if app.started {
                for (s, name) in TABS {
                    if ui.selectable_label(app.screen == *s, *name).clicked() {
                        app.go(*s);
                    }
                }
            } else {
                ui.heading("Baseball Sim");
                if ui
                    .selectable_label(app.screen == Screen::NewGame, "New game")
                    .clicked()
                {
                    app.screen = Screen::NewGame;
                }
                if ui
                    .selectable_label(app.screen == Screen::Saves, "Load a save")
                    .clicked()
                {
                    app.screen = Screen::Saves;
                    app.ask(host::api::Request::ListSaves, crate::app::Tag::Saves);
                }
            }
        });
    });
    egui::Panel::bottom("status").show(ui, |ui| {
        sim_bar(app, ui);
    });
    egui::CentralPanel::default().show(ui, |ui| match app.screen {
        Screen::NewGame => new_game::show(app, ui),
        Screen::Home => home::show(app, ui),
        Screen::Standings => standings::show(app, ui),
        Screen::Schedule => schedule::show(app, ui),
        Screen::Roster => roster::show(app, ui),
        Screen::Lineup => lineup::show(app, ui),
        Screen::Player => player::show(app, ui),
        Screen::FreeAgents => free_agents::show(app, ui),
        Screen::Trades => trades::show(app, ui),
        Screen::Draft => draft::show(app, ui),
        Screen::Finances => finances::show(app, ui),
        Screen::Saves => saves::show(app, ui),
        Screen::Settings => settings::show(app, ui),
        Screen::Credits => credits::show(app, ui),
    });
}

/// Progress and cancel while something runs; otherwise the last message.
fn sim_bar(app: &mut App, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        if app.busy() {
            let p = app.progress();
            let frac = if p.total > 0 {
                p.done as f32 / p.total as f32
            } else {
                0.0
            };
            ui.add(
                egui::ProgressBar::new(frac)
                    .desired_width(320.0)
                    .text(format!("{} {}/{}", p.label, p.done, p.total)),
            );
            if ui.button("Cancel").clicked() {
                app.cancel();
            }
        } else if let Some(m) = &app.message {
            let c = if m.error {
                egui::Color32::from_rgb(220, 90, 90)
            } else {
                ui.visuals().text_color()
            };
            ui.colored_label(c, &m.text);
        }
        if let Some(o) = &app.data.overview {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(format!(
                    "{} · {} · {}-{} · {}",
                    o.club_name,
                    o.year,
                    o.record.0,
                    o.record.1,
                    stage_name(o.stage)
                ));
            });
        }
    });
}

pub fn stage_name(s: Stage) -> &'static str {
    match s {
        Stage::Season => "Regular season",
        Stage::Playoffs => "Playoffs next",
        Stage::Offseason => "Offseason",
        Stage::Draft => "Draft",
        Stage::FreeAgency => "Trades & free agency",
    }
}

/// A button that asks the league to advance (disabled while busy).
pub fn advance_button(app: &mut App, ui: &mut egui::Ui, a: Advance, label: &str) {
    if ui
        .add_enabled(!app.busy(), egui::Button::new(label))
        .clicked()
    {
        app.advance(a);
    }
}
