//! Start a league: seed, club, difficulty (presets or custom levers).

use host::api::{Difficulty, NewGame, Preset, Request, TradeLevel};

use crate::app::{App, Tag};

pub fn show(app: &mut App, ui: &mut egui::Ui) {
    ui.heading("New game");
    ui.add_space(6.0);
    let busy = app.busy();
    egui::Grid::new("new_game")
        .num_columns(2)
        .spacing([12.0, 8.0])
        .show(ui, |ui| {
            ui.label("League seed");
            ui.text_edit_singleline(&mut app.state.seed_text);
            ui.end_row();

            ui.label("Your club");
            let names = app.state.clubs.clone();
            let cur = names
                .get(usize::from(app.state.pick_club))
                .cloned()
                .unwrap_or_default();
            egui::ComboBox::from_id_salt("club")
                .selected_text(cur)
                .width(260.0)
                .show_ui(ui, |ui| {
                    for (i, n) in names.iter().enumerate() {
                        ui.selectable_value(&mut app.state.pick_club, i as u8, n);
                    }
                });
            ui.end_row();

            ui.label("Years of history");
            ui.add(egui::Slider::new(&mut app.state.burn_in, 0..=30));
            ui.end_row();
        });

    ui.add_space(10.0);
    ui.heading("Difficulty");
    ui.label(
        "Difficulty changes what the other general managers know and how well \
         they decide. It never changes the dice.",
    );
    ui.horizontal(|ui| {
        for (p, name) in [
            (Preset::Rookie, "Rookie"),
            (Preset::Veteran, "Veteran"),
            (Preset::AllStar, "All-Star"),
            (Preset::HallOfFame, "Hall of Fame"),
        ] {
            let on = !app.state.custom && app.state.preset == p;
            if ui.selectable_label(on, name).clicked() {
                app.state.custom = false;
                app.state.preset = p;
                app.state.levers = host::levers_of(p);
            }
        }
        if ui.selectable_label(app.state.custom, "Custom").clicked() {
            app.state.custom = true;
        }
    });

    if app.state.custom {
        let l = &mut app.state.levers;
        egui::Grid::new("levers")
            .num_columns(2)
            .spacing([12.0, 6.0])
            .show(ui, |ui| {
                ui.label("Scouting error added");
                ui.add(egui::Slider::new(&mut l.extra_noise, 0.0..=3.0));
                ui.end_row();
                ui.label("Reads the market");
                ui.add(egui::Slider::new(&mut l.market_weight, 0.0..=1.0));
                ui.end_row();
                ui.label("Contract horizon (years)");
                ui.add(egui::Slider::new(&mut l.horizon, 1..=7));
                ui.end_row();
                ui.label("Projects ageing");
                ui.checkbox(&mut l.projects_aging, "");
                ui.end_row();
                ui.label("Values draft picks by years");
                ui.checkbox(&mut l.picks_years, "");
                ui.end_row();
                ui.label("Trading");
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut l.trades, TradeLevel::Off, "Off");
                    ui.selectable_value(&mut l.trades, TradeLevel::Respond, "Responds");
                    ui.selectable_value(&mut l.trades, TradeLevel::Active, "Active");
                });
                ui.end_row();
                ui.label("Spends its budget");
                ui.checkbox(&mut l.spends_budget, "");
                ui.end_row();
                ui.label("Taste for youth");
                ui.add(egui::Slider::new(&mut l.youth, 0.0..=1.0));
                ui.end_row();
                ui.label("Thrift");
                ui.add(egui::Slider::new(&mut l.thrift, -3.0..=3.0));
                ui.end_row();
                ui.label("Margin wanted in trades");
                ui.add(egui::Slider::new(&mut l.trade_margin, 0.0..=6.0));
                ui.end_row();
            });
    }

    ui.add_space(12.0);
    let seed = app.state.seed_text.trim().parse::<u64>().ok();
    if seed.is_none() {
        ui.colored_label(
            egui::Color32::from_rgb(220, 90, 90),
            "The seed must be a whole number.",
        );
    }
    ui.horizontal(|ui| {
        let go = ui.add_enabled(
            !busy && seed.is_some(),
            egui::Button::new("Start the league"),
        );
        if go.clicked() {
            let difficulty = if app.state.custom {
                Difficulty::Custom(app.state.levers)
            } else {
                Difficulty::Preset(app.state.preset)
            };
            app.ask(
                Request::NewGame(NewGame {
                    seed: seed.unwrap_or(0),
                    club: app.state.pick_club,
                    difficulty,
                    burn_in_years: app.state.burn_in,
                    roles: Default::default(),
                }),
                Tag::Start,
            );
        }
        if busy {
            ui.spinner();
            ui.label("Founding the league and playing its history…");
        }
    });
}
