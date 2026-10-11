//! The club's books: cash, budget, payroll against the floor and cap, past
//! seasons and every contract.

use host::api::FinancesView;

use crate::app::App;
use crate::widgets::{Col, money, player_table};

const RED: egui::Color32 = egui::Color32::from_rgb(220, 90, 90);

pub fn show(app: &mut App, ui: &mut egui::Ui) {
    let Some(f) = app.data.finances.clone() else {
        ui.spinner();
        return;
    };
    ui.heading(format!("Finances: {}", f.club_name));
    ui.add_space(6.0);

    summary(ui, &f);
    ui.add_space(10.0);

    ui.strong("Past seasons");
    history(ui, &f);
    ui.add_space(10.0);

    ui.strong("Contracts");
    let clicked = player_table(
        ui,
        "contracts",
        &f.contracts,
        &[
            Col::Name,
            Col::Pos,
            Col::Age,
            Col::Level,
            Col::Salary,
            Col::Years,
        ],
        &mut app.state.contract_sort,
        |_, _| {},
    );
    if let Some(id) = clicked {
        app.open_player(id);
    }
}

fn summary(ui: &mut egui::Ui, f: &FinancesView) {
    egui::Grid::new("finances_summary")
        .num_columns(2)
        .spacing([12.0, 6.0])
        .show(ui, |ui| {
            ui.label("Cash");
            ui.label(money(f.cash));
            ui.end_row();

            ui.label("Budget");
            ui.label(money(f.budget));
            ui.end_row();

            ui.label("Revenue last season");
            ui.label(money(f.revenue_prev));
            ui.end_row();

            ui.label("Payroll floor");
            ui.label(money(f.payroll_floor));
            ui.end_row();

            ui.label("Payroll cap");
            ui.label(money(f.payroll_cap));
            ui.end_row();
        });

    ui.add_space(6.0);
    let scale = f.payroll_cap.max(f.payroll_floor).max(f.payroll);
    let frac = if scale > 0.0 {
        (f.payroll / scale).clamp(0.0, 1.0)
    } else {
        0.0
    };
    ui.label(format!("Payroll {}", money(f.payroll)));
    ui.add(
        egui::ProgressBar::new(frac as f32)
            .desired_width(320.0)
            .text(format!(
                "{} of {} cap",
                money(f.payroll),
                money(f.payroll_cap)
            )),
    );
    if f.payroll < f.payroll_floor {
        ui.colored_label(
            RED,
            format!(
                "Below the payroll floor by {}.",
                money(f.payroll_floor - f.payroll)
            ),
        );
    } else if f.payroll > f.payroll_cap {
        ui.colored_label(
            RED,
            format!(
                "Over the payroll cap by {}.",
                money(f.payroll - f.payroll_cap)
            ),
        );
    }
}

fn history(ui: &mut egui::Ui, f: &FinancesView) {
    if f.history.is_empty() {
        ui.label("No past seasons yet.");
        return;
    }
    egui::ScrollArea::vertical()
        .id_salt("finances_history")
        .max_height(180.0)
        .show(ui, |ui| {
            egui::Grid::new("finances_history_grid")
                .striped(true)
                .num_columns(6)
                .spacing([14.0, 4.0])
                .show(ui, |ui| {
                    for h in ["Year", "Revenue", "Payroll", "Other", "Spent", "Dividend"] {
                        ui.strong(h);
                    }
                    ui.end_row();
                    for r in &f.history {
                        ui.label(r.year.to_string());
                        ui.label(money(r.revenue));
                        ui.label(money(r.payroll));
                        ui.label(money(r.other));
                        ui.label(money(r.spent));
                        ui.label(money(r.dividend));
                        ui.end_row();
                    }
                });
        });
}
