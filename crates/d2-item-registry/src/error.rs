//! Failures of the registry, and why an ID is refused.

use std::fmt;

use crate::Where;

/// Why one ID of a save or a transfer is refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// The ID is not registered and was not granted to the game.
    NotGranted,
    /// The ID is somewhere else: another character outside the game, a
    /// stash, escrow, a delivery, in flight to another character (a
    /// duplicate).
    HeldElsewhere(Where),
    /// The ID belongs to another split of the economy (`EconomyMismatch`).
    EconomyMismatch { split: String },
    /// The ID is twice in the same save.
    Twice,
}

impl Refusal {
    /// The link's error code for a refused transfer: 89 `EconomyMismatch`,
    /// else 109 `TransferInvalid`. A refused save is 105 `SaveInvalid`.
    pub fn transfer_code(&self) -> u16 {
        match self {
            Refusal::EconomyMismatch { .. } => 89,
            _ => 109,
        }
    }
}

impl fmt::Display for Refusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Refusal::NotGranted => f.write_str("not granted to this game"),
            Refusal::HeldElsewhere(w) => write!(f, "held by {w}"),
            Refusal::EconomyMismatch { split } => write!(f, "belongs to economy {split}"),
            Refusal::Twice => f.write_str("twice in the save"),
        }
    }
}

/// A registry failure.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    #[error("item registry database: {0}")]
    Db(String),
    #[error("item registry: the database belongs to mod `{theirs}`, not `{ours}`")]
    OtherMod { ours: String, theirs: String },
    #[error("item registry: schema version {0} is newer than this server's")]
    NewerSchema(i64),
    #[error("item registry: every item ID is granted")]
    Exhausted,
    #[error("item registry: item ID {0:#010x} is not registered")]
    Unregistered(u32),
    #[error("item registry: item ID {id:#010x} refused: {why}")]
    Refused { id: u32, why: Refusal },
    #[error("item registry config: {0}")]
    Config(&'static str),
}

impl From<rusqlite::Error> for Error {
    fn from(e: rusqlite::Error) -> Self {
        Error::Db(e.to_string())
    }
}
