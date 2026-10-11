//! Phase 5 exit gate: a scripted front end plays whole seasons through the
//! public protocol only (requests and responses, no access to the league),
//! saves, reloads, and gets exactly the results the uninterrupted game gets.

use host::api::{
    Action, Advance, Difficulty, NewGame, Overview, Preset, Request, Response, Stage, Standings,
};
use host::{GameClient, LocalClient};

fn client(dir: &std::path::Path) -> LocalClient {
    LocalClient::start(host::calibration(), Some(dir.to_path_buf()))
}

fn ok(r: Response) -> String {
    match r {
        Response::Done(m) => m,
        other => panic!("expected Done, got {other:?}"),
    }
}

fn overview(c: &impl GameClient) -> Overview {
    match c.call(Request::Overview) {
        Response::Overview(o) => o,
        other => panic!("expected an overview, got {other:?}"),
    }
}

fn standings(c: &impl GameClient) -> Standings {
    match c.call(Request::Standings) {
        Response::Standings(s) => s,
        other => panic!("expected standings, got {other:?}"),
    }
}

fn new_game(c: &impl GameClient) {
    ok(c.call(Request::NewGame(NewGame {
        seed: 77,
        club: 4,
        difficulty: Difficulty::Preset(Preset::Veteran),
        burn_in_years: 2,
    })));
}

fn play_season(c: &impl GameClient) {
    ok(c.call(Request::Advance(Advance::SimToEndOfSeason)));
    assert_eq!(overview(c).stage, Stage::Playoffs);
}

/// Everything between the end of one season and the start of the next.
fn offseason(c: &impl GameClient) {
    ok(c.call(Request::Advance(Advance::FinishSeason)));
    assert_eq!(overview(c).stage, Stage::Draft);
    // The draft: pass when the person's club is on the clock.
    for _ in 0..400 {
        ok(c.call(Request::Advance(Advance::SimDraft)));
        let o = overview(c);
        if o.stage != Stage::Draft {
            break;
        }
        assert!(o.on_the_clock, "the draft stopped for no reason");
        ok(c.call(Request::Act(Action::PassPick)));
    }
    assert_eq!(overview(c).stage, Stage::FreeAgency);
    ok(c.call(Request::Advance(Advance::StartNextSeason)));
    assert_eq!(overview(c).stage, Stage::Season);
}

#[test]
fn a_season_saved_and_reloaded_plays_on_identically() {
    let dir_a = tempfile_dir("a");
    let dir_b = tempfile_dir("b");

    // Game A: play straight through two seasons.
    let a = client(&dir_a);
    new_game(&a);
    let first_day = overview(&a);
    assert_eq!((first_day.day, first_day.stage), (0, Stage::Season));
    play_season(&a);
    let after_one = standings(&a);
    offseason(&a);
    play_season(&a);
    let a_end = standings(&a);

    // Game B: the same, but saved after season one and reloaded into a fresh
    // client (a fresh host thread) before season two.
    let b = client(&dir_b);
    new_game(&b);
    play_season(&b);
    assert_eq!(standings(&b), after_one);
    offseason(&b);
    ok(b.call(Request::Save {
        slot: "gate".into(),
    }));
    drop(b);
    let b2 = client(&dir_b);
    let saves = match b2.call(Request::ListSaves) {
        Response::Saves(s) => s,
        other => panic!("expected saves, got {other:?}"),
    };
    assert!(
        saves
            .iter()
            .any(|s| s.slot == "gate" && s.problem.is_none())
    );
    ok(b2.call(Request::Load {
        slot: "gate".into(),
    }));
    assert_eq!(overview(&b2).stage, Stage::Season);
    play_season(&b2);
    assert_eq!(standings(&b2), a_end, "the reloaded game diverged");
}

#[test]
fn a_long_request_can_be_cancelled_and_resumed() {
    let dir = tempfile_dir("cancel");
    let c = client(&dir);
    new_game(&c);
    let t = c.submit(Request::Advance(Advance::SimToEndOfSeason));
    // Wait until it is running, then ask it to stop.
    while !c.progress().running {
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    c.cancel();
    let r = loop {
        if let Some(r) = c.poll(t) {
            break r;
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    };
    assert!(matches!(r, Response::Done(_)), "{r:?}");
    let o = overview(&c);
    assert!(
        o.day < o.days,
        "it should have stopped early (day {})",
        o.day
    );
    // Playing on from where it stopped reaches the end.
    play_season(&c);
}

fn tempfile_dir(tag: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("sim-host-gate-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).expect("temp dir");
    d
}
