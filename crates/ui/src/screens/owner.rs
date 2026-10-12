//! The owner's desk: the club's books, the payroll budget, the ballpark, the
//! ticket price, the stance and the history of past seasons.

use host::api::{Action, OwnerView, Stance};

use crate::app::App;
use crate::screens::stage_name;
use crate::widgets::money;

const RED: egui::Color32 = egui::Color32::from_rgb(220, 90, 90);

pub fn show(app: &mut App, ui: &mut egui::Ui) {
    ui.heading("Owner's desk");
    if let Some(e) = app.data.owner_error.clone() {
        ui.colored_label(RED, e);
    }
    let Some(o) = app.data.owner.clone() else {
        if app.data.owner_error.is_none() {
            ui.spinner();
        }
        return;
    };
    let live = o.can_act && !app.busy();
    ui.label(format!(
        "{} · {} · {}",
        o.club_name,
        o.year,
        stage_name(o.stage)
    ));
    if !o.can_act {
        ui.label(
            "The owner acts in the offseason, the draft and free agency. Decisions are closed now.",
        );
    }
    ui.add_space(6.0);
    books(app, ui, &o, live);
    ui.separator();
    ballpark(app, ui, &o, live);
    ui.separator();
    stance(app, ui, &o, live);
    ui.separator();
    history(ui, &o);
}

/// Cash, revenue and payroll against the budget, with the budget slider.
fn books(app: &mut App, ui: &mut egui::Ui, o: &OwnerView, live: bool) {
    ui.strong("Books");
    egui::Grid::new("owner_books")
        .num_columns(2)
        .spacing([12.0, 6.0])
        .show(ui, |ui| {
            ui.label("Cash");
            ui.label(money(o.cash));
            ui.end_row();
            ui.label("Revenue last season");
            ui.label(money(o.revenue_prev));
            ui.end_row();
            ui.label("Payroll against budget");
            ui.label(format!("{} of {}", money(o.payroll), money(o.budget)));
            ui.end_row();
            ui.label("Lenders will back up to");
            ui.label(money(o.budget_limit));
            ui.end_row();
        });
    let limits = app
        .data
        .finances
        .as_ref()
        .map(|f| (f.payroll_floor, f.payroll_cap));
    let Some((lo, hi)) = limits else {
        ui.label("Payroll limits loading.");
        return;
    };
    // The slider stops at what the lenders will back, never below the floor.
    let top = hi.min(o.budget_limit).max(lo);
    let mut v = app.state.owner_budget.unwrap_or(o.budget);
    let r = ui.add_enabled(
        live,
        egui::Slider::new(&mut v, lo..=top)
            .text("Payroll budget")
            .suffix(" M"),
    );
    if let Some(p) = settle(&r, v, &mut app.state.owner_budget) {
        app.act(Action::SetBudget { payroll: p });
    }
    ui.label(format!(
        "The budget stays between the league's payroll floor and cap, {} to {}.",
        money(lo),
        money(top)
    ));
}

/// The ballpark: its seats, quality, crowd, gate and the ticket price.
fn ballpark(app: &mut App, ui: &mut egui::Ui, o: &OwnerView, live: bool) {
    ui.strong("Ballpark");
    let (lo, hi) = o.ticket_range;
    let mut ticket = app.state.owner_ticket.unwrap_or(o.stadium.ticket);
    let mut sent = None;
    egui::Grid::new("owner_park")
        .num_columns(2)
        .spacing([12.0, 6.0])
        .show(ui, |ui| {
            ui.label("Seats");
            ui.label(format!("{:.1}k", o.stadium.capacity_k));
            ui.end_row();
            ui.label("Quality");
            ui.label(format!("{:.2}", o.stadium.quality));
            ui.end_row();
            ui.label("Last crowd");
            ui.label(format!("{:.1}k per home game", o.attendance_prev));
            ui.end_row();
            ui.label("Sold out last season");
            ui.label(if o.sold_out { "Yes" } else { "No" });
            ui.end_row();
            ui.label("Gate last season");
            ui.label(money(o.gate_prev));
            ui.end_row();
            ui.label("Ticket price");
            let r = ui.add_enabled(
                live,
                egui::DragValue::new(&mut ticket)
                    .range(lo..=hi)
                    .speed(0.25)
                    .prefix("$"),
            );
            sent = settle(&r, ticket, &mut app.state.owner_ticket);
            ui.end_row();
            ui.label("Suggested price");
            ui.horizontal(|ui| {
                ui.label(format!("${:.2}", o.suggested_ticket));
                if ui
                    .add_enabled(live, egui::Button::new("Use suggested"))
                    .clicked()
                {
                    sent = Some(o.suggested_ticket);
                    app.state.owner_ticket = None;
                }
            });
            ui.end_row();
        });
    ui.label(format!(
        "Tickets sell between ${lo:.0} and ${hi:.0}. The books suggest ${:.0}. A higher price means fewer fans come.",
        o.suggested_ticket
    ));
    if let Some(p) = sent {
        app.act(Action::SetTicketPrice { price: p });
    }

    ui.add_space(4.0);
    ui.horizontal(|ui| {
        let mut amount = app.state.build_amount;
        let r = ui.add_enabled(
            live,
            egui::DragValue::new(&mut amount)
                .range(0.1..=1000.0)
                .speed(0.5)
                .suffix(" M"),
        );
        if r.changed() {
            app.state.build_amount = amount;
        }
        if ui.add_enabled(live, egui::Button::new("Build")).clicked() {
            app.act(Action::BuildStadium {
                amount: app.state.build_amount,
            });
        }
    });
    ui.label(o.build_cost.as_str());
}

/// The stance and what it tells the general manager.
fn stance(app: &mut App, ui: &mut egui::Ui, o: &OwnerView, live: bool) {
    ui.strong("Stance");
    ui.label(format!(
        "Now: {}. {}",
        stance_name(o.stance),
        stance_note(o.stance)
    ));
    ui.horizontal(|ui| {
        for s in [Stance::Contend, Stance::Balanced, Stance::Rebuild] {
            let b = egui::Button::new(stance_name(s)).selected(o.stance == s);
            if ui.add_enabled(live, b).clicked() {
                app.act(Action::SetStance { stance: s });
            }
        }
    });
}

fn stance_name(s: Stance) -> &'static str {
    match s {
        Stance::Contend => "Contend",
        Stance::Balanced => "Balanced",
        Stance::Rebuild => "Rebuild",
    }
}

fn stance_note(s: Stance) -> &'static str {
    match s {
        Stance::Contend => "The general manager wins now: veterans and picks are spent.",
        Stance::Balanced => "The general manager balances this year against the next.",
        Stance::Rebuild => "The general manager builds for later years: youth and picks.",
    }
}

/// Past seasons, oldest first.
fn history(ui: &mut egui::Ui, o: &OwnerView) {
    ui.strong("History");
    if o.history.is_empty() {
        ui.label("No past seasons yet.");
        return;
    }
    egui::ScrollArea::vertical()
        .id_salt("owner_history")
        .max_height(260.0)
        .show(ui, |ui| {
            egui::Grid::new("owner_history_grid")
                .num_columns(5)
                .striped(true)
                .spacing([14.0, 4.0])
                .show(ui, |ui| {
                    for h in ["Year", "Attendance", "Gate", "Revenue", "Payroll"] {
                        ui.strong(h);
                    }
                    ui.end_row();
                    for h in &o.history {
                        ui.label(h.year.to_string());
                        ui.label(format!("{:.1}k", h.attendance));
                        ui.label(money(h.gate));
                        ui.label(money(h.revenue));
                        ui.label(money(h.payroll));
                        ui.end_row();
                    }
                });
        });
}

/// Track a drag: while it runs, keep the value on the screen only. When it
/// ends, or when the value changes from the keyboard, return it to send.
fn settle(r: &egui::Response, v: f64, local: &mut Option<f64>) -> Option<f64> {
    if r.dragged() {
        *local = Some(v);
        return None;
    }
    *local = None;
    (r.drag_stopped() || r.changed()).then_some(v)
}
