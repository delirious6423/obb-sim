//! The depth chart: the person's roster by level, with batters and pitchers
//! grouped and sorted by grade. Level moves, auto-arrange and tactics go
//! through `app.act`.

use host::api::{Action, Level, PlayerRow, Position};

use crate::app::App;
use crate::widgets::{level_name, money};

const LEVELS: [Level; 4] = [Level::Majors, Level::Triple, Level::Double, Level::Single];
const MAJORS_TARGET: usize = 26;
const RED: egui::Color32 = egui::Color32::from_rgb(220, 90, 90);

pub fn show(app: &mut App, ui: &mut egui::Ui) {
    let own = app.own_club();
    let Some(r) = app.data.rosters.get(&own).cloned() else {
        ui.spinner();
        return;
    };
    let busy = app.busy();

    ui.heading(format!("Depth chart · {}", r.club_name));
    ui.horizontal(|ui| {
        if ui
            .add_enabled(!busy, egui::Button::new("Auto-arrange"))
            .clicked()
        {
            app.act(Action::AutoRoster);
        }
        let mut smart = app.state.smart_tactics;
        if ui
            .add_enabled(!busy, egui::Checkbox::new(&mut smart, "Smart tactics"))
            .changed()
        {
            app.state.smart_tactics = smart;
            app.act(Action::SetTactics { smart });
        }
    });
    for problem in &r.problems {
        ui.colored_label(RED, problem);
    }
    ui.separator();

    let mut open = None;
    let mut moved: Option<(u32, Level)> = None;
    egui::ScrollArea::vertical().show(ui, |ui| {
        for l in LEVELS {
            let in_level: Vec<&PlayerRow> = r.players.iter().filter(|p| p.level == l).collect();
            let heading = if l == Level::Majors {
                format!(
                    "{} · {} of {} target",
                    level_name(l),
                    in_level.len(),
                    MAJORS_TARGET
                )
            } else {
                format!("{} · {}", level_name(l), in_level.len())
            };
            ui.add_space(8.0);
            ui.heading(heading);

            for (pos, title) in [
                (Position::Batter, "Batters"),
                (Position::Pitcher, "Pitchers"),
            ] {
                let mut group: Vec<&PlayerRow> = in_level
                    .iter()
                    .copied()
                    .filter(|p| p.position == pos)
                    .collect();
                group.sort_by(|a, b| b.grade_now.cmp(&a.grade_now).then(a.id.cmp(&b.id)));
                ui.strong(format!("{title} ({})", group.len()));
                if group.is_empty() {
                    ui.label("none");
                    continue;
                }
                egui::Grid::new(format!("lineup_{}_{}", level_name(l), title))
                    .striped(true)
                    .num_columns(7)
                    .show(ui, |ui| {
                        for h in ["Name", "Age", "Now", "Peak", "Salary", "Out", "Move"] {
                            ui.label(h);
                        }
                        ui.end_row();
                        for p in group {
                            if ui.link(&p.name).clicked() {
                                open = Some(p.id);
                            }
                            ui.label(p.age.to_string());
                            ui.label(p.grade_now.to_string());
                            ui.label(p.grade_peak.to_string());
                            ui.label(money(p.salary));
                            ui.label(if p.out_games > 0 {
                                format!("{} g", p.out_games)
                            } else {
                                String::new()
                            });
                            ui.horizontal(|ui| {
                                if let Some(up) = promote(p.level) {
                                    let hint = format!("Move up to {}", level_name(up));
                                    if ui
                                        .add_enabled(!busy, egui::Button::new("▲"))
                                        .on_hover_text(hint)
                                        .clicked()
                                    {
                                        moved = Some((p.id, up));
                                    }
                                }
                                if let Some(down) = demote(p.level) {
                                    let hint = format!("Move down to {}", level_name(down));
                                    if ui
                                        .add_enabled(!busy, egui::Button::new("▼"))
                                        .on_hover_text(hint)
                                        .clicked()
                                    {
                                        moved = Some((p.id, down));
                                    }
                                }
                            });
                            ui.end_row();
                        }
                    });
            }
        }
    });

    if let Some((player, level)) = moved {
        app.act(Action::SetLevel { player, level });
    }
    if let Some(id) = open {
        app.open_player(id);
    }
}

/// The level above `l`, if any (Majors is the top).
fn promote(l: Level) -> Option<Level> {
    match l {
        Level::Majors => None,
        Level::Triple => Some(Level::Majors),
        Level::Double => Some(Level::Triple),
        Level::Single => Some(Level::Double),
    }
}

/// The level below `l`, if any (Single-A is the bottom).
fn demote(l: Level) -> Option<Level> {
    match l {
        Level::Majors => Some(Level::Triple),
        Level::Triple => Some(Level::Double),
        Level::Double => Some(Level::Single),
        Level::Single => None,
    }
}
