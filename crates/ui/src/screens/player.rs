//! One player: grades, contract, career and history. For a player on the
//! person's own club, Release (two clicks) and level moves.

use host::api::{Action, Level};

use crate::app::{App, Screen};
use crate::widgets::{level_name, money, pos_name};

const LEVELS: [Level; 4] = [Level::Majors, Level::Triple, Level::Double, Level::Single];

pub fn show(app: &mut App, ui: &mut egui::Ui) {
    if ui.button("Back").clicked() {
        app.go(Screen::Roster);
    }
    ui.separator();

    let Some(v) = app.data.player.clone() else {
        ui.spinner();
        return;
    };
    if app.state.player_id != Some(v.row.id) {
        ui.spinner();
        return;
    }
    let p = &v.row;
    let busy = app.busy();

    ui.heading(&p.name);
    let club = v.club_name.as_deref().unwrap_or("Free agent");
    ui.label(format!(
        "{club} · {} · age {} · {} · from {}",
        pos_name(p.position),
        p.age,
        level_name(p.level),
        p.origin
    ));
    ui.add_space(6.0);

    ui.label(format!("Grade now {} · peak {}", p.grade_now, p.grade_peak));
    if p.club.is_some() {
        ui.label(format!(
            "Contract {} for {} yr(s)",
            money(p.salary),
            p.years
        ));
    } else {
        ui.label(format!("Asks {} for {} yr(s)", money(p.ask), p.ask_years));
    }
    if p.out_games > 0 {
        ui.label(format!("Out for {} more games", p.out_games));
    }
    if let Some((year, pick)) = v.drafted {
        ui.label(format!("Drafted in {year} (pick {pick})"));
    }
    ui.label(format!(
        "Career: {} season(s), {:.1} WAR",
        v.career_seasons, v.career_war
    ));
    if !p.stat.is_empty() {
        ui.label(format!("This season: {}", p.stat));
    }

    if p.club.is_some() && p.club == Some(app.own_club()) {
        ui.separator();
        ui.heading("Your club");
        let mut moved = None;
        ui.horizontal(|ui| {
            ui.label("Move to:");
            for l in LEVELS {
                if l == p.level {
                    continue;
                }
                if ui
                    .add_enabled(!busy, egui::Button::new(level_name(l)))
                    .clicked()
                {
                    moved = Some(l);
                }
            }
        });
        if let Some(level) = moved {
            app.act(Action::SetLevel {
                player: p.id,
                level,
            });
        }

        release_button(app, ui, p.id, busy);
    }

    ui.separator();
    ui.heading("History");
    for line in &v.history {
        ui.label(line);
    }
}

/// Release needs a second click: the first arms it, the second sends it.
fn release_button(app: &mut App, ui: &mut egui::Ui, player: u32, busy: bool) {
    let armed_id = egui::Id::new("player_release_armed");
    let armed = ui.data(|d| d.get_temp::<u32>(armed_id)) == Some(player);
    let label = if armed { "Confirm release" } else { "Release" };
    if ui
        .add_enabled(!busy, egui::Button::new(label))
        .on_hover_text("Click twice to release this player")
        .clicked()
    {
        if armed {
            ui.data_mut(|d| d.remove_temp::<u32>(armed_id));
            app.act(Action::Release { player });
            app.go(Screen::Roster);
        } else {
            ui.data_mut(|d| d.insert_temp(armed_id, player));
        }
    }
}
