//! The desktop front end. It talks to the league only through the host
//! protocol (`host::api` and `GameClient`); it never sees the simulation.

mod app;
mod screens;
mod widgets;

/// `game --self-test`: found a small league, play 20 days and a box score
/// request through the protocol, with no window. For build checks.
fn self_test() -> Result<(), String> {
    use host::GameClient;
    use host::api::{Advance, Difficulty, NewGame, Preset, Request, Response, Roles};
    let c = host::LocalClient::start(host::calibration(), None);
    let new = Request::NewGame(NewGame {
        seed: 1,
        club: 0,
        difficulty: Difficulty::Preset(Preset::Rookie),
        burn_in_years: 0,
        roles: Default::default(),
    });
    for req in [
        new,
        Request::Advance(Advance::SimDays(20)),
        Request::Standings,
    ] {
        if let Response::Failed(e) = c.call(req) {
            return Err(e);
        }
    }
    // The owner's desk and the league office, through a second client. Each
    // seat starts its own league, then reads the desk it holds.
    let seats = host::LocalClient::start(host::calibration(), None);
    let new_seat = |roles: Roles| {
        Request::NewGame(NewGame {
            seed: 1,
            club: 0,
            difficulty: Difficulty::Preset(Preset::Rookie),
            burn_in_years: 0,
            roles,
        })
    };
    let owner = Roles {
        gm: true,
        owner: true,
        commissioner: false,
    };
    let commissioner = Roles {
        gm: true,
        owner: false,
        commissioner: true,
    };
    for req in [
        new_seat(owner),
        Request::OwnerDesk,
        new_seat(commissioner),
        Request::LeagueOffice,
    ] {
        if let Response::Failed(e) = seats.call(req) {
            return Err(e);
        }
    }
    println!("self-test ok");
    Ok(())
}

fn main() -> eframe::Result {
    if std::env::args().any(|a| a == "--self-test") {
        if let Err(e) = self_test() {
            eprintln!("self-test failed: {e}");
            std::process::exit(1);
        }
        return Ok(());
    }
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Baseball Sim")
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([900.0, 560.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Baseball Sim",
        options,
        Box::new(|_cc| Ok(Box::new(app::App::new()))),
    )
}
