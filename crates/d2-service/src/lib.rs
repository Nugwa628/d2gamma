//! `d2-service`: what every D2RUST server program shares, whatever it
//! serves. It builds for Linux and Windows: the game server ships on both.
//!
//! - [`config`]: the `key = value` config file reader.
//! - [`identity`]: the program's certificate, key and fingerprint.
//! - [`log`]: the program's log file in its log folder.
//! - [`status`]: `status.json` and the data folder's lock.
//! - [`ready`]: the one line a program prints once it serves.
//! - [`stop`]: the stop signals (SIGTERM and Ctrl+C on Linux; Ctrl+C,
//!   Ctrl+Break and the console's close on Windows).
//! - [`character`]: the account and new-character types a seat carries
//!   between the accounts and the games.
//! - [`datahash`]: the game-data hash a game server and its main server
//!   compare.

pub mod character;
pub mod config;
pub mod datahash;
pub mod identity;
pub mod log;
pub mod ready;
pub mod status;
pub mod stop;
