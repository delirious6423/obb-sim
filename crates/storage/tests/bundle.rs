//! Export and import of `.bsave` files through the public API.
//!
//! Some tests build bundle bytes by hand (`make_bundle`), so the format is
//! checked against a second, independent writer and not only against itself.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use storage::{
    FORMAT_VERSION, LocalStore, Manifest, OpenedBundle, SaveData, SaveKind, SaveStore, SlotId,
    StoreError, sanitise_slot_name,
};

fn manifest(saved_at_unix: u64) -> Manifest {
    Manifest {
        format_version: 0,
        engine_version: "sim test / rules 1".into(),
        league_name: "Test League".into(),
        team_name: "Fictional Hawks".into(),
        role: "gm".into(),
        in_game_date: "2031-04-01".into(),
        difficulty: "normal".into(),
        saved_at_unix,
        kind: SaveKind::Manual,
        thumbnail_png: None,
        checksum_hex: String::new(),
        autosave_seq: 0,
    }
}

fn data(seed: u8) -> SaveData {
    let mut meta = BTreeMap::new();
    meta.insert("seed".to_string(), seed.to_string());
    meta.insert("data".to_string(), "params-1".to_string());
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

fn open_store() -> (tempfile::TempDir, LocalStore) {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = LocalStore::open(dir.path().join("saves")).expect("open store");
    (dir, store)
}

fn sha_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Independent bundle writer. It follows the layout in `bundle.rs`, so the
/// tests can produce files the exporter would never write.
fn make_bundle(hint: &str, manifest: &Manifest, sav: &[u8]) -> Vec<u8> {
    let header = serde_json::to_vec(&serde_json::json!({
        "slot": hint,
        "manifest": manifest,
    }))
    .expect("header json");
    let mut out = b"BSIMSAVE".to_vec();
    out.extend_from_slice(&1u32.to_le_bytes());
    out.extend_from_slice(&(header.len() as u64).to_le_bytes());
    out.extend_from_slice(&header);
    out.extend_from_slice(&(sav.len() as u64).to_le_bytes());
    out.extend_from_slice(sav);
    let trailer = Sha256::digest(&out);
    out.extend_from_slice(&trailer);
    out
}

/// A manifest whose checksum matches `sav`.
fn manifest_for(sav: &[u8]) -> Manifest {
    let mut m = manifest(7);
    m.format_version = FORMAT_VERSION;
    m.checksum_hex = sha_hex(sav);
    m
}

/// Saves `name` with `seed`, and returns the `.sav` bytes.
fn save_slot(store: &LocalStore, name: &str, seed: u8) -> Vec<u8> {
    store
        .save(&slot(name), manifest(1), &data(seed))
        .expect("save");
    fs::read(store.dir().join(format!("{name}.sav"))).expect("read sav")
}

fn write(path: &Path, bytes: &[u8]) -> PathBuf {
    fs::write(path, bytes).expect("write bundle");
    path.to_path_buf()
}

fn export_one(store: &LocalStore, name: &str, folder: &Path) -> PathBuf {
    store.export_to(&slot(name), folder).expect("export")
}

fn file_names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .expect("read dir")
        .map(|e| e.expect("entry").file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

#[test]
fn round_trip_restores_the_same_save_in_a_new_store() {
    let (dir, store) = open_store();
    save_slot(&store, "career", 4);
    let folder = dir.path().join("downloads");
    let path = export_one(&store, "career", &folder);
    assert_eq!(
        path.file_name().and_then(|n| n.to_str()),
        Some("career.bsave")
    );

    let (_other_dir, other) = open_store();
    let name = other.import_file(&path, None).expect("import");
    assert_eq!(name.as_str(), "career");

    let back = other.load(&name).expect("load imported");
    assert_eq!(back.data, data(4));
    assert_eq!(back.manifest.league_name, "Test League");
    assert_eq!(back.manifest.engine_version, "sim test / rules 1");
    assert_eq!(back.manifest.format_version, FORMAT_VERSION);
    assert!(!back.used_backup);
}

#[test]
fn export_never_overwrites_an_existing_file() {
    let (dir, store) = open_store();
    save_slot(&store, "career", 1);
    let folder = dir.path().join("out");
    let first = export_one(&store, "career", &folder);
    let second = export_one(&store, "career", &folder);
    assert_ne!(first, second);
    assert_eq!(
        file_names(&folder),
        vec!["career-2.bsave".to_string(), "career.bsave".to_string()]
    );
}

#[test]
fn export_refuses_a_damaged_slot() {
    let (dir, store) = open_store();
    save_slot(&store, "career", 1);
    let sav = store.dir().join("career.sav");
    let mut bytes = fs::read(&sav).expect("read");
    let middle = bytes.len() / 2;
    bytes[middle] ^= 0xff;
    fs::write(&sav, bytes).expect("write");
    let folder = dir.path().join("out");
    let err = store
        .export_to(&slot("career"), &folder)
        .expect_err("damaged export refused");
    assert!(matches!(err, StoreError::Corrupt(_)), "got {err:?}");
    assert!(!folder.join("career.bsave").exists());
}

#[test]
fn corrupted_checksum_is_refused() {
    let (dir, store) = open_store();
    save_slot(&store, "career", 2);
    let path = export_one(&store, "career", dir.path());
    let mut bytes = fs::read(&path).expect("read");
    let middle = bytes.len() / 2;
    bytes[middle] ^= 0x01;
    write(&path, &bytes);

    let (_other_dir, other) = open_store();
    let err = other.import_file(&path, None).expect_err("refused");
    assert!(
        matches!(err, StoreError::Bundle(ref m) if m.contains("checksum")),
        "got {err:?}"
    );
    assert!(other.list().expect("list").is_empty());
}

#[test]
fn trailer_only_damage_is_refused() {
    let (dir, store) = open_store();
    save_slot(&store, "career", 2);
    let path = export_one(&store, "career", dir.path());
    let mut bytes = fs::read(&path).expect("read");
    let last = bytes.len() - 1;
    bytes[last] ^= 0x80;
    write(&path, &bytes);
    let err = OpenedBundle::read(&path).expect_err("refused");
    assert!(matches!(err, StoreError::Bundle(_)), "got {err:?}");
}

#[test]
fn truncated_file_is_refused() {
    let (dir, store) = open_store();
    save_slot(&store, "career", 2);
    let path = export_one(&store, "career", dir.path());
    let bytes = fs::read(&path).expect("read");
    for cut in [1, 10, bytes.len() / 2, bytes.len() - 1] {
        write(&path, &bytes[..bytes.len() - cut]);
        let (_other_dir, other) = open_store();
        let err = other.import_file(&path, None).expect_err("refused");
        assert!(
            matches!(err, StoreError::Bundle(_)),
            "cut {cut}: got {err:?}"
        );
        assert!(other.list().expect("list").is_empty());
    }
    write(&path, &bytes[..4]);
    let err = OpenedBundle::read(&path).expect_err("refused");
    assert!(matches!(err, StoreError::Bundle(_)), "got {err:?}");
}

#[test]
fn trailing_bytes_are_refused() {
    let (dir, store) = open_store();
    save_slot(&store, "career", 2);
    let path = export_one(&store, "career", dir.path());
    let mut bytes = fs::read(&path).expect("read");
    bytes.push(0);
    write(&path, &bytes);
    let err = OpenedBundle::read(&path).expect_err("refused");
    assert!(matches!(err, StoreError::Bundle(_)), "got {err:?}");
}

#[test]
fn wrong_marker_is_refused() {
    let (dir, store) = open_store();
    save_slot(&store, "career", 2);
    let path = export_one(&store, "career", dir.path());
    let mut bytes = fs::read(&path).expect("read");
    bytes[..8].copy_from_slice(b"SQLite f");
    write(&path, &bytes);
    let err = OpenedBundle::read(&path).expect_err("refused");
    assert!(
        matches!(err, StoreError::Bundle(ref m) if m.contains("marker")),
        "got {err:?}"
    );

    // A plain .sav file is not a bundle either.
    let sav = store.dir().join("career.sav");
    let err = OpenedBundle::read(&sav).expect_err("refused");
    assert!(matches!(err, StoreError::Bundle(_)), "got {err:?}");
}

#[test]
fn newer_bundle_version_is_refused() {
    let (dir, store) = open_store();
    let sav = save_slot(&store, "career", 2);
    let mut bytes = make_bundle("career", &manifest_for(&sav), &sav);
    bytes[8..12].copy_from_slice(&2u32.to_le_bytes());
    let body_len = bytes.len() - 32;
    let trailer = Sha256::digest(&bytes[..body_len]);
    bytes[body_len..].copy_from_slice(&trailer);
    let path = write(&dir.path().join("v2.bsave"), &bytes);
    let err = OpenedBundle::read(&path).expect_err("refused");
    assert!(matches!(err, StoreError::Bundle(_)), "got {err:?}");
}

#[test]
fn newer_manifest_format_is_refused() {
    let (dir, store) = open_store();
    let sav = save_slot(&store, "career", 2);
    let mut m = manifest_for(&sav);
    m.format_version = FORMAT_VERSION + 1;
    let path = write(
        &dir.path().join("future.bsave"),
        &make_bundle("career", &m, &sav),
    );
    let err = OpenedBundle::read(&path).expect_err("refused");
    assert!(
        matches!(err, StoreError::NewerVersion { found, .. } if found == FORMAT_VERSION + 1),
        "got {err:?}"
    );
}

#[test]
fn sav_that_does_not_match_its_manifest_is_refused() {
    let (dir, store) = open_store();
    let sav = save_slot(&store, "career", 2);
    let mut m = manifest_for(&sav);
    m.checksum_hex = "00".repeat(32);
    let path = write(
        &dir.path().join("wrong.bsave"),
        &make_bundle("career", &m, &sav),
    );
    let err = OpenedBundle::read(&path).expect_err("refused");
    assert!(matches!(err, StoreError::Corrupt(_)), "got {err:?}");
}

#[test]
fn checksum_valid_but_unreadable_sav_is_refused_and_nothing_is_written() {
    let (dir, _store) = open_store();
    let junk = b"this is not sqlite at all, just text".to_vec();
    let path = write(
        &dir.path().join("junk.bsave"),
        &make_bundle("career", &manifest_for(&junk), &junk),
    );
    let (_other_dir, other) = open_store();
    let err = other.import_file(&path, None).expect_err("refused");
    assert!(matches!(err, StoreError::Corrupt(_)), "got {err:?}");
    assert!(other.list().expect("list").is_empty());
}

#[test]
fn path_traversal_slot_hint_is_sanitised_and_stays_in_the_folder() {
    let root = tempfile::tempdir().expect("tempdir");
    let store_dir = root.path().join("saves");
    let store = LocalStore::open(&store_dir).expect("open");
    let sav = {
        let (_d, src) = open_store();
        save_slot(&src, "career", 3)
    };
    let path = write(
        &root.path().join("evil.bsave"),
        &make_bundle("../../escape", &manifest_for(&sav), &sav),
    );

    let name = store.import_file(&path, None).expect("import");
    assert!(!name.as_str().contains(['.', '/', '\\']), "name {name}");
    assert!(store_dir.join(format!("{name}.sav")).exists());
    // Nothing was written outside the save folder.
    assert_eq!(
        file_names(root.path()),
        vec!["evil.bsave".to_string(), "saves".to_string()]
    );
    assert_eq!(sanitise_slot_name("../../escape"), "------escape");
}

#[test]
fn path_traversal_as_name_is_refused() {
    let root = tempfile::tempdir().expect("tempdir");
    let store = LocalStore::open(root.path().join("saves")).expect("open");
    let sav = {
        let (_d, src) = open_store();
        save_slot(&src, "career", 3)
    };
    let path = write(
        &root.path().join("ok.bsave"),
        &make_bundle("career", &manifest_for(&sav), &sav),
    );
    for bad in ["../escape", "a/b", "", "x.sav"] {
        let err = store.import_file(&path, Some(bad)).expect_err("refused");
        assert!(
            matches!(err, StoreError::BadSlotName { .. }),
            "{bad:?}: {err:?}"
        );
    }
    assert!(store.list().expect("list").is_empty());
    assert!(!root.path().join("escape.sav").exists());
}

#[test]
fn import_never_overwrites_an_existing_slot() {
    let (dir, store) = open_store();
    save_slot(&store, "work", 1);
    let before_sav = fs::read(store.dir().join("work.sav")).expect("read");
    let before_manifest = fs::read(store.dir().join("work.manifest.json")).expect("read");

    let (_src_dir, src) = open_store();
    save_slot(&src, "work", 9);
    let path = export_one(&src, "work", dir.path());

    let same = store.import_file(&path, None).expect("import");
    assert_eq!(same.as_str(), "work-2");
    let other = store.import_file(&path, Some("WORK")).expect("import");
    assert_eq!(other.as_str(), "WORK-3");

    assert_eq!(
        fs::read(store.dir().join("work.sav")).expect("read"),
        before_sav
    );
    assert_eq!(
        fs::read(store.dir().join("work.manifest.json")).expect("read"),
        before_manifest
    );
    assert_eq!(store.load(&slot("work")).expect("load").data, data(1));
    assert_eq!(store.load(&same).expect("load").data, data(9));
}

#[test]
fn import_name_is_unique_regardless_of_letter_case() {
    let (dir, store) = open_store();
    save_slot(&store, "Career", 1);
    let path = export_one(&store, "Career", dir.path());
    let (_other_dir, other) = open_store();
    save_slot(&other, "career", 2);
    let name = other.import_file(&path, None).expect("import");
    assert_eq!(name.as_str(), "Career-2");
    assert_eq!(other.load(&slot("career")).expect("load").data, data(2));
}

#[test]
fn import_as_name_is_used_when_free() {
    let (dir, store) = open_store();
    save_slot(&store, "career", 5);
    let path = export_one(&store, "career", dir.path());
    let (_other_dir, other) = open_store();
    let name = other
        .import_file(&path, Some("from laptop"))
        .expect("import");
    assert_eq!(name.as_str(), "from laptop");
    assert_eq!(other.load(&name).expect("load").data, data(5));
}

#[test]
fn sanitised_names_are_limited_in_length() {
    let long = "x".repeat(200);
    assert!(sanitise_slot_name(&long).len() <= 64);
    let sav = {
        let (_d, src) = open_store();
        save_slot(&src, "career", 3)
    };
    let root = tempfile::tempdir().expect("tempdir");
    let path = write(
        &root.path().join("long.bsave"),
        &make_bundle(&long, &manifest_for(&sav), &sav),
    );
    let (_other_dir, other) = open_store();
    let first = other.import_file(&path, None).expect("import");
    let second = other.import_file(&path, None).expect("import");
    assert!(first.as_str().len() <= 64);
    assert!(second.as_str().len() <= 64);
    assert_ne!(first, second);
}

#[test]
fn bundle_read_does_not_touch_the_save_folder() {
    let (dir, store) = open_store();
    save_slot(&store, "career", 6);
    let path = export_one(&store, "career", dir.path());
    let before = file_names(store.dir());
    let opened = OpenedBundle::read(&path).expect("read");
    assert_eq!(opened.suggested_name, "career");
    assert_eq!(opened.data, data(6));
    assert_eq!(file_names(store.dir()), before);
}
