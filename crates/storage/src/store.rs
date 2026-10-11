//! The storage trait and the types it returns.

use crate::error::Result;
use crate::model::{Manifest, SaveData};
use crate::slot::SlotId;

/// One row of [`SaveStore::list`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlotSummary {
    /// The slot name.
    pub id: SlotId,
    /// The manifest, when it could be read. `None` when it is missing or broken.
    pub manifest: Option<Manifest>,
    /// Human-readable problem with the slot, if any. A slot can have a
    /// manifest and still have a problem (for example a missing `.sav`).
    pub problem: Option<String>,
}

/// Result of [`SaveStore::load`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Loaded {
    /// The manifest that matched the loaded data.
    pub manifest: Manifest,
    /// The save contents.
    pub data: SaveData,
    /// `true` when the primary `.sav` failed its checks and the backup was used.
    pub used_backup: bool,
}

/// Storage backend for save slots.
///
/// The trait is object safe. Implementations must keep these rules:
///
/// - [`list`](SaveStore::list) and [`read_manifest`](SaveStore::read_manifest)
///   must not open the save database.
/// - A slot holds one logical save. Each `save` replaces the previous one.
/// - **A Steam Cloud backend must write each revision to a NEW filename**
///   and then update its index, because cloud sync cannot overwrite a file
///   atomically. The local store does overwrite: it writes the new revision
///   to a temporary file and renames it into place, keeping the previous
///   revision as `.bak`.
pub trait SaveStore {
    /// Lists every slot that has a manifest file, sorted by slot name.
    ///
    /// Unreadable manifests are returned with `manifest: None` and a
    /// `problem`. They do not make the whole call fail.
    ///
    /// # Errors
    ///
    /// Returns an error only when the store directory cannot be read.
    fn list(&self) -> Result<Vec<SlotSummary>>;

    /// Reads the manifest of one slot without opening the save database.
    ///
    /// # Errors
    ///
    /// Returns [`crate::StoreError::NotFound`] when the slot has no manifest,
    /// or a JSON or I/O error when it cannot be read.
    fn read_manifest(&self, slot: &SlotId) -> Result<Manifest>;

    /// Writes `data` and `manifest` into `slot`, replacing the previous save.
    ///
    /// `format_version` and `checksum_hex` of `manifest` are overwritten.
    ///
    /// # Errors
    ///
    /// Returns [`crate::StoreError::NewerVersion`] when the slot holds a save
    /// written by a newer format. That file is not modified.
    fn save(&self, slot: &SlotId, manifest: Manifest, data: &SaveData) -> Result<()>;

    /// Loads and verifies a slot. Falls back to the backup when the primary
    /// file fails its checksum or cannot be opened.
    ///
    /// # Errors
    ///
    /// Returns [`crate::StoreError::NotFound`] when the slot is empty,
    /// [`crate::StoreError::Corrupt`] when no candidate passes its checks, and
    /// [`crate::StoreError::NewerVersion`] for files from a newer format.
    fn load(&self, slot: &SlotId) -> Result<Loaded>;

    /// Removes a slot and its backup and temporary files.
    ///
    /// # Errors
    ///
    /// Returns [`crate::StoreError::NotFound`] when the slot has no files.
    fn delete(&self, slot: &SlotId) -> Result<()>;
}
