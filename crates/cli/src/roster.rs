//! Offline roster editing: `sim roster export` and `sim roster import`.
//!
//! Export writes the club and player names of a saved league to JSON. Import
//! reads the same shape back, applies the name changes, checks the result
//! with `World::restore`, and writes it as a NEW save slot. The source slot
//! is never written.
//!
//! Names are cosmetic. The edit is made outside the event log: the new save
//! keeps the source's log, results and meta unchanged, so the renames are
//! not recorded as events. Only `clubs[].name` and `players[].name` may
//! change. Other fields in the file are checked against the save and an
//! edit to them is an error.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use calibration::Calibration;
use serde::{Deserialize, Serialize};
use storage::{Loaded, LocalStore, SaveKind, SaveStore, SlotId};
use world::player::{Player, PlayerId};
use world::rating::{Kind, Model};
use world::state::{Snapshot, World};

/// The shipped calibration, the same file `host` embeds. A save written by
/// the game restores against it.
const PARAMS: &str = include_str!("../../../data/params.json");

/// Longest allowed player or club name, in characters.
const NAME_MAX_CHARS: usize = 40;

/// Help text for `sim roster`. It states that names are cosmetic and that the
/// edit happens outside the event log.
const USAGE: &str = "\
usage:
  sim roster export <save|path/to/slot.sav> <out.json> [--saves <dir>]
  sim roster import <save|path/to/slot.sav> <edits.json> <new-slot> [--saves <dir>]

Export writes the club and player names of a saved league to JSON. Import
applies edited names and writes the result as a NEW save slot; the source
slot is never changed.

Only names may change. Names are cosmetic, so this edit is made outside the
event log: the new save keeps the source log unchanged and the renames are
not recorded as events. Names: 1 to 40 characters, no control characters,
and unique (ignoring case) among the players whose names you change.
Unknown ids, unknown club indexes, and changes to club, age or position
are errors. Nothing is written when any check fails.

Saves folder: --saves <dir>, else $BASEBALL_SIM_SAVES, else
~/.baseball-sim/saves. A path ending in .sav is read from its own folder.
The new slot is always written to the saves folder.";

/// The club and player names of one league, as exported.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Roster {
    clubs: Vec<ClubRow>,
    players: Vec<PlayerRow>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ClubRow {
    index: usize,
    name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PlayerRow {
    id: u32,
    name: String,
    /// Club index, or `null` for a free agent or a retired player.
    club: Option<usize>,
    age: i32,
    position: Position,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
enum Position {
    Batter,
    Pitcher,
}

/// What an import did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Report {
    renamed: usize,
    used_backup: bool,
}

fn calibrated() -> Result<Calibration, String> {
    Calibration::from_json(PARAMS)
        .map_err(|e| format!("the built-in calibration does not load: {e}"))
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

fn open_store(dir: &Path) -> Result<LocalStore, String> {
    LocalStore::open(dir)
        .map_err(|e| format!("cannot open the saves folder {}: {e}", dir.display()))
}

fn slot_id(name: &str) -> Result<SlotId, String> {
    SlotId::new(name).map_err(|e| e.to_string())
}

/// Loads a slot and checks that its snapshot restores. Loaded data is never
/// trusted, so this runs before any name is read.
fn load_league(store: &LocalStore, slot: &SlotId) -> Result<(Loaded, Snapshot), String> {
    let loaded = store.load(slot).map_err(|e| e.to_string())?;
    let snap: Snapshot = serde_json::from_slice(&loaded.data.snapshot)
        .map_err(|e| format!("the save's snapshot is not readable: {e}"))?;
    let cal = calibrated()?;
    World::restore(cal.params, Model::from_calibration(&cal), snap.clone())
        .map_err(|e| format!("the save is not a consistent league: {e}"))?;
    Ok((loaded, snap))
}

/// Every player of the league: the active ones (signed and free agents) and
/// the retired ones.
fn all_players(snap: &Snapshot) -> impl Iterator<Item = &Player> {
    snap.players
        .values()
        .chain(snap.retired.values().map(|(p, _)| p))
}

fn find(snap: &Snapshot, id: PlayerId) -> Option<&Player> {
    snap.players
        .get(&id)
        .or_else(|| snap.retired.get(&id).map(|(p, _)| p))
}

fn position_of(kind: Kind) -> Position {
    match kind {
        Kind::Batter => Position::Batter,
        Kind::Pitcher => Position::Pitcher,
    }
}

fn roster_of(snap: &Snapshot) -> Roster {
    let clubs = snap
        .clubs
        .iter()
        .enumerate()
        .map(|(index, club)| ClubRow {
            index,
            name: club.name.clone(),
        })
        .collect();
    let mut players: Vec<PlayerRow> = all_players(snap)
        .map(|p| PlayerRow {
            id: p.id.0,
            name: p.name.clone(),
            club: p.team.map(usize::from),
            age: p.age(snap.year),
            position: position_of(p.kind),
        })
        .collect();
    players.sort_by_key(|row| row.id);
    Roster { clubs, players }
}

/// A name is 1 to 40 characters, not blank, with no control characters.
fn check_name(name: &str) -> Result<(), String> {
    if name.trim().is_empty() {
        return Err("the name is empty".into());
    }
    if name.chars().count() > NAME_MAX_CHARS {
        return Err(format!(
            "the name is longer than {NAME_MAX_CHARS} characters"
        ));
    }
    if name.chars().any(char::is_control) {
        return Err("the name has a control character".into());
    }
    Ok(())
}

/// Checks every row of `edits` against `snap`. Only when all checks pass are
/// the new names written into `snap`. Returns how many names changed.
fn apply_names(snap: &mut Snapshot, edits: &Roster) -> Result<usize, String> {
    let mut club_renames: Vec<(usize, &str)> = Vec::new();
    let mut listed_clubs = BTreeSet::new();
    for row in &edits.clubs {
        let current = snap
            .clubs
            .get(row.index)
            .ok_or_else(|| format!("club {} does not exist", row.index))?;
        if !listed_clubs.insert(row.index) {
            return Err(format!("club {} is listed twice", row.index));
        }
        if current.name != row.name {
            check_name(&row.name).map_err(|why| format!("club {}: {why}", row.index))?;
            club_renames.push((row.index, row.name.as_str()));
        }
    }

    let mut renames: BTreeMap<PlayerId, &str> = BTreeMap::new();
    let mut listed_players = BTreeSet::new();
    for row in &edits.players {
        let id = PlayerId(row.id);
        let p = find(snap, id).ok_or_else(|| format!("player {} does not exist", row.id))?;
        if !listed_players.insert(id) {
            return Err(format!("player {} is listed twice", row.id));
        }
        if row.club != p.team.map(usize::from) {
            return Err(format!(
                "player {}: the club cannot be changed here",
                row.id
            ));
        }
        if row.age != p.age(snap.year) {
            return Err(format!("player {}: the age cannot be changed here", row.id));
        }
        if row.position != position_of(p.kind) {
            return Err(format!(
                "player {}: the position cannot be changed here",
                row.id
            ));
        }
        if row.name != p.name {
            check_name(&row.name).map_err(|why| format!("player {}: {why}", row.id))?;
            renames.insert(id, row.name.as_str());
        }
    }

    // A renamed player may not share a name (ignoring case) with any other
    // player after the edit. Pairs of unchanged players are not checked.
    let mut users: BTreeMap<String, usize> = BTreeMap::new();
    for p in all_players(snap) {
        let name = renames.get(&p.id).copied().unwrap_or(p.name.as_str());
        *users.entry(name.to_lowercase()).or_insert(0) += 1;
    }
    for (id, name) in &renames {
        if users.get(&name.to_lowercase()).is_some_and(|n| *n > 1) {
            return Err(format!(
                "player {}: the name \"{name}\" is already used by another player",
                id.0
            ));
        }
    }

    let changed = club_renames.len() + renames.len();
    for (index, name) in club_renames {
        if let Some(club) = snap.clubs.get_mut(index) {
            club.name = name.to_string();
        }
    }
    for (id, name) in renames {
        let target = match snap.players.get_mut(&id) {
            Some(p) => Some(p),
            None => snap.retired.get_mut(&id).map(|(p, _)| p),
        };
        if let Some(p) = target {
            p.name = name.to_string();
        }
    }
    Ok(changed)
}

fn slot_exists(store: &LocalStore, slot: &SlotId) -> bool {
    let dir = store.dir();
    [".sav", ".manifest.json"]
        .iter()
        .any(|ext| dir.join(format!("{slot}{ext}")).exists())
}

/// Reads a slot and returns its roster. The flag is true when the backup
/// file was used because the primary save was damaged.
fn export_roster(store: &LocalStore, source: &SlotId) -> Result<(Roster, bool), String> {
    let (loaded, snap) = load_league(store, source)?;
    Ok((roster_of(&snap), loaded.used_backup))
}

/// Applies `edits_json` to the source slot and writes the result to
/// `target`, a slot that must not exist yet. Nothing is written when any
/// check fails.
fn import_roster(
    source_store: &LocalStore,
    source: &SlotId,
    edits_json: &str,
    target_store: &LocalStore,
    target: &SlotId,
) -> Result<Report, String> {
    if slot_exists(target_store, target) {
        return Err(format!(
            "the slot {target} already exists; choose a new name"
        ));
    }
    let edits: Roster = serde_json::from_str(edits_json)
        .map_err(|e| format!("the edits file is not valid: {e}"))?;
    let (loaded, mut snap) = load_league(source_store, source)?;
    let renamed = apply_names(&mut snap, &edits)?;

    // The manifest shows the user's club, so it follows a renamed club.
    let user_club = loaded
        .data
        .meta
        .get("club")
        .and_then(|c| c.parse::<usize>().ok())
        .and_then(|i| snap.clubs.get(i))
        .map(|club| club.name.clone());

    let cal = calibrated()?;
    let snapshot = serde_json::to_vec(&snap).map_err(|e| e.to_string())?;
    World::restore(cal.params, Model::from_calibration(&cal), snap)
        .map_err(|e| format!("the edited league is not consistent: {e}"))?;

    let Loaded {
        mut manifest,
        mut data,
        used_backup,
    } = loaded;
    manifest.saved_at_unix = now();
    manifest.kind = SaveKind::Manual;
    if let Some(name) = user_club {
        manifest.team_name = name;
    }
    data.snapshot = snapshot;
    data.meta
        .insert("roster_edited_from".into(), source.to_string());
    target_store
        .save(target, manifest, &data)
        .map_err(|e| e.to_string())?;
    Ok(Report {
        renamed,
        used_backup,
    })
}

/// The save folder: `--saves`, else `BASEBALL_SIM_SAVES`, else
/// `~/.baseball-sim/saves`.
fn saves_dir(flag: Option<PathBuf>) -> Result<PathBuf, String> {
    if let Some(dir) = flag {
        return Ok(dir);
    }
    if let Some(dir) = std::env::var_os("BASEBALL_SIM_SAVES") {
        return Ok(PathBuf::from(dir));
    }
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .ok_or("no home folder; pass --saves <dir>")?;
    Ok(PathBuf::from(home).join(".baseball-sim").join("saves"))
}

/// Resolves a source given as a slot name (in `saves`) or as a path to a
/// `.sav` file (read from its own folder).
fn open_source(arg: &str, saves: &Path) -> Result<(LocalStore, SlotId), String> {
    let path = Path::new(arg);
    if path.extension().and_then(|e| e.to_str()) == Some("sav") {
        let dir = match path.parent() {
            Some(d) if !d.as_os_str().is_empty() => d,
            _ => Path::new("."),
        };
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or_else(|| format!("bad save path {arg}"))?;
        Ok((open_store(dir)?, slot_id(stem)?))
    } else {
        Ok((open_store(saves)?, slot_id(arg)?))
    }
}

/// Runs `sim roster ...`. Returns the message for stdout. Errors go to
/// stderr with exit code 2.
pub fn run(args: &[String]) -> Result<String, String> {
    let mut saves_flag: Option<PathBuf> = None;
    let mut rest: Vec<&str> = Vec::new();
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        if arg.as_str() == "--saves" {
            let dir = it.next().ok_or("--saves needs a folder")?;
            saves_flag = Some(PathBuf::from(dir));
        } else {
            rest.push(arg.as_str());
        }
    }
    match rest.as_slice() {
        ["export", source, out] => {
            let saves = saves_dir(saves_flag)?;
            let (store, slot) = open_source(source, &saves)?;
            let (roster, used_backup) = export_roster(&store, &slot)?;
            let text = serde_json::to_string_pretty(&roster).map_err(|e| e.to_string())?;
            fs::write(out, format!("{text}\n")).map_err(|e| format!("cannot write {out}: {e}"))?;
            let backup = if used_backup {
                " (the backup file was used: the primary save is damaged)"
            } else {
                ""
            };
            Ok(format!(
                "exported {} clubs and {} players from {slot} to {out}{backup}",
                roster.clubs.len(),
                roster.players.len()
            ))
        }
        ["import", source, edits, target] => {
            let saves = saves_dir(saves_flag)?;
            let (source_store, source_slot) = open_source(source, &saves)?;
            let target_slot = slot_id(target)?;
            let target_store = open_store(&saves)?;
            let text =
                fs::read_to_string(edits).map_err(|e| format!("cannot read {edits}: {e}"))?;
            let report = import_roster(
                &source_store,
                &source_slot,
                &text,
                &target_store,
                &target_slot,
            )?;
            Ok(format!(
                "imported {} name changes from {edits} into the new slot {target_slot}",
                report.renamed
            ))
        }
        _ => Err(USAGE.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use host::Host;
    use host::api::{Difficulty, NewGame, Preset, Request, Response};

    fn id(name: &str) -> SlotId {
        SlotId::new(name).unwrap()
    }

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    /// A real league saved by `host`, so the snapshot is the game's own.
    fn make_save(dir: &Path) -> LocalStore {
        let cal = calibrated().unwrap();
        let game = NewGame {
            seed: 7,
            club: 0,
            difficulty: Difficulty::Preset(Preset::Veteran),
            burn_in_years: 0,
        };
        let mut host = Host::new_game(&cal, &game)
            .unwrap()
            .with_saves(dir)
            .unwrap();
        let response = host.handle(Request::Save { slot: "src".into() });
        assert!(matches!(response, Response::Done(_)), "the save failed");
        open_store(dir).unwrap()
    }

    fn snapshot_of(store: &LocalStore, slot: &str) -> Snapshot {
        let loaded = store.load(&id(slot)).unwrap();
        serde_json::from_slice(&loaded.data.snapshot).unwrap()
    }

    fn set_name(snap: &mut Snapshot, pid: u32, name: &str) {
        let pid = PlayerId(pid);
        if let Some(p) = snap.players.get_mut(&pid) {
            p.name = name.into();
        } else if let Some((p, _)) = snap.retired.get_mut(&pid) {
            p.name = name.into();
        }
    }

    fn export(store: &LocalStore, slot: &str) -> Roster {
        export_roster(store, &id(slot)).unwrap().0
    }

    fn attempt(store: &LocalStore, edits: &Roster, target: &str) -> Result<Report, String> {
        let json = serde_json::to_string(edits).unwrap();
        import_roster(store, &id("src"), &json, store, &id(target))
    }

    /// Runs an import that must fail and checks that no slot was written.
    fn rejected(store: &LocalStore, edits: &Roster) -> String {
        let err = attempt(store, edits, "bad").unwrap_err();
        assert!(
            store.read_manifest(&id("bad")).is_err(),
            "a rejected import must not write a slot"
        );
        err
    }

    #[test]
    fn export_lists_every_club_and_player() {
        let dir = tempfile::tempdir().unwrap();
        let store = make_save(dir.path());
        let snap = snapshot_of(&store, "src");
        let roster = export(&store, "src");
        assert_eq!(roster.clubs.len(), snap.clubs.len());
        assert_eq!(
            roster.players.len(),
            snap.players.len() + snap.retired.len()
        );
        assert!(roster.players.windows(2).all(|w| w[0].id < w[1].id));
        let (pid, p) = snap.players.iter().next().unwrap();
        let row = roster.players.iter().find(|r| r.id == pid.0).unwrap();
        assert_eq!(row.name, p.name);
        assert_eq!(row.club, p.team.map(usize::from));
        assert_eq!(row.age, p.age(snap.year));
        assert_eq!(row.position, position_of(p.kind));
        let json = serde_json::to_value(&roster).unwrap();
        assert_eq!(json["clubs"][0]["index"], 0);
        assert!(json["players"][0]["position"].is_string());
    }

    #[test]
    fn rename_round_trip_changes_only_the_names() {
        let dir = tempfile::tempdir().unwrap();
        let store = make_save(dir.path());
        let before = snapshot_of(&store, "src");
        let mut edits = export(&store, "src");
        edits.clubs[0].name = "Lake Bend Herons".into();
        edits.players[0].name = "Renamed Player".into();
        let report = attempt(&store, &edits, "renamed").unwrap();
        assert_eq!(
            report,
            Report {
                renamed: 2,
                used_backup: false
            }
        );

        let after = snapshot_of(&store, "renamed");
        let mut expected = before.clone();
        expected.clubs[0].name = "Lake Bend Herons".into();
        set_name(&mut expected, edits.players[0].id, "Renamed Player");
        assert_eq!(after, expected);
        assert_eq!(snapshot_of(&store, "src"), before, "the source changed");
        assert_eq!(export(&store, "renamed").players, edits.players);

        // The user's club is club 0, so the manifest follows its new name.
        let manifest = store.read_manifest(&id("renamed")).unwrap();
        assert_eq!(manifest.team_name, "Lake Bend Herons");
        assert_eq!(manifest.kind, SaveKind::Manual);
        assert_eq!(
            store.read_manifest(&id("src")).unwrap().team_name,
            before.clubs[0].name
        );
        // load checks the checksum and the SQLite file.
        assert!(!store.load(&id("renamed")).unwrap().used_backup);
    }

    #[test]
    fn an_unchanged_export_copies_the_league() {
        let dir = tempfile::tempdir().unwrap();
        let store = make_save(dir.path());
        let before = snapshot_of(&store, "src");
        let edits = export(&store, "src");
        let report = attempt(&store, &edits, "copy").unwrap();
        assert_eq!(report.renamed, 0);
        assert_eq!(snapshot_of(&store, "copy"), before);
    }

    #[test]
    fn a_forty_character_name_is_allowed() {
        let dir = tempfile::tempdir().unwrap();
        let store = make_save(dir.path());
        let mut edits = export(&store, "src");
        edits.players[0].name = "y".repeat(40);
        attempt(&store, &edits, "long").unwrap();
        assert_eq!(
            snapshot_of(&store, "long")
                .players
                .get(&PlayerId(edits.players[0].id))
                .map(|p| p.name.clone())
                .or_else(|| snapshot_of(&store, "long")
                    .retired
                    .get(&PlayerId(edits.players[0].id))
                    .map(|(p, _)| p.name.clone())),
            Some("y".repeat(40))
        );
    }

    #[test]
    fn bad_edits_are_rejected_and_nothing_is_written() {
        let dir = tempfile::tempdir().unwrap();
        let store = make_save(dir.path());
        let base = export(&store, "src");
        let with = |edit: &dyn Fn(&mut Roster)| {
            let mut r = base.clone();
            edit(&mut r);
            r
        };
        let flip = |p: Position| match p {
            Position::Batter => Position::Pitcher,
            Position::Pitcher => Position::Batter,
        };
        let cases: Vec<(&str, Roster)> = vec![
            (
                "empty",
                with(&|r: &mut Roster| r.clubs[0].name = String::new()),
            ),
            (
                "empty",
                with(&|r: &mut Roster| r.players[0].name = "   ".into()),
            ),
            (
                "longer than",
                with(&|r: &mut Roster| r.players[0].name = "x".repeat(41)),
            ),
            (
                "control",
                with(&|r: &mut Roster| r.players[0].name = "Bad\u{7}Name".into()),
            ),
            (
                "already used",
                with(&|r: &mut Roster| r.players[0].name = r.players[1].name.clone()),
            ),
            (
                "already used",
                with(&|r: &mut Roster| r.players[0].name = r.players[1].name.to_uppercase()),
            ),
            (
                "does not exist",
                with(&|r: &mut Roster| r.players[0].id = 999_999),
            ),
            (
                "does not exist",
                with(&|r: &mut Roster| {
                    r.clubs.push(ClubRow {
                        index: 99,
                        name: "Nowhere".into(),
                    })
                }),
            ),
            (
                "listed twice",
                with(&|r: &mut Roster| r.clubs.push(r.clubs[0].clone())),
            ),
            ("age cannot", with(&|r: &mut Roster| r.players[0].age += 1)),
            (
                "club cannot",
                with(&|r: &mut Roster| r.players[0].club = Some(99)),
            ),
            (
                "position cannot",
                with(&|r: &mut Roster| r.players[0].position = flip(r.players[0].position)),
            ),
        ];
        for (expected, edits) in &cases {
            let err = rejected(&store, edits);
            assert!(err.contains(expected), "wanted \"{expected}\", got: {err}");
        }
        // A rejection of a later row must not leave earlier renames applied.
        let mut edits = base.clone();
        edits.clubs[0].name = "Lake Bend Herons".into();
        edits.players[0].name = "y".repeat(41);
        rejected(&store, &edits);
        assert_eq!(snapshot_of(&store, "src").clubs[0].name, base.clubs[0].name);
    }

    #[test]
    fn a_malformed_edits_file_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let store = make_save(dir.path());
        let text = r#"{"clubs":[],"players":[],"extra":1}"#;
        let err = import_roster(&store, &id("src"), text, &store, &id("bad")).unwrap_err();
        assert!(err.contains("not valid"), "{err}");
        assert!(store.read_manifest(&id("bad")).is_err());
    }

    #[test]
    fn an_existing_target_slot_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let store = make_save(dir.path());
        let before = snapshot_of(&store, "src");
        let mut edits = export(&store, "src");
        edits.clubs[0].name = "Lake Bend Herons".into();
        let err = attempt(&store, &edits, "src").unwrap_err();
        assert!(err.contains("already exists"), "{err}");
        assert_eq!(snapshot_of(&store, "src"), before);
    }

    #[test]
    fn command_line_export_and_import_work_on_a_save_file_path() {
        let dir = tempfile::tempdir().unwrap();
        let store = make_save(dir.path());
        let saves = dir.path().to_string_lossy().to_string();
        let sav = dir.path().join("src.sav").to_string_lossy().to_string();
        let out_path = dir.path().join("roster.json");
        let out = out_path.to_string_lossy().to_string();

        let msg = run(&args(&["export", &sav, &out, "--saves", &saves])).unwrap();
        assert!(msg.starts_with("exported"), "{msg}");
        let mut roster: Roster =
            serde_json::from_str(&fs::read_to_string(&out_path).unwrap()).unwrap();
        roster.clubs[1].name = "Harbor Point Otters".into();
        fs::write(&out_path, serde_json::to_string(&roster).unwrap()).unwrap();

        run(&args(&[
            "import",
            &sav,
            &out,
            "from-file",
            "--saves",
            &saves,
        ]))
        .unwrap();
        assert_eq!(
            snapshot_of(&store, "from-file").clubs[1].name,
            "Harbor Point Otters"
        );
        assert_eq!(
            snapshot_of(&store, "src").clubs[1].name,
            roster_of(&snapshot_of(&store, "src")).clubs[1].name
        );
    }

    #[test]
    fn bad_command_lines_print_the_usage() {
        let err = run(&args(&["export"])).unwrap_err();
        assert!(err.contains("usage"), "{err}");
        let err = run(&args(&["import", "a", "b", "c", "--saves"])).unwrap_err();
        assert!(err.contains("--saves"), "{err}");
    }
}
