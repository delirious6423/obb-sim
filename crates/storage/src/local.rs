//! [`LocalStore`]: save slots as files in one directory.
//!
//! # Save order
//!
//! 1. Refuse to touch a slot whose existing save is from a newer format.
//! 2. Delete leftover `.tmp` files of the slot.
//! 3. Write `<slot>.sav.tmp` with SQLite, fsync it, and hash its bytes.
//! 4. Write `<slot>.manifest.json.tmp` with the hash, fsync it.
//! 5. If the current `.sav` matches its manifest, rename `.sav` to `.sav.bak`
//!    and the manifest to `.manifest.json.bak`. A damaged current save never
//!    replaces a good backup.
//! 6. Rename the two `.tmp` files into place, then fsync the directory.
//!
//! At every point a good `.sav` or a good `.sav.bak` exists, each with a
//! manifest whose checksum matches. A crash after step 5 leaves the backup
//! pair, which load can use.
//!
//! # Load order
//!
//! Candidates are tried in order: `(.sav, manifest)`, then
//! `(.sav.bak, manifest.bak)`, then `(.sav.bak, manifest)`. The third covers a
//! crash between the two backup renames. A candidate passes only when its
//! checksum matches and SQLite can read it. The first passing candidate wins.

use std::collections::BTreeSet;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OpenFlags};

use crate::FORMAT_VERSION;
use crate::bundle::{self, BUNDLE_EXTENSION, MAX_BUNDLE_BYTES, sha256_hex};
use crate::db;
use crate::error::{Result, StoreError};
use crate::model::{Manifest, SaveData, SaveKind};
use crate::slot::{MAX_SLOT_LEN, SlotId};
use crate::store::{Loaded, SaveStore, SlotSummary};

/// Default number of rotating autosave slots (`auto-1` to `auto-3`).
pub const DEFAULT_AUTO_SLOTS: u32 = 3;

/// Slot name used by [`LocalStore::quicksave`].
pub const QUICK_SLOT: &str = "quick";

const AUTO_PREFIX: &str = "auto-";
const SAV: &str = ".sav";
const SAV_TMP: &str = ".sav.tmp";
const SAV_BAK: &str = ".sav.bak";
const SAV_LOAD_TMP: &str = ".sav.load.tmp";
const MANIFEST: &str = ".manifest.json";
const MANIFEST_TMP: &str = ".manifest.json.tmp";
const MANIFEST_BAK: &str = ".manifest.json.bak";
const IMPORT_LOAD_TMP: &str = ".import.load.tmp";
/// Highest `-N` suffix tried when a name is taken. Past this, export or
/// import fails.
const MAX_NAME_TRIES: u32 = 10_000;

/// A save bundle that passed every check, ready to be imported.
///
/// Nothing from the file is written to the save folder until
/// [`LocalStore::import_opened`] is called.
#[derive(Debug, Clone)]
pub struct OpenedBundle {
    /// Slot name the bundle was exported from. It is sanitised before use,
    /// and used only when the caller gives no name.
    pub suggested_name: String,
    /// The manifest as exported.
    pub manifest: Manifest,
    /// The save contents, read and checked in a scratch folder.
    pub data: SaveData,
}

impl OpenedBundle {
    /// Reads and checks a `.bsave` file. Refuses files over
    /// [`MAX_BUNDLE_BYTES`]. Validation runs in a scratch folder under the
    /// system temp directory, which is removed afterwards. The save folder is
    /// not touched.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError::Bundle`] for a bad marker, bad framing or a bad
    /// checksum, [`StoreError::Corrupt`] when the `.sav` does not match the
    /// manifest or cannot be read, and [`StoreError::NewerVersion`] for a
    /// newer format.
    pub fn read(path: &Path) -> Result<Self> {
        let len = fs::metadata(path)?.len();
        if len > MAX_BUNDLE_BYTES {
            return Err(StoreError::Bundle(format!(
                "file is {len} bytes; the limit is {MAX_BUNDLE_BYTES}"
            )));
        }
        let bytes = fs::read(path)?;
        let decoded = bundle::decode(&bytes)?;
        let scratch = scratch_dir()?;
        let result = (|| {
            let sav = scratch.join("import.sav");
            fs::write(&sav, &decoded.sav)?;
            let data = db::read_file(&sav, &scratch.join(IMPORT_LOAD_TMP))
                .map_err(|e| StoreError::Corrupt(format!("bundle .sav: {e}")))?;
            Ok(Self {
                suggested_name: bundle::sanitise_slot_name(&decoded.slot_hint),
                manifest: decoded.manifest,
                data,
            })
        })();
        let _ = fs::remove_dir_all(&scratch);
        result
    }
}

fn scratch_dir() -> Result<PathBuf> {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let dir = std::env::temp_dir().join(format!("bsave-import-{}-{nanos}", std::process::id()));
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Save slots stored as files in one directory. See the module docs for the
/// write and read order and the crash guarantees.
#[derive(Debug, Clone)]
pub struct LocalStore {
    dir: PathBuf,
    auto_slots: u32,
}

impl LocalStore {
    /// Opens the store, creating `dir` when it does not exist.
    ///
    /// # Errors
    ///
    /// Returns an I/O error when the directory cannot be created.
    pub fn open(dir: impl AsRef<Path>) -> Result<Self> {
        let dir = dir.as_ref();
        fs::create_dir_all(dir)?;
        Ok(Self {
            dir: dir.to_path_buf(),
            auto_slots: DEFAULT_AUTO_SLOTS,
        })
    }

    /// The store directory.
    #[must_use]
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Sets how many autosave slots rotate (default [`DEFAULT_AUTO_SLOTS`]).
    ///
    /// # Errors
    ///
    /// Returns [`StoreError::InvalidInput`] when `count` is 0.
    pub fn with_auto_slots(mut self, count: u32) -> Result<Self> {
        if count == 0 {
            return Err(StoreError::InvalidInput(
                "autosave needs at least one slot".into(),
            ));
        }
        self.auto_slots = count;
        Ok(self)
    }

    /// The autosave slot names, `auto-1` to `auto-N`, in order.
    #[must_use]
    pub fn auto_slot_ids(&self) -> Vec<SlotId> {
        (1..=self.auto_slots)
            .filter_map(|index| SlotId::new(format!("{AUTO_PREFIX}{index}")).ok())
            .collect()
    }

    /// Writes an autosave into the next rotating slot and returns its name.
    ///
    /// The slot is chosen like this: the first `auto-N` (in order) that has no
    /// manifest; otherwise the one with the oldest `saved_at_unix`, then the
    /// lowest index. Unreadable manifests count as oldest. `manifest.kind` is
    /// set to [`SaveKind::Auto`].
    ///
    /// # Errors
    ///
    /// Same as [`SaveStore::save`].
    pub fn autosave(&self, mut manifest: Manifest, data: &SaveData) -> Result<SlotId> {
        manifest.kind = SaveKind::Auto;
        let slot = self.next_auto_slot()?;
        self.save(&slot, manifest, data)?;
        Ok(slot)
    }

    /// Writes the quicksave slot ([`QUICK_SLOT`]) and returns its name.
    /// `manifest.kind` is set to [`SaveKind::Quick`].
    ///
    /// # Errors
    ///
    /// Same as [`SaveStore::save`].
    pub fn quicksave(&self, mut manifest: Manifest, data: &SaveData) -> Result<SlotId> {
        manifest.kind = SaveKind::Quick;
        let slot = SlotId::new(QUICK_SLOT)?;
        self.save(&slot, manifest, data)?;
        Ok(slot)
    }

    fn next_auto_slot(&self) -> Result<SlotId> {
        // Pick by a key that orders free slots first, then the oldest save.
        let mut best: Option<(Option<u64>, usize, SlotId)> = None;
        for (index, slot) in self.auto_slot_ids().into_iter().enumerate() {
            if !self.file(&slot, MANIFEST).exists() {
                return Ok(slot);
            }
            let saved_at = self.read_manifest(&slot).ok().map(|m| m.saved_at_unix);
            let better = match &best {
                None => true,
                Some((best_at, best_index, _)) => (saved_at, index) < (*best_at, *best_index),
            };
            if better {
                best = Some((saved_at, index, slot));
            }
        }
        best.map(|(_, _, slot)| slot)
            .ok_or_else(|| StoreError::InvalidInput("no autosave slots configured".into()))
    }

    fn file(&self, slot: &SlotId, suffix: &str) -> PathBuf {
        self.dir.join(format!("{}{suffix}", slot.as_str()))
    }

    /// Best-effort fsync of the directory, so renames survive a crash.
    /// Directory sync is not available on every platform, so errors are ignored.
    fn sync_dir(&self) {
        if let Ok(dir) = File::open(&self.dir) {
            let _ = dir.sync_all();
        }
    }

    /// Returns `NewerVersion` when the slot already holds a newer save.
    /// Unreadable files do not block a save, so a damaged slot can be replaced.
    fn refuse_newer_existing(&self, slot: &SlotId) -> Result<()> {
        if let Ok(manifest) = read_manifest_at(&self.file(slot, MANIFEST), slot) {
            check_manifest_version(manifest.format_version)?;
        }
        let sav = self.file(slot, SAV);
        let db_version = Connection::open_with_flags(&sav, OpenFlags::SQLITE_OPEN_READ_ONLY)
            .ok()
            .and_then(|conn| db::user_version(&conn).ok());
        if let Some(found) = db_version {
            check_manifest_version(found)?;
        }
        Ok(())
    }

    /// True when the current `.sav` exists and matches its manifest checksum.
    fn current_is_good(&self, slot: &SlotId) -> bool {
        let Ok(manifest) = read_manifest_at(&self.file(slot, MANIFEST), slot) else {
            return false;
        };
        match fs::read(self.file(slot, SAV)) {
            Ok(bytes) => sha256_hex(&bytes) == manifest.checksum_hex,
            Err(_) => false,
        }
    }

    fn remove_tmp_files(&self, slot: &SlotId) -> Result<()> {
        for suffix in [SAV_TMP, MANIFEST_TMP, SAV_LOAD_TMP] {
            remove_if_exists(&self.file(slot, suffix))?;
        }
        Ok(())
    }

    fn summarize(&self, id: SlotId) -> SlotSummary {
        match read_manifest_at(&self.file(&id, MANIFEST), &id) {
            Err(err) => SlotSummary {
                manifest: None,
                problem: Some(format!("manifest unreadable: {err}")),
                id,
            },
            Ok(manifest) => {
                let problem = if manifest.format_version > FORMAT_VERSION {
                    Some(format!(
                        "written by newer format version {}",
                        manifest.format_version
                    ))
                } else if !self.file(&id, SAV).exists() {
                    Some("save file missing".to_string())
                } else {
                    None
                };
                SlotSummary {
                    id,
                    manifest: Some(manifest),
                    problem,
                }
            }
        }
    }

    /// Tries one `(sav, manifest)` pair. Returns the verified save, with the
    /// `.sav` bytes it was read from, or the reason it failed.
    fn try_candidate(
        &self,
        slot: &SlotId,
        sav_suffix: &str,
        manifest_suffix: &str,
    ) -> Result<(Manifest, SaveData, Vec<u8>)> {
        let manifest = read_manifest_at(&self.file(slot, manifest_suffix), slot)?;
        check_manifest_version(manifest.format_version)?;
        let sav = self.file(slot, sav_suffix);
        let bytes = fs::read(&sav)?;
        let actual = sha256_hex(&bytes);
        if actual != manifest.checksum_hex {
            return Err(StoreError::Corrupt(format!(
                "{sav_suffix}: checksum mismatch (manifest {}, file {actual})",
                manifest.checksum_hex
            )));
        }
        let data = db::read_file(&sav, &self.file(slot, SAV_LOAD_TMP))?;
        Ok((manifest, data, bytes))
    }

    /// Loads a slot like [`SaveStore::load`], and also returns the `.sav`
    /// bytes that were verified against the manifest.
    fn load_verified(&self, slot: &SlotId) -> Result<(Loaded, Vec<u8>)> {
        let sav_exists = self.file(slot, SAV).exists();
        let bak_exists = self.file(slot, SAV_BAK).exists();
        if !sav_exists && !bak_exists && !self.file(slot, MANIFEST).exists() {
            return Err(StoreError::NotFound(slot.to_string()));
        }

        let candidates = [
            (SAV, MANIFEST, false),
            (SAV_BAK, MANIFEST_BAK, true),
            (SAV_BAK, MANIFEST, true),
        ];
        let mut problems = Vec::new();
        for (sav_suffix, manifest_suffix, used_backup) in candidates {
            match self.try_candidate(slot, sav_suffix, manifest_suffix) {
                Ok((manifest, data, sav)) => {
                    let loaded = Loaded {
                        manifest,
                        data,
                        used_backup,
                    };
                    return Ok((loaded, sav));
                }
                // A newer format is not a damaged file. Falling back to an
                // older backup would silently lose data, so stop here.
                Err(err @ StoreError::NewerVersion { .. }) => return Err(err),
                Err(err) => problems.push(format!("{sav_suffix}/{manifest_suffix}: {err}")),
            }
        }
        Err(StoreError::Corrupt(problems.join("; ")))
    }

    /// Writes a copy of `slot` as `<slot>.bsave` (or `<slot>-2.bsave`, ...)
    /// inside `folder`, which is created when missing. An existing file is
    /// never overwritten. Returns the path written.
    ///
    /// The slot is loaded and checked first, so a damaged slot is not exported.
    ///
    /// # Errors
    ///
    /// Returns the load error for a missing or damaged slot, or an I/O error.
    pub fn export_to(&self, slot: &SlotId, folder: &Path) -> Result<PathBuf> {
        let (loaded, sav) = self.load_verified(slot)?;
        let bytes = bundle::encode(slot.as_str(), &loaded.manifest, &sav)?;
        fs::create_dir_all(folder)?;
        let path = unique_path(folder, slot.as_str(), BUNDLE_EXTENSION)?;
        let tmp = path.with_extension(format!("{BUNDLE_EXTENSION}.part"));
        write_synced(&tmp, &bytes)?;
        fs::rename(&tmp, &path)?;
        Ok(path)
    }

    /// Imports a `.bsave` file as a new slot. See [`LocalStore::import_opened`].
    ///
    /// # Errors
    ///
    /// Same as [`OpenedBundle::read`] and [`LocalStore::import_opened`].
    pub fn import_file(&self, path: &Path, as_name: Option<&str>) -> Result<SlotId> {
        let opened = OpenedBundle::read(path)?;
        self.import_opened(opened, as_name)
    }

    /// Stores an opened bundle as a new slot and returns its name.
    ///
    /// With `as_name`, that name is validated (refused when it breaks the
    /// slot rules). Without it, the exported name is sanitised. In both cases
    /// the slot is never an existing one: when the name is taken, in any letter
    /// case, `-2`, `-3`, ... is appended.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError::BadSlotName`] for a bad `as_name`, and I/O or
    /// SQLite errors from writing the slot.
    pub fn import_opened(&self, opened: OpenedBundle, as_name: Option<&str>) -> Result<SlotId> {
        let base = match as_name {
            Some(name) => SlotId::new(name)?.as_str().to_string(),
            None => opened.suggested_name.clone(),
        };
        let slot = self.unique_slot(&base)?;
        self.save(&slot, opened.manifest, &opened.data)?;
        Ok(slot)
    }

    /// The first free slot name based on `base`, compared without case.
    fn unique_slot(&self, base: &str) -> Result<SlotId> {
        let taken = self.taken_stems()?;
        for n in 1..=MAX_NAME_TRIES {
            let candidate = numbered(base, n);
            if !taken.contains(&candidate.to_ascii_lowercase()) {
                return SlotId::new(candidate);
            }
        }
        Err(StoreError::InvalidInput(format!(
            "no free slot name based on {base:?}"
        )))
    }

    /// Lowercase names of every file in the save folder, cut at the first
    /// dot. Slot names never contain a dot, so this is the slot name.
    fn taken_stems(&self) -> Result<BTreeSet<String>> {
        let mut out = BTreeSet::new();
        for entry in fs::read_dir(&self.dir)? {
            let name = entry?.file_name();
            if let Some(name) = name.to_str() {
                let stem = name.split('.').next().unwrap_or(name);
                out.insert(stem.to_ascii_lowercase());
            }
        }
        Ok(out)
    }
}

/// `base` for `n == 1`, otherwise `base-n`. `base` is cut so the result stays
/// within the slot name limit.
fn numbered(base: &str, n: u32) -> String {
    if n == 1 {
        return base.to_string();
    }
    let suffix = format!("-{n}");
    let keep = MAX_SLOT_LEN.saturating_sub(suffix.len()).min(base.len());
    let head = base.get(..keep).unwrap_or(base);
    format!("{head}{suffix}")
}

/// First free `<base>.<ext>` (or `<base>-n.<ext>`) in `folder`.
fn unique_path(folder: &Path, base: &str, ext: &str) -> Result<PathBuf> {
    for n in 1..=MAX_NAME_TRIES {
        let path = folder.join(format!("{}.{ext}", numbered(base, n)));
        if !path.exists() {
            return Ok(path);
        }
    }
    Err(StoreError::InvalidInput(format!(
        "no free file name based on {base:?} in {}",
        folder.display()
    )))
}

impl SaveStore for LocalStore {
    fn list(&self) -> Result<Vec<SlotSummary>> {
        let mut out = Vec::new();
        for entry in fs::read_dir(&self.dir)? {
            let name = entry?.file_name();
            let Some(stem) = name.to_str().and_then(|n| n.strip_suffix(MANIFEST)) else {
                continue;
            };
            // Names that are not valid slot names are not slots. Skip them.
            if let Ok(id) = SlotId::new(stem) {
                out.push(self.summarize(id));
            }
        }
        out.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(out)
    }

    fn read_manifest(&self, slot: &SlotId) -> Result<Manifest> {
        read_manifest_at(&self.file(slot, MANIFEST), slot)
    }

    fn save(&self, slot: &SlotId, mut manifest: Manifest, data: &SaveData) -> Result<()> {
        self.refuse_newer_existing(slot)?;
        self.remove_tmp_files(slot)?;

        let sav_tmp = self.file(slot, SAV_TMP);
        db::write_new(&sav_tmp, data)?;
        manifest.format_version = FORMAT_VERSION;
        manifest.checksum_hex = sha256_hex(&fs::read(&sav_tmp)?);

        let manifest_tmp = self.file(slot, MANIFEST_TMP);
        write_synced(&manifest_tmp, &serde_json::to_vec_pretty(&manifest)?)?;

        let sav = self.file(slot, SAV);
        let manifest_path = self.file(slot, MANIFEST);
        if self.current_is_good(slot) {
            fs::rename(&sav, self.file(slot, SAV_BAK))?;
            if manifest_path.exists() {
                fs::rename(&manifest_path, self.file(slot, MANIFEST_BAK))?;
            }
        }
        fs::rename(&sav_tmp, &sav)?;
        fs::rename(&manifest_tmp, &manifest_path)?;
        self.sync_dir();
        Ok(())
    }

    fn load(&self, slot: &SlotId) -> Result<Loaded> {
        self.load_verified(slot).map(|(loaded, _)| loaded)
    }

    fn delete(&self, slot: &SlotId) -> Result<()> {
        let all = [
            SAV,
            SAV_TMP,
            SAV_BAK,
            SAV_LOAD_TMP,
            MANIFEST,
            MANIFEST_TMP,
            MANIFEST_BAK,
        ];
        if !all.iter().any(|suffix| self.file(slot, suffix).exists()) {
            return Err(StoreError::NotFound(slot.to_string()));
        }
        // Remove the manifest first, so the slot disappears from `list`
        // before its save file is gone.
        remove_if_exists(&self.file(slot, MANIFEST))?;
        for suffix in all.into_iter().filter(|s| *s != MANIFEST) {
            remove_if_exists(&self.file(slot, suffix))?;
        }
        self.sync_dir();
        Ok(())
    }
}

/// Refuses files written by a format newer than [`FORMAT_VERSION`].
fn check_manifest_version(found: u32) -> Result<()> {
    if found > FORMAT_VERSION {
        Err(StoreError::NewerVersion {
            found,
            supported: FORMAT_VERSION,
        })
    } else {
        Ok(())
    }
}

fn read_manifest_at(path: &Path, slot: &SlotId) -> Result<Manifest> {
    match fs::read(path) {
        Ok(bytes) => Ok(serde_json::from_slice(&bytes)?),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            Err(StoreError::NotFound(slot.to_string()))
        }
        Err(err) => Err(err.into()),
    }
}

fn write_synced(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = File::create(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}

fn remove_if_exists(path: &Path) -> Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err.into()),
    }
}
