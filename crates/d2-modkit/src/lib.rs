//! The D2Gamma mod format.
//!
//! A pure library (no Bevy, no renderer, no game code) shared by D2Gamma,
//! which loads mods, and the Mod Studio tool, which edits and packages them.
//!
//! - [`manifest`]: `mod.toml`, parsed strictly and validated with messages
//!   that name the field; the rules for building on another mod and for
//!   add-ons.
//! - [`version`]: mod versions and the version ranges a mod names for the
//!   mod it builds on.
//! - [`layout`]: the folder a mod is edited in (readable files) and how a
//!   new one is started.
//! - [`pack`]: a mod pack seen through one trait (a folder, the data pack and
//!   memory are sources): the manifest read back without the rest, the
//!   container-independent content hash.
//! - [`resolve`]: which mod folders to stack for `--mod <id>` / `--mod-dir`,
//!   with the checks that make a refusal readable.
//! - [`sign`]: the signature format over that hash (Ed25519) and its check.
//! - [`api`]: the modding API version, the supported window and the
//!   player-facing "needs updating" / "needs a newer D2Gamma" check.
//!
//! Loading a mod into the game is not here; it lives with the game.

pub mod api;
pub mod error;
pub mod layout;
pub mod manifest;
pub mod pack;
pub mod publish;
pub mod resolve;
pub mod sign;
pub mod ui;
pub mod version;

#[cfg(test)]
mod testutil;

pub use api::{check_api, check_api_now, ApiCheck, CURRENT_API};
pub use error::ModError;
pub use layout::ModFolder;
pub use manifest::{Kind, Manifest, ManifestError};
pub use pack::{content_hash, read_manifest, MemoryPack, PackSource};
pub use resolve::{resolve, ModSpec, ModStack, RulesFile};
pub use sign::PackSignature;
pub use version::{Version, VersionReq};

pub mod effects;

pub mod net;

pub mod graph;

pub mod addons;
