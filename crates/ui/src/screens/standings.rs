//! Standings by division.

use crate::app::{App, Screen};

pub fn show(app: &mut App, ui: &mut egui::Ui) {
    let Some(s) = app.data.standings.clone() else {
        ui.spinner();
        return;
    };
    ui.heading(format!("Standings {}", s.year));
    if let Some(c) = &s.champion {
        ui.label(format!("Champions: {c}"));
    }
    let own = app.own_club();
    let mut open = None;
    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.columns(2, |cols| {
            for (i, d) in s.divisions.iter().enumerate() {
                let ui = &mut cols[i % 2];
                ui.strong(&d.name);
                egui::Grid::new(format!("div{i}"))
                    .striped(true)
                    .num_columns(6)
                    .show(ui, |ui| {
                        ui.label("Club");
                        ui.label("W");
                        ui.label("L");
                        ui.label("Pct");
                        ui.label("RS");
                        ui.label("RA");
                        ui.end_row();
                        for r in &d.rows {
                            let name = if r.club == own {
                                egui::RichText::new(&r.name).strong()
                            } else {
                                egui::RichText::new(&r.name)
                            };
                            if ui.link(name).clicked() {
                                open = Some(r.club);
                            }
                            ui.label(r.w.to_string());
                            ui.label(r.l.to_string());
                            let g = f64::from(r.w + r.l);
                            ui.label(if g > 0.0 {
                                format!("{:.3}", f64::from(r.w) / g)
                            } else {
                                "—".into()
                            });
                            ui.label(r.rs.to_string());
                            ui.label(r.ra.to_string());
                            ui.end_row();
                        }
                    });
                ui.add_space(10.0);
            }
        });
    });
    if let Some(c) = open {
        app.viewing = c;
        app.go(Screen::Roster);
    }
}
