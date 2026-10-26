//! "Host on my PC" (multiplayer v2 plan, module N8), the game client's
//! side: the game client starts the game server program, `d2rust-node
//! --host` (`D2RUST-Node.exe` beside the game on Windows), as **its own
//! process** and drives it over the line protocol of contracts 11.12. The
//! game server never runs inside the game client, and this crate holds no
//! game-server code: it starts, talks to and watches a child process.
//!
//! - The host token from `HostGrant` goes to the child on its stdin, in
//!   the start line ([`StartLine`]), never on its command line or into a
//!   log; [`HostToken`] hides it from `Debug` and wipes it on drop.
//! - The child's lines come back as [`HostEvent`]s and in a
//!   [`HostStatus`]: `ready` (its id, its listen address, where the host's
//!   own client connects: [`Ready::connect_addr`]), `reach`, `game`,
//!   `closed`, `error`.
//! - [`HostProcess::sleep`] passes the Windows sleep notice on
//!   ([`HostProcess::power_message`] reads `WM_POWERBROADCAST`
//!   `PBT_APMSUSPEND`), [`HostProcess::stop`] a clean quit, and
//!   [`HostProcess::host_left_game`] ends the game when the host leaves it
//!   ([`GAME_ENDS_WHEN_HOST_LEAVES`]). Each makes the game server save
//!   everyone and close the game.
//! - Dropping the handle closes the child's stdin, so the game server
//!   saves, closes and exits; a crash of the game client closes the pipe
//!   the same way. A child that is still running [`STOP_GRACE`] after
//!   `stop` is killed.
//! - A crashed child is started again only by its [`RestartPolicy`]
//!   ([`DEFAULT_RESTART`]: once, before it linked).
//! - Reachability: the game server opens its port and the main server
//!   checks it from outside (`d2-reach`, in the game server); the `reach`
//!   line becomes the panel's [`ReachView`] ("Opening port...",
//!   "Reachable", "Not reachable: see help") and [`support_details`].
//!
//! ```no_run
//! use d2_host_control::{node_program_beside_exe, HostConfig, HostEvent, HostProcess, StartLine};
//! # fn grant() -> ([u8; 16], String, [u8; 32]) { unimplemented!() }
//! let (token, realm_addr, realm_cert) = grant(); // HostGrant, and the pinned realm certificate
//! let start = StartLine::from_grant(token, &realm_addr, realm_cert, "0.1.3.0")?;
//! let host = HostProcess::spawn(HostConfig::new(node_program_beside_exe()?, start))?;
//! // Each frame:
//! while let Some(e) = host.poll() {
//!     if let HostEvent::Ready(r) = e {
//!         let _play_on = r.connect_addr(); // join the own game over loopback
//!     }
//! }
//! let _panel_line = host.status().reach_view().text();
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

mod lines;
mod power;
mod process;
mod reach;

pub use lines::{
    parse_hex32, parse_node_line, GameInfo, HostCommand, HostToken, LineError, NodeError, NodeLine,
    Ready, StartLine, DEFAULT_PORT, MAX_LINE,
};
pub use power::{is_sleep_notice, PBT_APMSUSPEND, WM_POWERBROADCAST};
pub use process::{
    node_program_beside_exe, node_program_in, ExitKind, HostConfig, HostError, HostEvent,
    HostProcess, HostStatus, Phase, RestartPolicy, DEFAULT_RESTART, GAME_ENDS_WHEN_HOST_LEAVES,
    NODE_PROGRAM, STDERR_TAIL, STOP_GRACE,
};
pub use reach::{
    support_details, NatKind, PortMethod, ReachHow, ReachReport, ReachState, ReachView,
};
