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

/// Where saves are written: the BASEBALL_SIM_SAVES folder if set, else the
/// default folder in the home directory.
fn save_folder() -> String {
    match std::env::var_os("BASEBALL_SIM_SAVES") {
        Some(p) => p.to_string_lossy().into_owned(),
        None => "~/.baseball-sim/saves".into(),
    }
}
