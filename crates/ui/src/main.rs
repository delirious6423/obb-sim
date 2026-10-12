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
    transfer_self_test()?;
    println!("self-test ok");
    Ok(())
}

/// Export a save to a folder, import the file into a second save folder, and
/// check the copy loads. Both folders are removed afterwards.
fn transfer_self_test() -> Result<(), String> {
    use host::GameClient;
    use host::api::{Difficulty, NewGame, Preset, Request, Response};
    let root = std::env::temp_dir().join(format!("bsim-self-test-{}", std::process::id()));
    let saves = root.join("saves");
    let out = root.join("out");
    let result = (|| {
        std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
        let src = host::LocalClient::start(host::calibration(), Some(saves.clone()));
        let new = Request::NewGame(NewGame {
            seed: 1,
            club: 0,
            difficulty: Difficulty::Preset(Preset::Rookie),
            burn_in_years: 0,
            roles: Default::default(),
        });
        expect_done(src.call(new))?;
        expect_done(src.call(Request::Save {
            slot: "selftest".into(),
        }))?;
        let exported = expect_done(src.call(Request::ExportSave {
            slot: "selftest".into(),
            folder: out.display().to_string(),
        }))?;
        if !exported.starts_with("Exported to ") {
            return Err(format!("export: unexpected answer \"{exported}\""));
        }
        let file = out.join("selftest.bsave");
        let dst = host::LocalClient::start(host::calibration(), Some(root.join("dst")));
        let imported = |as_name: Option<String>| {
            dst.call(Request::ImportSave {
                path: file.display().to_string(),
                as_name,
            })
        };
        expect_done(imported(None))?;
        // A second import must not overwrite the first copy.
        expect_done(imported(None))?;
        let list = match dst.call(Request::ListSaves) {
            Response::Saves(list) => list,
            other => return Err(format!("list after import: {other:?}")),
        };
        if list.len() != 2 {
            return Err(format!(
                "expected 2 saves after two imports, found {}",
                list.len()
            ));
        }
        let first = list
            .iter()
            .map(|s| s.slot.clone())
            .min()
            .unwrap_or_default();
        expect_done(dst.call(Request::Load { slot: first }))?;
        Ok(())
    })();
    let _ = std::fs::remove_dir_all(&root);
    result
}

fn expect_done(r: host::api::Response) -> Result<String, String> {
    match r {
        host::api::Response::Done(m) => Ok(m),
        host::api::Response::Failed(e) => Err(e),
        other => Err(format!("unexpected answer {other:?}")),
    }
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
