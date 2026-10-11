//! The desk: where things stand and what to do next.

use host::api::Stage;

use crate::app::App;
use crate::screens::{advance_button, stage_name};
use crate::widgets::money;

pub fn show(app: &mut App, ui: &mut egui::Ui) {
    let Some(o) = app.data.overview.clone() else {
        ui.spinner();
        return;
    };
    ui.heading(format!("{} — {}", o.club_name, o.league));
    ui.label(format!(
        "{} · {} · {} · difficulty {}",
        o.year,
        stage_name(o.stage),
        o.place,
        o.difficulty
    ));
    ui.add_space(6.0);
    if o.stage == Stage::Season {
        ui.add(
            egui::ProgressBar::new(f32::from(o.day) / f32::from(o.days.max(1))).text(format!(
                "Day {} of {} · {}-{}",
                o.day, o.days, o.record.0, o.record.1
            )),
        );
    }
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.label(format!(
            "Cash {} · payroll {} · budget {}",
            money(o.cash),
            money(o.payroll),
            money(o.budget)
        ));
    });
    if o.stage == Stage::Offseason && (o.roles.owner || o.roles.commissioner) {
        ui.label("Set your owner/league decisions, then close the books.");
    }
    if o.on_the_clock {
        ui.colored_label(
            egui::Color32::from_rgb(230, 180, 60),
            "You are on the clock in the draft.",
        );
    }
    if o.open_offers > 0 {
        ui.colored_label(
            egui::Color32::from_rgb(230, 180, 60),
            format!("{} trade offer(s) waiting for you.", o.open_offers),
        );
    }
    ui.separator();
    ui.heading("Advance");
    ui.horizontal_wrapped(|ui| {
        for (a, label) in &o.advance {
            advance_button(app, ui, *a, label);
        }
    });
    ui.separator();
    ui.heading("News");
    egui::ScrollArea::vertical().show(ui, |ui| {
        for n in o.notices.iter().rev() {
            ui.label(n);
        }
    });
}
