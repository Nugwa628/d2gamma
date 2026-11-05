//! The one error type of the mod kit's file work.

use crate::manifest::ManifestError;

/// Something wrong with a mod folder or a mod pack. The messages are
/// written to be shown to a modder as they are.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ModError {
    /// A file or folder could not be read or written.
    #[error("{path}: {message}")]
    Io { path: String, message: String },
    /// The manifest is invalid.
    #[error("{0}")]
    Manifest(#[from] ManifestError),
    /// The folder does not follow the mod folder layout.
    #[error("{path}: {message}")]
    Layout { path: String, message: String },
    /// The file is not a readable mod pack (or is cut short).
    #[error("not a readable mod pack: {0}")]
    BadPack(String),
    /// The pack's bytes do not match the content hash recorded in it.
    #[error(
        "the pack was changed or damaged after it was made (its content hash is {actual}, \
         the pack says {expected})"
    )]
    HashMismatch { expected: String, actual: String },
    /// The pack carries a signature that does not verify.
    #[error("the pack's signature is not valid for its content")]
    BadSignature,
    /// A mod could not be found or cannot be started; the message says why.
    #[error("{0}")]
    Load(String),
    /// The pack is signed with a signature type this build does not know.
    #[error("the pack uses signature type `{0}`, which this version does not know")]
    UnknownSignature(String),
}

impl ModError {
    pub(crate) fn io(path: impl std::fmt::Display, e: std::io::Error) -> ModError {
        ModError::Io {
            path: path.to_string(),
            message: e.to_string(),
        }
    }

    pub(crate) fn layout(path: impl Into<String>, message: impl Into<String>) -> ModError {
        ModError::Layout {
            path: path.into(),
            message: message.into(),
        }
    }
}
