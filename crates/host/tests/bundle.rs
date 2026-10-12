//! Export and import of saves through the public protocol only.
//!
//! The tests use a scratch folder under the system temp dir. No new
//! dependency is needed for it: `ScratchDir` removes itself on drop.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use host::api::{Difficulty, NewGame, Preset, Request, Response};
use host::{GameClient, LocalClient};
use storage::{LocalStore, SaveStore, SlotId};

static COUNTER: AtomicU32 = AtomicU32::new(0);

struct ScratchDir(PathBuf);

impl ScratchDir {
    fn new(tag: &str) -> Self {
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("bsim-host-{tag}-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("scratch dir");
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for ScratchDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn client(dir: &Path) -> LocalClient {
    LocalClient::start(host::calibration(), Some(dir.to_path_buf()))
}

fn new_game(c: &impl GameClient) {
    let r = c.call(Request::NewGame(NewGame {
        seed: 91,
        club: 2,
        difficulty: Difficulty::Preset(Preset::Veteran),
        burn_in_years: 1,
        roles: Default::default(),
    }));
    assert!(matches!(r, Response::Done(_)), "new game: {r:?}");
}

fn done(r: Response) -> String {
    match r {
        Response::Done(m) => m,
        other => panic!("expected Done, got {other:?}"),
    }
}

fn failed(r: Response) -> String {
    match r {
        Response::Failed(m) => m,
        other => panic!("expected Failed, got {other:?}"),
    }
}

fn saves(c: &impl GameClient) -> Vec<String> {
    match c.call(Request::ListSaves) {
        Response::Saves(list) => {
            let mut names: Vec<String> = list.into_iter().map(|s| s.slot).collect();
            names.sort();
            names
        }
        other => panic!("expected saves, got {other:?}"),
    }
}

fn export(c: &impl GameClient, slot: &str, folder: &Path) -> String {
    done(c.call(Request::ExportSave {
        slot: slot.into(),
        folder: folder.display().to_string(),
    }))
}

fn import(c: &impl GameClient, path: &Path, as_name: Option<&str>) -> Response {
    c.call(Request::ImportSave {
        path: path.display().to_string(),
        as_name: as_name.map(str::to_string),
    })
}

/// Saves a game in `dir` and exports it to `folder`; returns the bundle path.
fn exported_game(dir: &Path, folder: &Path) -> PathBuf {
    let c = client(dir);
    new_game(&c);
    done(c.call(Request::Save {
        slot: "career".into(),
    }));
    let msg = export(&c, "career", folder);
    assert!(msg.starts_with("Exported to "), "{msg}");
    folder.join("career.bsave")
}

#[test]
fn export_then_import_then_load_gives_the_same_club() {
    let src = ScratchDir::new("src");
    let out = ScratchDir::new("out");
    let bundle = exported_game(src.path(), out.path());

    let dst = ScratchDir::new("dst");
    let c = client(dst.path());
    let msg = done(import(&c, &bundle, None));
    assert!(msg.starts_with("Imported as career"), "{msg}");
    assert_eq!(saves(&c), vec!["career".to_string()]);
    done(c.call(Request::Load {
        slot: "career".into(),
    }));
    match c.call(Request::Overview) {
        Response::Overview(o) => assert_eq!(o.club, 2),
        other => panic!("expected overview, got {other:?}"),
    }
}

#[test]
fn import_never_overwrites_and_picks_a_fresh_name() {
    let src = ScratchDir::new("src2");
    let out = ScratchDir::new("out2");
    let bundle = exported_game(src.path(), out.path());

    let dst = ScratchDir::new("dst2");
    let c = client(dst.path());
    done(import(&c, &bundle, None));
    done(import(&c, &bundle, None));
    done(import(&c, &bundle, Some("career")));
    assert_eq!(saves(&c).len(), 3);
    assert!(saves(&c).contains(&"career".to_string()));
}

#[test]
fn damaged_bundle_is_refused_and_nothing_is_saved() {
    let src = ScratchDir::new("src3");
    let out = ScratchDir::new("out3");
    let bundle = exported_game(src.path(), out.path());
    let mut bytes = std::fs::read(&bundle).expect("read");
    let middle = bytes.len() / 2;
    bytes[middle] ^= 0x10;
    let bad = out.path().join("bad.bsave");
    std::fs::write(&bad, bytes).expect("write");

    let dst = ScratchDir::new("dst3");
    let c = client(dst.path());
    let msg = failed(import(&c, &bad, None));
    assert!(msg.contains("damaged") || msg.contains("checksum"), "{msg}");
    assert!(saves(&c).is_empty());
}

#[test]
fn calibration_mismatch_is_refused_with_a_message() {
    let src = ScratchDir::new("src4");
    let out = ScratchDir::new("out4");
    let bundle = exported_game(src.path(), out.path());

    // Re-save the same league tagged with other calibration data.
    let store = LocalStore::open(src.path()).expect("open");
    let id = SlotId::new("career").expect("id");
    let loaded = store.load(&id).expect("load source");
    let mut data = loaded.data.clone();
    data.meta.insert("data".into(), "params-0".into());
    store
        .save(
            &SlotId::new("tagged").expect("id"),
            loaded.manifest.clone(),
            &data,
        )
        .expect("save tagged");
    store
        .export_to(&SlotId::new("tagged").expect("id"), out.path())
        .expect("export tagged");
    let tagged = out.path().join("tagged.bsave");
    assert!(tagged.exists());
    assert!(bundle.exists());

    let dst = ScratchDir::new("dst4");
    let c = client(dst.path());
    let msg = failed(import(&c, &tagged, None));
    assert!(msg.contains("calibration"), "{msg}");
    assert!(
        msg.contains("params-0") && msg.contains("params-1"),
        "{msg}"
    );
    assert!(saves(&c).is_empty());
}

#[test]
fn import_from_an_older_engine_says_so() {
    let src = ScratchDir::new("src5");
    let out = ScratchDir::new("out5");
    exported_game(src.path(), out.path());

    // Re-save the same league with an engine name from an older build.
    let store = LocalStore::open(src.path()).expect("open");
    let loaded = store
        .load(&SlotId::new("career").expect("id"))
        .expect("load source");
    let mut manifest = loaded.manifest.clone();
    manifest.engine_version = "sim 0.0.1 / rules 0".into();
    store
        .save(&SlotId::new("old").expect("id"), manifest, &loaded.data)
        .expect("save old engine");
    store
        .export_to(&SlotId::new("old").expect("id"), out.path())
        .expect("export");
    let old = out.path().join("old.bsave");

    let dst = ScratchDir::new("dst5");
    let c = client(dst.path());
    let msg = done(import(&c, &old, None));
    assert!(msg.contains("sim 0.0.1 / rules 0"), "{msg}");
    assert!(msg.contains("rules may differ"), "{msg}");
    assert_eq!(saves(&c).len(), 1);
}

#[test]
fn export_with_no_save_folder_fails_cleanly() {
    let c = LocalClient::start(host::calibration(), None);
    let msg = failed(c.call(Request::ExportSave {
        slot: "career".into(),
        folder: "/nonexistent".into(),
    }));
    assert!(msg.contains("no save folder"), "{msg}");
}
