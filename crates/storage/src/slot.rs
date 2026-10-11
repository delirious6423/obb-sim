//! Validated slot names.

use serde::{Deserialize, Serialize};

use crate::error::{Result, StoreError};

/// Longest allowed slot name, in bytes (all allowed characters are ASCII).
pub const MAX_SLOT_LEN: usize = 64;

/// A validated slot name: 1 to 64 characters from ASCII letters, digits,
/// space, `-` and `_`. Path separators and dots are never allowed, so a
/// slot name is always a single safe file-name component.
///
/// The inner string is private: every value goes through [`SlotId::new`],
/// including deserialisation.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct SlotId(String);

impl SlotId {
    /// Validates `name` and wraps it.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError::BadSlotName`] when the name is empty, longer than
    /// [`MAX_SLOT_LEN`], or contains a character outside the allowed set.
    pub fn new(name: impl Into<String>) -> Result<Self> {
        let name = name.into();
        let reason = if name.is_empty() {
            Some("name is empty")
        } else if name.len() > MAX_SLOT_LEN {
            Some("longer than 64 characters")
        } else if !name.bytes().all(is_allowed) {
            Some("only ASCII letters, digits, space, '-' and '_' are allowed")
        } else {
            None
        };
        match reason {
            Some(reason) => Err(StoreError::BadSlotName { name, reason }),
            None => Ok(Self(name)),
        }
    }

    /// The validated name.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

fn is_allowed(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b' ' | b'-' | b'_')
}

impl TryFrom<String> for SlotId {
    type Error = StoreError;

    fn try_from(name: String) -> Result<Self> {
        Self::new(name)
    }
}

impl From<SlotId> for String {
    fn from(id: SlotId) -> Self {
        id.0
    }
}

impl std::fmt::Display for SlotId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
