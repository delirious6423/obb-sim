//! The desktop front end. It talks to the league only through the host
//! protocol (`host::api` and `GameClient`); it never sees the simulation.

mod app;
mod screens;
mod widgets;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Baseball Sim")
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([900.0, 560.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Baseball Sim",
        options,
        Box::new(|_cc| Ok(Box::new(app::App::new()))),
    )
}
