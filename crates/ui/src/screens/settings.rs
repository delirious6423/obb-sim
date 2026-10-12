//! Settings. Mostly informational: the difficulty in use, where saves live,
//! and a pointer to the depth chart for tactics.

use crate::app::App;

pub fn show(app: &mut App, ui: &mut egui::Ui) {
    ui.heading("Settings");
    ui.add_space(6.0);

    egui::Grid::new("settings_grid")
        .num_columns(2)
        .spacing([12.0, 8.0])
        .show(ui, |ui| {
            ui.label("Difficulty");
            match &app.data.overview {
                Some(o) => ui.label(&o.difficulty),
                None => ui.label("No league started yet"),
            };
            ui.end_row();

            ui.label("Save folder");
            ui.label(save_folder());
            ui.end_row();
        });

    ui.add_space(12.0);
    ui.strong("Smart tactics");
    ui.label("Smart tactics is switched on and off on the Depth chart screen.");
}

/// Where saves are written (see [`host::default_saves_dir`]).
fn save_folder() -> String {
    host::default_saves_dir().map_or_else(
        || format!("no home folder; set {}", host::SAVES_ENV),
        |p| p.to_string_lossy().into_owned(),
    )
}
