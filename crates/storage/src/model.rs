//! Data types stored in a save slot. All of them are serde types.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Why a save was written.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SaveKind {
    /// Written by the player.
    Manual,
    /// Written by the rotating autosave.
    Auto,
    /// Written by the quicksave slot.
    Quick,
}

/// Small summary stored next to the `.sav` as JSON.
///
/// The caller fills every field except `format_version` and `checksum_hex`.
/// [`crate::SaveStore::save`] overwrites those two with the values it writes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    /// Format version of the `.sav`. Set by the store on save.
    pub format_version: u32,
    /// Version of the game engine that wrote the save.
    pub engine_version: String,
    /// Name of the league shown in the load screen.
    pub league_name: String,
    /// Name of the user's team.
    pub team_name: String,
    /// Role the user played (opaque to the store).
    pub role: String,
    /// In-game date as text (opaque to the store).
    pub in_game_date: String,
    /// Difficulty label (opaque to the store).
    pub difficulty: String,
    /// Unix time of the save, supplied by the caller. The store reads no clock.
    pub saved_at_unix: u64,
    /// Why the save was written.
    pub kind: SaveKind,
    /// Optional PNG thumbnail for the load screen.
    pub thumbnail_png: Option<Vec<u8>>,
    /// SHA-256 of the `.sav` bytes, lowercase hex. Set by the store on save.
    pub checksum_hex: String,
    /// Order of the autosave writes, set by [`crate::LocalStore::autosave`].
    /// Two autosaves in the same second are told apart by this number.
    /// Manifests written before it existed read as 0.
    #[serde(default)]
    pub autosave_seq: u64,
}

/// Everything stored inside one `.sav`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct SaveData {
    /// Latest snapshot of the game state (opaque).
    pub snapshot: Vec<u8>,
    /// Number of log entries the snapshot covers. The first entry of `log`
    /// has index `snapshot_seq`.
    pub snapshot_seq: u64,
    /// Serialised command/event entries after the snapshot, in order (opaque).
    pub log: Vec<Vec<u8>>,
    /// Stored game results (opaque). May be empty.
    pub results: Vec<Vec<u8>>,
    /// Caller key/value pairs.
    pub meta: BTreeMap<String, String>,
}
