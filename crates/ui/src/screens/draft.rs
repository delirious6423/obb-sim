//! The amateur draft: the clock, the board and the picks made so far.

use host::api::{Action, Advance};

use crate::app::App;
use crate::widgets::{Col, money, player_table};

const COLS: &[Col] = &[Col::Name, Col::Pos, Col::Age, Col::Now, Col::Peak];

pub fn show(app: &mut App, ui: &mut egui::Ui) {
    let Some(d) = app.data.draft.clone() else {
        ui.spinner();
        return;
    };
    let busy = app.busy();
    ui.heading(format!("Draft {}", d.year));
    if !d.open {
        ui.label("The draft is not open.");
    } else {
        let amber = egui::Color32::from_rgb(230, 180, 60);
        ui.label(format!(
            "Pick {} of {}{}",
            d.pick + 1,
            d.picks,
            if d.international {
                " · international"
            } else {
                ""
            }
        ));
        ui.label(format!(
            "On the clock: {}",
            d.clock.as_deref().unwrap_or("nobody")
        ));
        if d.on_the_clock {
            ui.colored_label(
                amber,
                format!("You are on the clock. Ceiling bonus {}", money(d.ceiling)),
            );
        } else {
            ui.label(format!("Ceiling bonus {}", money(d.ceiling)));
        }
        ui.horizontal(|ui| {
            if ui
                .add_enabled(!busy && d.on_the_clock, egui::Button::new("Pass"))
                .clicked()
            {
                app.act(Action::PassPick);
            }
            if ui
                .add_enabled(!busy, egui::Button::new("Sim to my pick"))
                .clicked()
            {
                app.advance(Advance::SimDraft);
            }
        });
        ui.separator();
        let on_clock = !busy && d.on_the_clock;
        let mut drafting = None;
        let clicked = player_table(
            ui,
            "draft_board",
            &d.prospects,
            COLS,
            &mut app.state.draft_sort,
            |ui, p| {
                ui.horizontal(|ui| {
                    ui.label(format!("demand {}", money(p.demand)));
                    if ui
                        .add_enabled(on_clock, egui::Button::new("Draft"))
                        .clicked()
                    {
                        drafting = Some(p.id);
                    }
                });
            },
        );
        if let Some(id) = clicked {
            app.open_player(id);
        }
        if let Some(player) = drafting {
            app.act(Action::Draft { player });
        }
    }
    ui.separator();
    ui.strong("Picks so far, newest first");
    egui::ScrollArea::vertical()
        .id_salt("draft_made")
        .show(ui, |ui| {
            for (n, club, player) in d.made.iter().rev() {
                ui.label(format!("#{} · {club}: {player}", *n + 1));
            }
        });
}
