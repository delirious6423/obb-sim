//! Free agents: sign a player at his ask, and move free agency on.

use host::api::{Action, PlayerRow, Stage};

use crate::app::App;
use crate::screens::{advance_button, stage_name};
use crate::widgets::{Col, money, player_table};

const COLS: &[Col] = &[
    Col::Name,
    Col::Pos,
    Col::Age,
    Col::Now,
    Col::Peak,
    Col::Ask,
    Col::Out,
];

pub fn show(app: &mut App, ui: &mut egui::Ui) {
    let Some(rows) = app.data.free_agents.clone() else {
        ui.spinner();
        return;
    };
    let overview = app.data.overview.clone();
    ui.heading("Free agents");
    if let Some(o) = &overview {
        ui.label(format!(
            "Payroll {} of budget {} · cash {}",
            money(o.payroll),
            money(o.budget),
            money(o.cash)
        ));
        if o.stage == Stage::FreeAgency {
            ui.horizontal_wrapped(|ui| {
                for (a, label) in &o.advance {
                    advance_button(app, ui, *a, label);
                }
            });
        } else {
            ui.label(format!("Now: {}", stage_name(o.stage)));
        }
    }
    ui.separator();
    ui.horizontal(|ui| {
        ui.label("Name");
        ui.add(egui::TextEdit::singleline(&mut app.state.fa_filter).hint_text("contains"));
    });
    let needle = app.state.fa_filter.trim().to_lowercase();
    let shown: Vec<PlayerRow> = rows
        .iter()
        .filter(|p| needle.is_empty() || p.name.to_lowercase().contains(&needle))
        .cloned()
        .collect();
    ui.label(format!("{} of {} shown", shown.len(), rows.len()));

    let busy = app.busy();
    let mut signing = None;
    let clicked = player_table(
        ui,
        "free_agents",
        &shown,
        COLS,
        &mut app.state.fa_sort,
        |ui, p| {
            if ui
                .add_enabled(!busy, egui::Button::new("Sign"))
                .on_hover_text(format!("{} for {} years", money(p.ask), p.ask_years))
                .clicked()
            {
                signing = Some(Action::Sign {
                    player: p.id,
                    salary: p.ask,
                    years: p.ask_years,
                });
            }
        },
    );
    if let Some(id) = clicked {
        app.open_player(id);
    }
    if let Some(a) = signing {
        app.act(a);
    }
}
