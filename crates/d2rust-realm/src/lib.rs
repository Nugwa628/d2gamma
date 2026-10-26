//! `d2rust-realm`: the main server (multiplayer v2, contracts 11.1). It
//! runs no games: it holds the sessions, the accounts, the lobby and chat,
//! the ladder, the shared stash and market, and the game-server list; its game servers (`d2rust-node`) link to it
//! and run the games.
//!
//! - [`config`]: the main server's settings, read from a `key = value`
//!   file.
//! - [`net`]: the QUIC endpoint's connections: players' handshakes and
//!   requests, and game servers' links handed to the [`realm`].
//! - [`session`]: sessions and the seam to the lobby.
//! - [`lobby`]: the game list, create and join rules and chat.
//! - [`realm`]: the game-server link's main-server side, the game-server
//!   list and the bundles.
//! - [`console`]: a test setup's lines on standard input (`dev = test`).
//! - [`economy`]: the online stash's economy, on a thread of its own.
//! - [`market`]: the market's messages on a player's session.
//! - [`ladder`]: the ladder's seasons, rows, rank tables and requests.
//! - [`social`]: the realm's chat rooms, DMs, user lists, guilds and
//!   friends on a player's session.
//! - [`upgrade`]: the countdown to a restart for a new version, in Global,
//!   in every game and on every game server.
//!
//! The accounts and the character folder are `d2-accounts`'.

pub use d2_accounts as accounts;
pub use d2_accounts::denied;
pub use d2_service::log;

pub mod config;
pub mod console;
pub mod economy;
pub mod ladder;
pub mod lobby;
pub mod market;
pub mod net;
pub mod realm;
pub mod session;
pub mod social;
pub mod upgrade;
