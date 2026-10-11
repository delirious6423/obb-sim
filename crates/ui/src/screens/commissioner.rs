//! The league office: the rules a commissioner sets, and the public books of
//! every club, sortable by clicking a header.

use std::cmp::Ordering;

use host::api::{Action, ClubBooksRow, LeagueView};

use crate::app::App;
use crate::screens::stage_name;
use crate::widgets::money;

const RED: egui::Color32 = egui::Color32::from_rgb(220, 90, 90);

/// A column of the league table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BooksCol {
    Name,
    Market,
    Payroll,
    Cash,
    Revenue,
    Ticket,
    Attendance,
    Budget,
}

/// The league table's sort: a column and a direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BooksSort {
    pub col: BooksCol,
    pub desc: bool,
}

impl Default for BooksSort {
    fn default() -> Self {
        Self {
            col: BooksCol::Market,
            desc: true,
        }
    }
}

impl BooksSort {
    /// Sort by a clicked column. Clicking the same column again flips the order.
    pub fn click(&mut self, col: BooksCol) {
        if self.col == col {
            self.desc = !self.desc;
        } else {
            self.col = col;
            self.desc = true;
        }
    }

    fn order(&self, a: &ClubBooksRow, b: &ClubBooksRow) -> Ordering {
        let o = match self.col {
            BooksCol::Name => a.name.cmp(&b.name),
            BooksCol::Market => a.market.total_cmp(&b.market),
            BooksCol::Payroll => a.payroll.total_cmp(&b.payroll),
            BooksCol::Cash => a.cash.total_cmp(&b.cash),
            BooksCol::Revenue => a.revenue_prev.total_cmp(&b.revenue_prev),
            BooksCol::Ticket => a.ticket.total_cmp(&b.ticket),
            BooksCol::Attendance => a.attendance_prev.total_cmp(&b.attendance_prev),
            BooksCol::Budget => a.budget.total_cmp(&b.budget),
        };
        if self.desc { o.reverse() } else { o }
    }
}

pub fn show(app: &mut App, ui: &mut egui::Ui) {
    ui.heading("League office");
    if let Some(e) = app.data.league_error.clone() {
        ui.colored_label(RED, e);
    }
    let Some(l) = app.data.league.clone() else {
        if app.data.league_error.is_none() {
            ui.spinner();
        }
        return;
    };
    let live = l.can_act && !app.busy();
    ui.label(format!("{} · {}", l.year, stage_name(l.stage)));
    if !l.can_act {
        ui.label("The rules change in the offseason only. They are read-only now.");
    }
    ui.add_space(6.0);
    rules(app, ui, &l, live);
    ui.separator();
    books(app, ui, &l);
}

/// The rules as the commissioner edits them, and the Apply button.
fn rules(app: &mut App, ui: &mut egui::Ui, l: &LeagueView, live: bool) {
    let mut r = app.state.rules.unwrap_or(l.rules);
    let lim = l.limits;
    ui.strong("Rules");
    egui::Grid::new("rules_grid")
        .num_columns(2)
        .spacing([12.0, 6.0])
        .show(ui, |ui| {
            ui.label("Payroll floor");
            ui.add_enabled(
                live,
                egui::Slider::new(
                    &mut r.payroll_floor,
                    lim.payroll_floor.0..=lim.payroll_floor.1,
                )
                .suffix(" M"),
            );
            ui.end_row();

            ui.label("Payroll cap");
            ui.add_enabled(
                live,
                egui::Slider::new(&mut r.payroll_cap, lim.payroll_cap.0..=lim.payroll_cap.1)
                    .suffix(" M"),
            );
            ui.end_row();

            ui.label("Revenue sharing");
            ui.add_enabled(
                live,
                egui::Slider::new(
                    &mut r.revenue_sharing,
                    lim.revenue_sharing.0..=lim.revenue_sharing.1,
                ),
            );
            ui.end_row();

            ui.label("Paid per win above replacement");
            ui.add_enabled(
                live,
                egui::Slider::new(&mut r.war_price, lim.war_price.0..=lim.war_price.1).suffix(" M"),
            );
            ui.end_row();

            ui.label("Draft rounds");
            ui.add_enabled(live, egui::Slider::new(&mut r.draft_rounds, 1..=30));
            ui.end_row();

            ui.label("International rounds");
            ui.add_enabled(live, egui::Slider::new(&mut r.international_rounds, 0..=15));
            ui.end_row();
        });
    app.state.rules = Some(r);
    ui.label(
        "A rule moves at most 10% from last year's value in a year. Revenue sharing moves at most 0.04.",
    );
    ui.horizontal(|ui| {
        if ui
            .add_enabled(live, egui::Button::new("Apply rules"))
            .clicked()
        {
            app.act(Action::SetRules { rules: r });
        }
    });
}

/// Every club's books, with the league totals under the table.
fn books(app: &mut App, ui: &mut egui::Ui, l: &LeagueView) {
    let mut sort = app.state.books_sort;
    let mut rows = l.clubs.clone();
    rows.sort_by(|a, b| sort.order(a, b));
    ui.strong("League books");
    egui::ScrollArea::vertical()
        .id_salt("league_books")
        .max_height(420.0)
        .show(ui, |ui| {
            egui::Grid::new("league_table")
                .num_columns(8)
                .striped(true)
                .spacing([12.0, 4.0])
                .show(ui, |ui| {
                    header(ui, &mut sort, BooksCol::Name, "Club");
                    header(ui, &mut sort, BooksCol::Market, "Market");
                    header(ui, &mut sort, BooksCol::Payroll, "Payroll");
                    header(ui, &mut sort, BooksCol::Cash, "Cash");
                    header(ui, &mut sort, BooksCol::Revenue, "Revenue");
                    header(ui, &mut sort, BooksCol::Ticket, "Ticket");
                    header(ui, &mut sort, BooksCol::Attendance, "Crowd");
                    header(ui, &mut sort, BooksCol::Budget, "Budget");
                    ui.end_row();
                    for c in &rows {
                        club_row(ui, c);
                    }
                    ui.strong("League");
                    ui.label("");
                    ui.label(money(l.total_payroll));
                    ui.label(money(l.total_cash));
                    ui.label(money(l.total_revenue_prev));
                    ui.label("");
                    ui.label("");
                    ui.label("");
                    ui.end_row();
                });
        });
    ui.label(format!(
        "Median payroll {}, median cash {}. Clubs with negative cash: {} (shown in red).",
        money(l.median_payroll),
        money(l.median_cash),
        l.low_cash
    ));
    app.state.books_sort = sort;
}

/// One club's row. A club with negative cash is shown in red.
fn club_row(ui: &mut egui::Ui, c: &ClubBooksRow) {
    let low = c.cash < 0.0;
    let text = if low { RED } else { ui.visuals().text_color() };
    ui.colored_label(text, c.name.as_str());
    ui.label(money(c.market));
    ui.label(money(c.payroll));
    ui.colored_label(text, money(c.cash));
    ui.label(money(c.revenue_prev));
    ui.label(format!("${:.2}", c.ticket));
    ui.label(format!("{:.1}k", c.attendance_prev));
    ui.label(money(c.budget));
    ui.end_row();
}

/// A clickable column header. The column being sorted shows its direction.
fn header(ui: &mut egui::Ui, sort: &mut BooksSort, col: BooksCol, name: &str) {
    let mark = match (sort.col == col, sort.desc) {
        (true, true) => " v",
        (true, false) => " ^",
        _ => "",
    };
    let b = egui::Button::new(format!("{name}{mark}")).frame(false);
    if ui.add(b).clicked() {
        sort.click(col);
    }
}
