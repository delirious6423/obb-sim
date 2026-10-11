//! A club's games this season. Clicking a played game opens its box score.

use host::api::{Request, ScheduleView};

use crate::app::{App, Tag};

const GREEN: egui::Color32 = egui::Color32::from_rgb(90, 190, 110);
const RED: egui::Color32 = egui::Color32::from_rgb(220, 90, 90);

pub fn show(app: &mut App, ui: &mut egui::Ui) {
    club_switcher(app, ui);
    ui.add_space(6.0);

    let Some(s) = app.data.schedule.get(&app.viewing).cloned() else {
        ui.spinner();
        return;
    };
    ui.heading(format!("{} schedule {}", s.club_name, s.year));
    record(ui, &s);
    ui.add_space(6.0);

    if let Some(game) = app.state.game_open {
        box_score_panel(app, ui, game);
    }

    let mut open = None;
    egui::ScrollArea::vertical().show(ui, |ui| {
        egui::Grid::new("schedule_games")
            .striped(true)
            .num_columns(4)
            .spacing([16.0, 4.0])
            .show(ui, |ui| {
                ui.strong("Day");
                ui.strong("Home/Away");
                ui.strong("Opponent");
                ui.strong("Result");
                ui.end_row();
                for g in &s.games {
                    ui.label(g.day.to_string());
                    ui.label(if g.home { "Home" } else { "Away" });
                    ui.label(&g.opponent);
                    match g.result {
                        Some((rf, ra)) => {
                            let (letter, color) = match rf.cmp(&ra) {
                                std::cmp::Ordering::Greater => ("W", GREEN),
                                std::cmp::Ordering::Less => ("L", RED),
                                std::cmp::Ordering::Equal => ("T", egui::Color32::GRAY),
                            };
                            let text =
                                egui::RichText::new(format!("{letter} {rf}-{ra}")).color(color);
                            if ui.link(text).clicked() {
                                open = Some(g.game);
                            }
                        }
                        None => {
                            ui.label("—");
                        }
                    }
                    ui.end_row();
                }
            });
    });

    if let Some(game) = open {
        app.state.game_open = Some(game);
        app.ask(Request::BoxScore { game }, Tag::BoxScore(game));
    }
}

fn club_switcher(app: &mut App, ui: &mut egui::Ui) {
    let names = app.state.clubs.clone();
    let current = names
        .get(usize::from(app.viewing))
        .cloned()
        .unwrap_or_default();
    let mut pick = app.viewing;
    ui.horizontal(|ui| {
        ui.label("Club");
        egui::ComboBox::from_id_salt("schedule_club")
            .selected_text(current)
            .width(260.0)
            .show_ui(ui, |ui| {
                for (i, n) in names.iter().enumerate() {
                    ui.selectable_value(&mut pick, i as u8, n);
                }
            });
    });
    if pick != app.viewing {
        app.viewing = pick;
        app.fetch();
    }
}

/// Wins and losses from the games played so far.
fn record(ui: &mut egui::Ui, s: &ScheduleView) {
    let (mut w, mut l, mut played) = (0u32, 0u32, 0u32);
    for (rf, ra) in s.games.iter().filter_map(|g| g.result) {
        played += 1;
        match rf.cmp(&ra) {
            std::cmp::Ordering::Greater => w += 1,
            std::cmp::Ordering::Less => l += 1,
            std::cmp::Ordering::Equal => {}
        }
    }
    ui.label(format!(
        "Record {w}-{l} · {played} of {} games played",
        s.games.len()
    ));
}

fn box_score_panel(app: &mut App, ui: &mut egui::Ui, game: u16) {
    let mut close = false;
    egui::Panel::right("box_score")
        .resizable(true)
        .default_size(420.0)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.strong(format!("Game {game}"));
                if ui.button("Close").clicked() {
                    close = true;
                }
            });
            ui.separator();
            match app.data.box_score.as_ref() {
                Some((g, Some(text))) if *g == game => {
                    egui::ScrollArea::both().show(ui, |ui| {
                        ui.label(egui::RichText::new(text).monospace());
                    });
                }
                Some((g, None)) if *g == game => {
                    ui.label("Box score not available for this game");
                }
                _ => {
                    ui.label("Loading box score…");
                }
            }
        });
    if close {
        app.state.game_open = None;
    }
}
