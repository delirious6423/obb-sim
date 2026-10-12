//! Local multi-slot save files for the baseball management game.
//!
//! The crate knows nothing about the game. It stores opaque byte blobs (a
//! snapshot, a command/event log tail, stored results) plus a small JSON
//! manifest that can be read without opening the database.
//!
//! Layout of a slot `<slot>` inside the store directory:
//!
//! - `<slot>.sav`: one SQLite file (journal mode DELETE, closed before rename).
//! - `<slot>.manifest.json`: pretty JSON with the checksum of the `.sav`.
//! - `<slot>.sav.bak` and `<slot>.manifest.json.bak`: the last good pair that
//!   was replaced by a save.
//!
//! A slot can also be exported as one portable `.bsave` file (see
//! [`LocalStore::export_to`] and [`LocalStore::import_file`]).
//!
//! Writes are atomic per file (write to `.tmp`, fsync, rename). See
//! [`LocalStore`] for the exact order and the crash guarantees.

mod bundle;
mod db;
mod error;
mod local;
mod model;
mod slot;
mod store;

pub use bundle::{BUNDLE_EXTENSION, MAX_BUNDLE_BYTES, sanitise_slot_name};
pub use error::{Result, StoreError};
pub use local::{DEFAULT_AUTO_SLOTS, LocalStore, OpenedBundle, QUICK_SLOT};
pub use model::{Manifest, SaveData, SaveKind};
pub use slot::{MAX_SLOT_LEN, SlotId};
pub use store::{Loaded, SaveStore, SlotSummary};

/// Version of the on-disk format written by this crate.
///
/// Stored in the manifest (`format_version`) and in the SQLite
/// `PRAGMA user_version`. Files with a newer version are refused.
pub const FORMAT_VERSION: u32 = 1;
