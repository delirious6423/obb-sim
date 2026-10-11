//! Saved games: load, delete, and (during a game) save and quick-save.

use host::api::{Request, SaveInfo};

use crate::app::{App, Tag};

const RED: egui::Color32 = egui::Color32::from_rgb(220, 90, 90);

pub fn show(app: &mut App, ui: &mut egui::Ui) {
    ui.heading("Saves");
    ui.add_space(6.0);
    let busy = app.busy();

    if app.started {
        save_controls(app, ui, busy);
        ui.add_space(8.0);
    }

    let Some(saves) = app.data.saves.clone() else {
        ui.spinner();
        return;
    };
    if saves.is_empty() {
        ui.label("No saves yet.");
        return;
    }
    table(app, ui, &saves, busy);
}

fn save_controls(app: &mut App, ui: &mut egui::Ui, busy: bool) {
    ui.horizontal(|ui| {
        ui.label("Save as");
        ui.add(
            egui::TextEdit::singleline(&mut app.state.save_name)
                .desired_width(200.0)
                .hint_text("slot name"),
        );
        let name = app.state.save_name.trim().to_string();
        if ui
            .add_enabled(!busy && !name.is_empty(), egui::Button::new("Save"))
            .clicked()
        {
            app.ask(Request::Save { slot: name }, Tag::Change);
        }
        if ui
            .add_enabled(!busy, egui::Button::new("Quick save"))
            .clicked()
        {
            app.ask(Request::QuickSave, Tag::Change);
        }
    });
}

fn table(app: &mut App, ui: &mut egui::Ui, saves: &[SaveInfo], busy: bool) {
    // The slot waiting for a second click to delete it.
    let ctx = ui.ctx().clone();
    let armed_id = egui::Id::new("saves_armed_delete");
    let armed: Option<String> = ctx.data(|d| d.get_temp(armed_id));
    let mut load = None;
    let mut delete = None;

    egui::ScrollArea::vertical().show(ui, |ui| {
        egui::Grid::new("saves_grid")
            .striped(true)
            .num_columns(10)
            .spacing([14.0, 4.0])
            .show(ui, |ui| {
                for h in [
                    "Slot",
                    "League",
                    "Club",
                    "Role",
                    "Date",
                    "Difficulty",
                    "Kind",
                    "Problem",
                ] {
                    ui.strong(h);
                }
                ui.label("");
                ui.label("");
                ui.end_row();

                for s in saves {
                    ui.label(&s.slot);
                    ui.label(&s.league);
                    ui.label(&s.club_name);
                    ui.label(&s.role);
                    ui.label(&s.date);
                    ui.label(&s.difficulty);
                    ui.label(&s.kind);
                    match &s.problem {
                        Some(p) => ui.colored_label(RED, p),
                        None => ui.label(""),
                    };

                    let can_load = !busy && s.problem.is_none();
                    if ui
                        .add_enabled(can_load, egui::Button::new("Load"))
                        .clicked()
                    {
                        load = Some(s.slot.clone());
                    }

                    let confirming = armed.as_deref() == Some(s.slot.as_str());
                    let label = if confirming {
                        "Confirm delete"
                    } else {
                        "Delete"
                    };
                    if ui.add_enabled(!busy, egui::Button::new(label)).clicked() {
                        delete = Some((s.slot.clone(), confirming));
                    }
                    ui.end_row();
                }
            });
    });

    if let Some(slot) = load {
        // A load replaces the game in progress; the answer (Done) starts it.
        app.ask(Request::Load { slot }, Tag::Start);
    }
    if let Some((slot, confirming)) = delete {
        if confirming {
            ctx.data_mut(|d| d.remove_temp::<String>(armed_id));
            app.ask(Request::DeleteSave { slot }, Tag::Change);
            if !app.started {
                // Changes only refresh the list while a game runs.
                app.ask(Request::ListSaves, Tag::Saves);
            }
        } else {
            ctx.data_mut(|d| d.insert_temp(armed_id, slot));
        }
    }
}
