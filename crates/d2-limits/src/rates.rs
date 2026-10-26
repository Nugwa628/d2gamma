//! The default limits, and the key each is counted under.
//!
//! | Limit | Key | Default | From |
//! |---|---|---|---|
//! | [`LOGIN`] | address | 10 per minute | contracts 5 |
//! | [`REGISTER`] | address | 3 per hour | contracts 5 |
//! | [`CREATE_GAME`] | account | 5 per minute | contracts 5 |
//! | [`JOIN_WRONG_PASSWORD`] | account | 5 per minute | contracts 5 |
//! | [`JOIN`] | account | 10 per minute | proposed here |
//! | [`TICKET`] | account | 10 per minute | proposed here |
//! | [`CHAT`] | account | 5 lines per 10 s | contracts 5 |
//! | [`MARKET_SEARCH`] | account | 30 per minute | proposed here (plan E2, open question 23) |
//! | [`MARKET_POST`] | account | 10 per minute | proposed here (plan E2, open question 23) |
//! | [`PING`] | address | 10 per 10 s | contracts 11.4 |
//! | [`HANDSHAKE`] | address | 20 per 10 s | proposed here |
//! | [`LINK_MESSAGES`] | game server | 500 per second | proposed here (plan C6) |
//! | [`SAVES`] | game | 60 per minute | proposed here (plan C6) |
//! | [`JOURNAL_BYTES`] | game | 4 MiB, refilled at 64 KiB/s | proposed here (plan C6) |
//!
//! The contracts count create, join and chat per session; an account
//! holds one session at a time, so the account is the same key and
//! doesn't reset when the session reconnects. The proposed ones are first
//! picks for limits the plan names without a number; each is this one
//! constant.

use std::time::Duration;

use crate::Rate;

/// `Login` per address (a token bucket, contracts section 5).
pub const LOGIN: Rate = Rate::per_secs(10, 60);
/// `Register` per address.
pub const REGISTER: Rate = Rate::per_secs(3, 3600);
/// `CreateGame` per account.
pub const CREATE_GAME: Rate = Rate::per_secs(5, 60);
/// `JoinGame` with a wrong game password, per account.
pub const JOIN_WRONG_PASSWORD: Rate = Rate::per_secs(5, 60);
/// `JoinGame` per account, whatever the answer: a player joins a few games
/// a minute at most.
pub const JOIN: Rate = Rate::per_secs(10, 60);
/// Join tickets issued per account (`JoinNode`, including the new ticket a
/// `GameStatus` after a lost connection gets).
pub const TICKET: Rate = Rate::per_secs(10, 60);
/// `Chat` lines per account.
pub const CHAT: Rate = Rate::per_secs(5, 10);
/// Market searches per account.
pub const MARKET_SEARCH: Rate = Rate::per_secs(30, 60);
/// Market posts per account: list, offer, counter, buy, cancel.
pub const MARKET_POST: Rate = Rate::per_secs(10, 60);
/// Pings (ALPN `d2rust-ping/1`) per address, on a game server's port and
/// the main server's observer endpoint (contracts 11.4). The observer
/// endpoint answers pings only, so there it is checked before the
/// handshake ([`crate::AddrGate`]).
pub const PING: Rate = Rate::per_secs(10, 10);
/// Connection attempts per address on an endpoint that serves players and
/// pings together (a game server's port, the main server's port),
/// checked before the handshake: the ALPN isn't known yet, so this is the
/// ping limit with room for a player's own connections. After the
/// handshake a ping still counts toward [`PING`].
pub const HANDSHAKE: Rate = Rate::per_secs(20, 10);
/// Messages per second on one game server's link to the main server.
pub const LINK_MESSAGES: Rate = Rate::per_secs(500, 1);
/// Saves per game per minute: 8 players saving every 30 s is 16, the rest
/// is room for the immediate saves of stash moves and trades.
pub const SAVES: Rate = Rate::per_secs(60, 60);
/// Journal bytes per game: a 4 MiB burst refilled at 64 KiB/s, above the
/// plan's estimate for 8 busy players (30 MB per game-hour, ~8.5 KB/s).
/// Count a chunk's body after inflating it, not its gzip bytes: gzip of
/// repeated records shrinks them about a thousand times, so a limit on
/// the compressed bytes would let a game server send gigabytes.
pub const JOURNAL_BYTES: Rate = Rate::new(4 << 20, Duration::from_secs(64));
