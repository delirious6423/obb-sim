//! A club's roster: its players, sortable, with payroll and budget. Any club
//! can be shown; the problems list is only filled for the person's own club.

use host::api::{Club, Level, PlayerRow};

use crate::app::App;
use crate::widgets::{Col, level_name, money, player_table};

const LEVELS: [Level; 4] = [Level::Majors, Level::Triple, Level::Double, Level::Single];

const COLS: &[Col] = &[
    Col::Name,
    Col::Pos,
    Col::Age,
    Col::Level,
    Col::Now,
    Col::Peak,
    Col::Salary,
    Col::Years,
    Col::Out,
    Col::Stat,
];

pub fn show(app: &mut App, ui: &mut egui::Ui) {
    club_switcher(app, ui);
    ui.separator();

    level_filter(app, ui);

    let viewing = app.viewing;
    let Some(r) = app.data.rosters.get(&viewing) else {
        ui.spinner();
        return;
    };

    ui.heading(&r.club_name);
    ui.label(format!(
        "Payroll {} · budget {}",
        money(r.payroll),
        money(r.budget)
    ));
    if r.own {
        for p in &r.problems {
            ui.colored_label(egui::Color32::from_rgb(220, 90, 90), p);
        }
    }
    ui.add_space(6.0);

    let filter = app.state.level_filter;
    let rows: Vec<PlayerRow> = r
        .players
        .iter()
        .filter(|p| filter.is_none_or(|l| p.level == l))
        .cloned()
        .collect();

    let clicked = player_table(
        ui,
        "roster",
        &rows,
        COLS,
        &mut app.state.roster_sort,
        |_, _| {},
    );
    if let Some(id) = clicked {
        app.open_player(id);
    }
}

/// A ComboBox over the club names. Changing the club asks for its roster.
fn club_switcher(app: &mut App, ui: &mut egui::Ui) {
    let before: Club = app.viewing;
    let selected = app
        .state
        .clubs
        .get(usize::from(app.viewing))
        .map_or("?", String::as_str);
    egui::ComboBox::from_id_salt("roster_club")
        .selected_text(selected)
        .show_ui(ui, |ui| {
            for (i, name) in app.state.clubs.iter().enumerate() {
                if let Ok(c) = Club::try_from(i) {
                    ui.selectable_value(&mut app.viewing, c, name);
                }
            }
        });
    if app.viewing != before {
        app.fetch();
    }
}

/// A ComboBox that narrows the table to one level (or all of them).
fn level_filter(app: &mut App, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.label("Level");
        egui::ComboBox::from_id_salt("roster_level")
            .selected_text(app.state.level_filter.map_or("All levels", level_name))
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut app.state.level_filter, None, "All levels");
                for l in LEVELS {
                    ui.selectable_value(&mut app.state.level_filter, Some(l), level_name(l));
                }
            });
    });
}
