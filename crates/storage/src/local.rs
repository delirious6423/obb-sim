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

use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

use rusqlite::{Connection, OpenFlags};
use sha2::{Digest, Sha256};

use crate::FORMAT_VERSION;
use crate::db;
use crate::error::{Result, StoreError};
use crate::model::{Manifest, SaveData, SaveKind};
use crate::slot::SlotId;
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

    /// Tries one `(sav, manifest)` pair. Returns the verified save or the
    /// reason it failed.
    fn try_candidate(
        &self,
        slot: &SlotId,
        sav_suffix: &str,
        manifest_suffix: &str,
    ) -> Result<(Manifest, SaveData)> {
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
        Ok((manifest, data))
    }
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
                Ok((manifest, data)) => {
                    return Ok(Loaded {
                        manifest,
                        data,
                        used_backup,
                    });
                }
                // A newer format is not a damaged file. Falling back to an
                // older backup would silently lose data, so stop here.
                Err(err @ StoreError::NewerVersion { .. }) => return Err(err),
                Err(err) => problems.push(format!("{sav_suffix}/{manifest_suffix}: {err}")),
            }
        }
        Err(StoreError::Corrupt(problems.join("; ")))
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

/// SHA-256 of `bytes` as lowercase hex.
fn sha256_hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest.iter() {
        let _ = write!(out, "{byte:02x}");
    }
    out
}
