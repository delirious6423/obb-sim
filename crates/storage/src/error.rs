//! Error type for the storage crate.

use thiserror::Error;

/// Every failure the storage crate reports.
#[derive(Debug, Error)]
pub enum StoreError {
    /// File system failure.
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),
    /// SQLite failure while opening, reading, writing or migrating a `.sav`.
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    /// Malformed manifest JSON.
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    /// A slot name that breaks the naming rules.
    #[error("invalid slot name {name:?}: {reason}")]
    BadSlotName {
        /// The rejected name.
        name: String,
        /// Why it was rejected.
        reason: &'static str,
    },
    /// The slot has no manifest and no save file.
    #[error("no save in slot {0:?}")]
    NotFound(String),
    /// Data failed a check (checksum, layout, sequence numbers) and no
    /// usable backup exists. The message lists every failed candidate.
    #[error("save is corrupt: {0}")]
    Corrupt(String),
    /// The file was written by a newer format. It is left untouched.
    #[error("save format version {found} is newer than supported version {supported}")]
    NewerVersion {
        /// Version found in the file.
        found: u32,
        /// Highest version this crate can read.
        supported: u32,
    },
    /// The caller passed a value the format cannot store.
    #[error("invalid input: {0}")]
    InvalidInput(String),
    /// A `.bsave` file failed a check (marker, version, framing, checksum).
    #[error("not a valid save bundle: {0}")]
    Bundle(String),
}

/// Result alias used across the crate.
pub type Result<T, E = StoreError> = std::result::Result<T, E>;
