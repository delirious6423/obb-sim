//! The `.bsave` portable save file: one file that carries a slot's `.sav`
//! bytes and its manifest.
//!
//! # Layout (all integers little endian)
//!
//! | bytes | field |
//! | --- | --- |
//! | 8 | marker `BSIMSAVE` |
//! | 4 | bundle version (`u32`, currently 1) |
//! | 8 | header length `H` (`u64`) |
//! | `H` | header: UTF-8 JSON `{"slot": "...", "manifest": {...}}` |
//! | 8 | `.sav` length `S` (`u64`) |
//! | `S` | the `.sav` bytes, exactly as stored in a slot |
//! | 32 | SHA-256 of every byte before this trailer |
//!
//! The `slot` value in the header is a hint only. The importer never uses it
//! as a path and never trusts it as a slot name; see [`sanitise_slot_name`].
//!
//! [`decode`] checks, in this order: marker, bundle version, exact framing
//! (truncation and trailing bytes), the trailer checksum, the header JSON, the
//! manifest format version, and the manifest checksum of the `.sav` bytes.
//! Nothing is written to disk by this module.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::FORMAT_VERSION;
use crate::error::{Result, StoreError};
use crate::model::Manifest;
use crate::slot::MAX_SLOT_LEN;

/// File extension of a save bundle, without the dot.
pub const BUNDLE_EXTENSION: &str = "bsave";

/// Largest bundle the importer will read (1 GiB).
pub const MAX_BUNDLE_BYTES: u64 = 1 << 30;

/// Marker that starts every bundle.
pub const BUNDLE_MARKER: &[u8; 8] = b"BSIMSAVE";

/// Bundle layout version written by this crate.
pub const BUNDLE_VERSION: u32 = 1;

/// Name used when a bundle's slot hint sanitises to nothing.
const FALLBACK_NAME: &str = "imported";

const FIXED_PREFIX: usize = BUNDLE_MARKER.len() + 4 + 8;
const TRAILER: usize = 32;

#[derive(Serialize, Deserialize)]
struct Header {
    slot: String,
    manifest: Manifest,
}

/// A bundle that passed every check in [`decode`].
#[derive(Debug)]
pub(crate) struct Decoded {
    /// The slot name the file was exported from. A hint, not trusted.
    pub(crate) slot_hint: String,
    /// The manifest, with `checksum_hex` matching `sav`.
    pub(crate) manifest: Manifest,
    /// The verified `.sav` bytes.
    pub(crate) sav: Vec<u8>,
}

/// Builds the bundle bytes for one save.
pub(crate) fn encode(slot_hint: &str, manifest: &Manifest, sav: &[u8]) -> Result<Vec<u8>> {
    let header = serde_json::to_vec(&Header {
        slot: slot_hint.to_string(),
        manifest: manifest.clone(),
    })?;
    let mut out = Vec::with_capacity(FIXED_PREFIX + header.len() + sav.len() + TRAILER + 8);
    out.extend_from_slice(BUNDLE_MARKER);
    out.extend_from_slice(&BUNDLE_VERSION.to_le_bytes());
    out.extend_from_slice(&len_u64(header.len())?.to_le_bytes());
    out.extend_from_slice(&header);
    out.extend_from_slice(&len_u64(sav.len())?.to_le_bytes());
    out.extend_from_slice(sav);
    let trailer = Sha256::digest(&out);
    out.extend_from_slice(&trailer);
    Ok(out)
}

/// Checks and splits a bundle. See the module docs for the order of checks.
pub(crate) fn decode(bytes: &[u8]) -> Result<Decoded> {
    if bytes.len() < BUNDLE_MARKER.len() || &bytes[..BUNDLE_MARKER.len()] != BUNDLE_MARKER {
        return Err(bad("wrong marker: this is not a save bundle"));
    }
    if bytes.len() < FIXED_PREFIX + 8 + TRAILER {
        return Err(bad("truncated: shorter than the fixed fields"));
    }
    let version = u32::from_le_bytes(array(&bytes[8..12]));
    if version > BUNDLE_VERSION {
        return Err(bad(&format!(
            "bundle version {version} is newer than supported version {BUNDLE_VERSION}"
        )));
    }
    if version != BUNDLE_VERSION {
        return Err(bad(&format!("unsupported bundle version {version}")));
    }

    let (body, trailer) = bytes.split_at(bytes.len() - TRAILER);
    let mut pos = FIXED_PREFIX - 8;
    let header_len = take_len(body, &mut pos)?;
    let header = take(body, &mut pos, header_len)?;
    let sav_len = take_len(body, &mut pos)?;
    let sav = take(body, &mut pos, sav_len)?;
    if pos != body.len() {
        return Err(bad("trailing bytes after the .sav data"));
    }

    if Sha256::digest(body).as_slice() != trailer {
        return Err(bad("checksum mismatch: the file is damaged"));
    }

    let parsed: Header =
        serde_json::from_slice(header).map_err(|e| bad(&format!("header unreadable: {e}")))?;
    if parsed.manifest.format_version > FORMAT_VERSION {
        return Err(StoreError::NewerVersion {
            found: parsed.manifest.format_version,
            supported: FORMAT_VERSION,
        });
    }
    let actual = sha256_hex(sav);
    if actual != parsed.manifest.checksum_hex {
        return Err(StoreError::Corrupt(format!(
            "bundle .sav checksum mismatch (manifest {}, file {actual})",
            parsed.manifest.checksum_hex
        )));
    }
    Ok(Decoded {
        slot_hint: parsed.slot,
        manifest: parsed.manifest,
        sav: sav.to_vec(),
    })
}

/// Turns any text into a valid slot name base: ASCII letters, digits, space,
/// `-` and `_` are kept, every other character becomes `-`. The result is at
/// most [`MAX_SLOT_LEN`] bytes. Empty input gives `imported`.
///
/// This is used on the slot hint stored in a bundle. Names typed by a user are
/// validated with `SlotId::new` instead.
#[must_use]
pub fn sanitise_slot_name(raw: &str) -> String {
    let mapped: String = raw
        .chars()
        .take(MAX_SLOT_LEN)
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == ' ' || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect();
    let trimmed = mapped.trim();
    if trimmed.chars().all(|c| c == '-') || trimmed.is_empty() {
        FALLBACK_NAME.to_string()
    } else {
        trimmed.to_string()
    }
}

/// SHA-256 of `bytes` as lowercase hex.
pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest.iter() {
        let _ = write!(out, "{byte:02x}");
    }
    out
}

fn bad(reason: &str) -> StoreError {
    StoreError::Bundle(reason.to_string())
}

fn len_u64(len: usize) -> Result<u64> {
    u64::try_from(len).map_err(|_| bad("section too large"))
}

fn array(slice: &[u8]) -> [u8; 4] {
    let mut out = [0u8; 4];
    out.copy_from_slice(slice);
    out
}

fn take_len(body: &[u8], pos: &mut usize) -> Result<usize> {
    let raw = take(body, pos, 8)?;
    let mut buf = [0u8; 8];
    buf.copy_from_slice(raw);
    usize::try_from(u64::from_le_bytes(buf)).map_err(|_| bad("section length out of range"))
}

fn take<'a>(body: &'a [u8], pos: &mut usize, len: usize) -> Result<&'a [u8]> {
    let end = pos
        .checked_add(len)
        .filter(|end| *end <= body.len())
        .ok_or_else(|| bad("truncated: a section runs past the end of the file"))?;
    let slice = &body[*pos..end];
    *pos = end;
    Ok(slice)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::SaveKind;

    fn manifest(checksum: String) -> Manifest {
        Manifest {
            format_version: FORMAT_VERSION,
            engine_version: "test".into(),
            league_name: "L".into(),
            team_name: "T".into(),
            role: "gm".into(),
            in_game_date: "2031-04-01".into(),
            difficulty: "normal".into(),
            saved_at_unix: 1,
            kind: SaveKind::Manual,
            thumbnail_png: None,
            checksum_hex: checksum,
        }
    }

    #[test]
    fn encode_then_decode_round_trips() {
        let sav = b"sqlite bytes".to_vec();
        let m = manifest(sha256_hex(&sav));
        let bytes = encode("my slot", &m, &sav).expect("encode");
        let back = decode(&bytes).expect("decode");
        assert_eq!(back.slot_hint, "my slot");
        assert_eq!(back.sav, sav);
        assert_eq!(back.manifest.league_name, "L");
    }

    #[test]
    fn sanitise_removes_path_parts() {
        assert_eq!(sanitise_slot_name("../../etc/passwd"), "------etc-passwd");
        assert_eq!(sanitise_slot_name("   "), "imported");
        assert_eq!(sanitise_slot_name(""), "imported");
        assert_eq!(sanitise_slot_name("ok_name-1"), "ok_name-1");
        assert!(sanitise_slot_name(&"x".repeat(200)).len() <= MAX_SLOT_LEN);
    }
}
