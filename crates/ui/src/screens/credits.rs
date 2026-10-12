//! Credits and the data notice.

use crate::app::App;

pub fn show(_app: &mut App, ui: &mut egui::Ui) {
    ui.heading("Credits");
    ui.add_space(6.0);
    ui.label(
        "All players and teams in this game are fictional. Any resemblance to real \
         people or clubs is coincidental.",
    );
    ui.add_space(10.0);
    ui.label(
        "The information used here was obtained free of charge from and is copyrighted \
         by Retrosheet. Interested parties may contact Retrosheet at \
         \"www.retrosheet.org\".",
    );
    ui.add_space(10.0);
    ui.label("Built with Rust, egui/eframe, rayon, rusqlite.");
    ui.label(
        "The default fonts come with egui and are used under the SIL Open Font \
         License 1.1 and the Ubuntu Font Licence 1.0. Full licence list: THIRD_PARTY.md.",
    );
}
