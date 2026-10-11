//! Trades: answer offers, see the ones out, and propose a deal to a partner club.

use host::api::{Action, Advance, Club, OfferView, PlayerRow};

use crate::app::App;
use crate::screens::advance_button;
use crate::widgets::{money, pos_name};

pub fn show(app: &mut App, ui: &mut egui::Ui) {
    let Some(t) = app.data.trades.clone() else {
        ui.spinner();
        return;
    };
    let own = app.own_club();
    let roster: Vec<PlayerRow> = app
        .data
        .rosters
        .get(&own)
        .map(|r| r.players.clone())
        .unwrap_or_default();
    let busy = app.busy();

    ui.heading("Trades");
    if !t.open {
        ui.colored_label(
            egui::Color32::from_rgb(230, 180, 60),
            "Trading is closed. Offers can be read, but no new deal can be proposed.",
        );
    }
    advance_button(
        app,
        ui,
        Advance::TradeRound,
        "Other clubs hold a trade round",
    );
    ui.separator();

    // (a) Offers to the person's club.
    let mut response = None;
    ui.strong("Incoming offers");
    if t.incoming.is_empty() {
        ui.label("None.");
    }
    for o in &t.incoming {
        offer_card(ui, o, |ui| {
            if ui.add_enabled(!busy, egui::Button::new("Accept")).clicked() {
                response = Some((o.id, true));
            }
            if ui
                .add_enabled(!busy, egui::Button::new("Decline"))
                .clicked()
            {
                response = Some((o.id, false));
            }
        });
    }
    if let Some((offer, accept)) = response {
        app.act(Action::RespondTrade { offer, accept });
    }

    // (b) Offers the person's club has out. The view does not name the partner.
    ui.add_space(6.0);
    ui.strong("Offers you have out");
    if t.outgoing.is_empty() {
        ui.label("None.");
    }
    for o in &t.outgoing {
        offer_card(ui, o, |ui| {
            ui.label(format!("Offer {} (waiting)", o.id));
        });
    }

    // (c) The proposal builder.
    ui.separator();
    ui.strong("Propose a trade");
    let clubs = app.state.clubs.clone();
    let before = app.state.trade_partner;
    let cur = clubs.get(usize::from(before)).cloned().unwrap_or_default();
    ui.horizontal(|ui| {
        ui.label("Partner");
        egui::ComboBox::from_id_salt("trade_partner")
            .selected_text(cur)
            .show_ui(ui, |ui| {
                for (i, name) in clubs.iter().enumerate() {
                    let c = i as u8;
                    if c != own {
                        ui.selectable_value(&mut app.state.trade_partner, c, name);
                    }
                }
            });
    });
    if app.state.trade_partner != before {
        // The market list is per partner, so the picks to get no longer apply.
        app.state.trade_get.clear();
    }
    let partner: Club = app.state.trade_partner;
    let market: Vec<PlayerRow> = t
        .market
        .iter()
        .filter(|p| p.club == Some(partner))
        .cloned()
        .collect();

    ui.columns(2, |cols| {
        cols[0].label("You give (your roster)");
        egui::ScrollArea::vertical()
            .id_salt("trade_give")
            .max_height(260.0)
            .show(&mut cols[0], |ui| {
                for p in &roster {
                    let mut on = app.state.trade_give.contains(&p.id);
                    if ui.checkbox(&mut on, player_label(p)).changed() {
                        set_member(&mut app.state.trade_give, p.id, on);
                    }
                }
            });
        cols[1].label("You get (partner's players)");
        egui::ScrollArea::vertical()
            .id_salt("trade_get")
            .max_height(260.0)
            .show(&mut cols[1], |ui| {
                if market.is_empty() {
                    ui.label("No players listed for this club.");
                }
                for p in &market {
                    let mut on = app.state.trade_get.contains(&p.id);
                    if ui.checkbox(&mut on, player_label(p)).changed() {
                        set_member(&mut app.state.trade_get, p.id, on);
                    }
                }
            });
    });

    ui.add_space(6.0);
    let can = t.open
        && !busy
        && partner != own
        && !(app.state.trade_give.is_empty() && app.state.trade_get.is_empty());
    if partner == own {
        ui.label("Pick a club other than your own.");
    }
    if ui.add_enabled(can, egui::Button::new("Propose")).clicked() {
        let give = std::mem::take(&mut app.state.trade_give);
        let get = std::mem::take(&mut app.state.trade_get);
        app.act(Action::ProposeTrade { partner, give, get });
    }
}

/// One offer: who it is with, what moves each way, and its buttons.
fn offer_card(ui: &mut egui::Ui, o: &OfferView, buttons: impl FnOnce(&mut egui::Ui)) {
    ui.group(|ui| {
        ui.strong(format!("With {}", o.from_name));
        ui.label(format!("Gives: {}", names(&o.give)));
        ui.label(format!("Asks for: {}", names(&o.get)));
        ui.horizontal(buttons);
    });
}

fn names(rows: &[PlayerRow]) -> String {
    if rows.is_empty() {
        return "nothing".into();
    }
    rows.iter()
        .map(|p| format!("{} ({})", p.name, money(p.salary)))
        .collect::<Vec<_>>()
        .join(", ")
}

fn player_label(p: &PlayerRow) -> String {
    format!(
        "{} · {} · now {} · {}",
        p.name,
        pos_name(p.position),
        p.grade_now,
        money(p.salary)
    )
}

/// Add or remove a player id from a checkbox list.
fn set_member(list: &mut Vec<u32>, id: u32, on: bool) {
    if on {
        if !list.contains(&id) {
            list.push(id);
        }
    } else {
        list.retain(|x| *x != id);
    }
}
