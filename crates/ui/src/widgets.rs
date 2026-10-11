//! Small helpers shared by the screens.

use host::api::{Level, PlayerRow, Position};

/// Money: the league counts in millions, e.g. `$12.5M`.
pub fn money(x: f64) -> String {
    format!("${x:.1}M")
}

pub fn level_name(l: Level) -> &'static str {
    match l {
        Level::Majors => "Majors",
        Level::Triple => "Triple-A",
        Level::Double => "Double-A",
        Level::Single => "Single-A",
    }
}

pub fn pos_name(p: Position) -> &'static str {
    match p {
        Position::Batter => "Bat",
        Position::Pitcher => "Pit",
    }
}

/// A column a player table can be sorted by.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Col {
    Name,
    Pos,
    Age,
    Level,
    Now,
    Peak,
    Salary,
    Years,
    Ask,
    Out,
    Stat,
}

impl Col {
    pub fn title(self) -> &'static str {
        match self {
            Col::Name => "Name",
            Col::Pos => "Pos",
            Col::Age => "Age",
            Col::Level => "Level",
            Col::Now => "Now",
            Col::Peak => "Peak",
            Col::Salary => "Salary",
            Col::Years => "Yrs",
            Col::Ask => "Ask",
            Col::Out => "Out",
            Col::Stat => "This season",
        }
    }
}

/// Sort state for a table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sort {
    pub col: Col,
    pub desc: bool,
}

impl Sort {
    pub fn by(col: Col, desc: bool) -> Self {
        Self { col, desc }
    }

    /// Click on a header: same column flips the direction.
    pub fn click(&mut self, col: Col) {
        if self.col == col {
            self.desc = !self.desc;
        } else {
            self.col = col;
            self.desc = !matches!(col, Col::Name | Col::Pos | Col::Level | Col::Age);
        }
    }

    pub fn apply(&self, rows: &mut [&PlayerRow]) {
        rows.sort_by(|a, b| {
            let o = match self.col {
                Col::Name => a.name.cmp(&b.name),
                Col::Pos => a.position.as_ord().cmp(&b.position.as_ord()),
                Col::Age => a.age.cmp(&b.age),
                Col::Level => a.level.cmp(&b.level),
                Col::Now => a.grade_now.cmp(&b.grade_now),
                Col::Peak => a.grade_peak.cmp(&b.grade_peak),
                Col::Salary => a.salary.total_cmp(&b.salary),
                Col::Years => a.years.cmp(&b.years),
                Col::Ask => a.ask.total_cmp(&b.ask),
                Col::Out => a.out_games.cmp(&b.out_games),
                Col::Stat => a.stat.cmp(&b.stat),
            };
            let o = if self.desc { o.reverse() } else { o };
            o.then(a.id.cmp(&b.id))
        });
    }
}

trait AsOrd {
    fn as_ord(&self) -> u8;
}

impl AsOrd for Position {
    fn as_ord(&self) -> u8 {
        match self {
            Position::Batter => 0,
            Position::Pitcher => 1,
        }
    }
}

/// A sortable table of players. Returns the id of a player whose name was
/// clicked. `extra` draws one more column per row (for buttons).
pub fn player_table(
    ui: &mut egui::Ui,
    id: &str,
    rows: &[PlayerRow],
    cols: &[Col],
    sort: &mut Sort,
    mut extra: impl FnMut(&mut egui::Ui, &PlayerRow),
) -> Option<u32> {
    use egui_extras::{Column, TableBuilder};
    let mut order: Vec<&PlayerRow> = rows.iter().collect();
    sort.apply(&mut order);
    let mut clicked = None;
    let mut b = TableBuilder::new(ui)
        .id_salt(id)
        .striped(true)
        .resizable(true)
        .cell_layout(egui::Layout::left_to_right(egui::Align::Center));
    for c in cols {
        b = b.column(if *c == Col::Name {
            Column::auto().at_least(150.0)
        } else {
            Column::auto().at_least(44.0)
        });
    }
    b = b.column(Column::remainder());
    b.header(22.0, |mut h| {
        for c in cols {
            h.col(|ui| {
                let arrow = if sort.col == *c {
                    if sort.desc { " ▼" } else { " ▲" }
                } else {
                    ""
                };
                if ui
                    .add(egui::Button::new(format!("{}{arrow}", c.title())).frame(false))
                    .clicked()
                {
                    sort.click(*c);
                }
            });
        }
        h.col(|_| {});
    })
    .body(|body| {
        body.rows(22.0, order.len(), |mut row| {
            let p = order[row.index()];
            for c in cols {
                row.col(|ui| match c {
                    Col::Name => {
                        if ui.link(&p.name).clicked() {
                            clicked = Some(p.id);
                        }
                    }
                    Col::Pos => {
                        ui.label(pos_name(p.position));
                    }
                    Col::Age => {
                        ui.label(p.age.to_string());
                    }
                    Col::Level => {
                        ui.label(level_name(p.level));
                    }
                    Col::Now => {
                        ui.label(p.grade_now.to_string());
                    }
                    Col::Peak => {
                        ui.label(p.grade_peak.to_string());
                    }
                    Col::Salary => {
                        ui.label(money(p.salary));
                    }
                    Col::Years => {
                        ui.label(p.years.to_string());
                    }
                    Col::Ask => {
                        ui.label(format!("{} x{}", money(p.ask), p.ask_years));
                    }
                    Col::Out => {
                        ui.label(if p.out_games > 0 {
                            format!("{} g", p.out_games)
                        } else {
                            String::new()
                        });
                    }
                    Col::Stat => {
                        ui.label(&p.stat);
                    }
                });
            }
            row.col(|ui| extra(ui, p));
        });
    });
    clicked
}
