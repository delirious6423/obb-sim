//! End-to-end tests of [`LocalStore`] through the public API only.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use sha2::{Digest, Sha256};
use storage::{
    FORMAT_VERSION, LocalStore, Manifest, SaveData, SaveKind, SaveStore, SlotId, StoreError,
};

fn manifest(saved_at_unix: u64) -> Manifest {
    Manifest {
        format_version: 0,
        engine_version: "0.1.0".into(),
        league_name: "Test League".into(),
        team_name: "Fictional Hawks".into(),
        role: "gm".into(),
        in_game_date: "2031-04-01".into(),
        difficulty: "normal".into(),
        saved_at_unix,
        kind: SaveKind::Manual,
        thumbnail_png: Some(vec![0x89, 0x50, 0x4e, 0x47]),
        checksum_hex: String::new(),
    }
}

fn data(seed: u8) -> SaveData {
    let mut meta = BTreeMap::new();
    meta.insert("seed".to_string(), seed.to_string());
    SaveData {
        snapshot: vec![seed; 256],
        snapshot_seq: 3,
        log: vec![vec![seed, 1], vec![seed, 2, 2]],
        results: vec![vec![seed, 9]],
        meta,
    }
}

fn slot(name: &str) -> SlotId {
    SlotId::new(name).expect("valid test slot name")
}

fn flip_byte(path: &Path) {
    let mut bytes = fs::read(path).expect("read file");
    let middle = bytes.len() / 2;
    bytes[middle] ^= 0xff;
    fs::write(path, bytes).expect("write file");
}

fn open_store() -> (tempfile::TempDir, LocalStore) {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = LocalStore::open(dir.path().join("saves")).expect("open store");
    (dir, store)
}

#[test]
fn slot_names_are_validated() {
    for bad in ["../x", "", "a/b", "a.b", &"x".repeat(65), "tab\tname", "é"] {
        assert!(
            matches!(SlotId::new(bad), Err(StoreError::BadSlotName { .. })),
            "{bad:?} must be rejected"
        );
    }
    let longest = "x".repeat(64);
    assert_eq!(
        SlotId::new(longest.as_str())
            .expect("64 is allowed")
            .as_str(),
        longest
    );
    assert!(SlotId::new("Save 1-a_B").is_ok());
    assert!(
        serde_json::from_str::<SlotId>("\"../x\"").is_err(),
        "deserialise must validate"
    );
}

#[test]
fn round_trip_snapshot_log_results_and_meta() {
    let (_dir, store) = open_store();
    let input = data(7);
    store
        .save(&slot("manual one"), manifest(100), &input)
        .expect("save");

    let loaded = store.load(&slot("manual one")).expect("load");

    assert_eq!(loaded.data, input);
    assert!(!loaded.used_backup);
    assert_eq!(loaded.manifest.format_version, FORMAT_VERSION);
    assert_eq!(loaded.manifest.saved_at_unix, 100);
    assert_eq!(
        loaded.manifest.thumbnail_png,
        Some(vec![0x89, 0x50, 0x4e, 0x47])
    );
    assert_eq!(loaded.manifest.checksum_hex.len(), 64);
}

#[test]
fn list_reads_manifests_without_sav_and_load_flags_problem() {
    let (_dir, store) = open_store();
    store.save(&slot("a"), manifest(1), &data(1)).expect("save");
    fs::remove_file(store.dir().join("a.sav")).expect("remove sav");

    let listed = store.list().expect("list");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, slot("a"));
    assert!(listed[0].manifest.is_some(), "manifest is still readable");
    assert!(listed[0].problem.is_some(), "missing sav is flagged");
    assert!(store.read_manifest(&slot("a")).is_ok());
    assert!(store.load(&slot("a")).is_err());
}

#[test]
fn list_flags_broken_manifest_and_sorts_by_name() {
    let (_dir, store) = open_store();
    store
        .save(&slot("zeta"), manifest(1), &data(1))
        .expect("save");
    store
        .save(&slot("alpha"), manifest(2), &data(2))
        .expect("save");
    fs::write(store.dir().join("broken.manifest.json"), b"{not json").expect("write");

    let listed = store.list().expect("list must not fail");
    let names: Vec<&str> = listed.iter().map(|s| s.id.as_str()).collect();
    assert_eq!(names, ["alpha", "broken", "zeta"]);
    let broken = &listed[1];
    assert!(broken.manifest.is_none());
    assert!(broken.problem.is_some());
    assert!(listed[0].problem.is_none());
}

#[test]
fn overwrite_keeps_backup_and_corrupt_sav_falls_back() {
    let (_dir, store) = open_store();
    let first = data(1);
    store
        .save(&slot("s"), manifest(10), &first)
        .expect("first save");
    store
        .save(&slot("s"), manifest(20), &data(2))
        .expect("second save");
    assert!(store.dir().join("s.sav.bak").exists());
    assert!(store.dir().join("s.manifest.json.bak").exists());

    flip_byte(&store.dir().join("s.sav"));
    let loaded = store.load(&slot("s")).expect("backup must load");

    assert!(loaded.used_backup);
    assert_eq!(loaded.data, first);
    assert_eq!(loaded.manifest.saved_at_unix, 10);
}

#[test]
fn both_copies_corrupt_gives_corrupt_error() {
    let (_dir, store) = open_store();
    store
        .save(&slot("s"), manifest(10), &data(1))
        .expect("first save");
    store
        .save(&slot("s"), manifest(20), &data(2))
        .expect("second save");
    flip_byte(&store.dir().join("s.sav"));
    flip_byte(&store.dir().join("s.sav.bak"));

    let err = store.load(&slot("s")).expect_err("both corrupt");

    assert!(matches!(err, StoreError::Corrupt(_)), "got {err:?}");
}

#[test]
fn corrupt_current_save_does_not_replace_good_backup() {
    let (_dir, store) = open_store();
    let first = data(1);
    store
        .save(&slot("s"), manifest(10), &first)
        .expect("first save");
    store
        .save(&slot("s"), manifest(20), &data(2))
        .expect("second save");
    flip_byte(&store.dir().join("s.sav"));

    store
        .save(&slot("s"), manifest(30), &data(3))
        .expect("third save");

    let loaded = store.load(&slot("s")).expect("load");
    assert_eq!(loaded.data, data(3));
    flip_byte(&store.dir().join("s.sav"));
    let fallback = store.load(&slot("s")).expect("backup must still be good");
    assert!(fallback.used_backup);
    assert_eq!(
        fallback.data, first,
        "good backup must survive the third save"
    );
}

#[test]
fn newer_format_is_refused_and_files_are_untouched() {
    let (_dir, store) = open_store();
    store
        .save(&slot("future"), manifest(1), &data(1))
        .expect("save");
    let manifest_path = store.dir().join("future.manifest.json");
    let mut value: serde_json::Value =
        serde_json::from_slice(&fs::read(&manifest_path).expect("read")).expect("json");
    value["format_version"] = serde_json::json!(FORMAT_VERSION + 1);
    fs::write(
        &manifest_path,
        serde_json::to_vec_pretty(&value).expect("json"),
    )
    .expect("write");
    let sav_before = fs::read(store.dir().join("future.sav")).expect("read sav");
    let manifest_before = fs::read(&manifest_path).expect("read manifest");

    let save_err = store
        .save(&slot("future"), manifest(2), &data(2))
        .expect_err("save refused");
    assert!(
        matches!(save_err, StoreError::NewerVersion { .. }),
        "got {save_err:?}"
    );
    let load_err = store.load(&slot("future")).expect_err("load refused");
    assert!(
        matches!(load_err, StoreError::NewerVersion { .. }),
        "got {load_err:?}"
    );

    assert_eq!(
        fs::read(store.dir().join("future.sav")).expect("read"),
        sav_before
    );
    assert_eq!(fs::read(&manifest_path).expect("read"), manifest_before);
    assert!(!store.dir().join("future.sav.bak").exists());
}

#[test]
fn autosave_rotates_through_three_slots_oldest_first() {
    let (_dir, store) = open_store();
    let mut used = Vec::new();
    for ts in 1..=5u64 {
        let id = store
            .autosave(manifest(ts), &data(ts as u8))
            .expect("autosave");
        used.push(id.as_str().to_string());
    }
    assert_eq!(used, ["auto-1", "auto-2", "auto-3", "auto-1", "auto-2"]);

    let listed = store.list().expect("list");
    let stamps: Vec<(String, u64)> = listed
        .iter()
        .map(|s| {
            let m = s.manifest.as_ref().expect("manifest");
            assert_eq!(m.kind, SaveKind::Auto);
            (s.id.as_str().to_string(), m.saved_at_unix)
        })
        .collect();
    assert_eq!(
        stamps,
        [
            ("auto-1".into(), 4),
            ("auto-2".into(), 5),
            ("auto-3".into(), 3)
        ]
    );
}

#[test]
fn autosave_slot_count_is_configurable() {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = LocalStore::open(dir.path())
        .expect("open")
        .with_auto_slots(2)
        .expect("cfg");
    let used: Vec<String> = (1..=3u64)
        .map(|ts| {
            store
                .autosave(manifest(ts), &data(1))
                .expect("autosave")
                .as_str()
                .to_string()
        })
        .collect();
    assert_eq!(used, ["auto-1", "auto-2", "auto-1"]);
    assert!(
        LocalStore::open(dir.path())
            .expect("open")
            .with_auto_slots(0)
            .is_err()
    );
}

#[test]
fn quicksave_uses_quick_slot_and_quick_kind() {
    let (_dir, store) = open_store();
    let id = store.quicksave(manifest(9), &data(9)).expect("quicksave");
    assert_eq!(id.as_str(), "quick");
    assert_eq!(
        store.read_manifest(&id).expect("manifest").kind,
        SaveKind::Quick
    );
}

#[test]
fn leftover_tmp_files_are_ignored_and_cleaned_on_save() {
    let (_dir, store) = open_store();
    store
        .save(&slot("crash"), manifest(1), &data(1))
        .expect("save");
    let dir = store.dir().to_path_buf();
    fs::write(dir.join("crash.sav.tmp"), b"half written").expect("write");
    fs::write(dir.join("crash.manifest.json.tmp"), b"{half").expect("write");
    fs::write(dir.join("junk.sav.tmp"), b"orphan").expect("write");

    let listed = store.list().expect("list");
    assert_eq!(listed.len(), 1, "tmp files are not slots");
    assert_eq!(store.load(&slot("crash")).expect("load").data, data(1));

    store
        .save(&slot("crash"), manifest(2), &data(2))
        .expect("save over leftovers");
    assert!(!dir.join("crash.sav.tmp").exists());
    assert!(!dir.join("crash.manifest.json.tmp").exists());
    assert_eq!(store.load(&slot("crash")).expect("load").data, data(2));
}

#[test]
fn delete_removes_every_file_of_the_slot() {
    let (_dir, store) = open_store();
    store.save(&slot("d"), manifest(1), &data(1)).expect("save");
    store.save(&slot("d"), manifest(2), &data(2)).expect("save");
    store.delete(&slot("d")).expect("delete");

    assert!(store.list().expect("list").is_empty());
    assert!(matches!(
        store.load(&slot("d")),
        Err(StoreError::NotFound(_))
    ));
    assert!(matches!(
        store.delete(&slot("d")),
        Err(StoreError::NotFound(_))
    ));
    let left: Vec<_> = fs::read_dir(store.dir()).expect("read dir").collect();
    assert!(left.is_empty(), "no files left: {left:?}");
}

#[test]
fn large_blobs_round_trip() {
    let (_dir, store) = open_store();
    let big = SaveData {
        snapshot: (0..5_000_000u32).map(|i| (i % 251) as u8).collect(),
        snapshot_seq: 10_000,
        log: (0..10_000u32).map(|i| i.to_le_bytes().to_vec()).collect(),
        results: Vec::new(),
        meta: BTreeMap::new(),
    };
    store.save(&slot("big"), manifest(1), &big).expect("save");
    let loaded = store.load(&slot("big")).expect("load");
    assert_eq!(loaded.data.snapshot.len(), 5_000_000);
    assert_eq!(loaded.data.log.len(), 10_000);
    assert_eq!(loaded.data, big);
}

#[test]
fn version_zero_file_without_results_table_is_migrated_on_load() {
    let (_dir, store) = open_store();
    let dir = store.dir().to_path_buf();
    {
        let conn = rusqlite::Connection::open(dir.join("old.sav")).expect("create");
        conn.execute_batch(
            "CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
             CREATE TABLE snapshot (id INTEGER PRIMARY KEY CHECK (id = 1), seq INTEGER NOT NULL, bytes BLOB NOT NULL);
             CREATE TABLE log (seq INTEGER PRIMARY KEY, bytes BLOB NOT NULL);
             INSERT INTO snapshot (id, seq, bytes) VALUES (1, 3, x'0102');
             INSERT INTO log (seq, bytes) VALUES (3, x'aa');",
        )
        .expect("v0 layout");
        conn.close().expect("close");
    }
    let sav_bytes = fs::read(dir.join("old.sav")).expect("read");
    let mut old = manifest(5);
    old.format_version = 0;
    old.checksum_hex = Sha256::digest(&sav_bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    fs::write(
        dir.join("old.manifest.json"),
        serde_json::to_vec(&old).expect("json"),
    )
    .expect("w");

    let loaded = store.load(&slot("old")).expect("v0 must load");

    assert_eq!(loaded.data.snapshot, vec![1, 2]);
    assert_eq!(loaded.data.snapshot_seq, 3);
    assert_eq!(loaded.data.log, vec![vec![0xaa]]);
    assert!(loaded.data.results.is_empty());
    assert!(!loaded.used_backup);
    assert_eq!(
        fs::read(dir.join("old.sav")).expect("read"),
        sav_bytes,
        "not modified"
    );
    assert!(!dir.join("old.sav.load.tmp").exists());
}

#[test]
fn missing_slot_is_not_found() {
    let (_dir, store) = open_store();
    assert!(matches!(
        store.load(&slot("nobody")),
        Err(StoreError::NotFound(_))
    ));
    assert!(matches!(
        store.read_manifest(&slot("nobody")),
        Err(StoreError::NotFound(_))
    ));
}

#[test]
fn store_is_usable_as_trait_object() {
    let (_dir, store) = open_store();
    let boxed: Box<dyn SaveStore> = Box::new(store);
    boxed
        .save(&slot("dyn"), manifest(1), &data(1))
        .expect("save");
    assert_eq!(boxed.load(&slot("dyn")).expect("load").data, data(1));
}
