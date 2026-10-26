//! The game server's settings (contracts 11.8), read with the server
//! programs' shared reader ([`d2_service::config`]). Relative paths are
//! taken from the config file's folder.
//!
//! ```text
//! # d2rust-node.cfg
//! bundle      = node-eu-1.bundle     # from d2rust-realm --add-node (required)
//! identity    = identity             # its certificate and key
//! listen      = 0.0.0.0:6113         # players connect here; the link leaves from here
//! public_addr = 203.0.113.20:6113    # what players are told, when not the address seen
//! game_data   = /srv/d2rust/game     # required
//! game        = d2rust                 # must match the binary's NodeGame and bundle
//! max_games   = 16
//! max_players = 8                    # per game
//! frame_ms    = 1
//! save_interval_s = 60                # until the main server's welcome sets it
//! grace_s     = 30
//! logs        = logs
//! realm_addr  = 127.0.0.1:6112       # instead of the bundle's address (the pin stays)
//! update_command = server-ctl        # fetches a version for an upgrade (`… fetch V`)
//! ```
//!
//! `game` optionally checks the game id provided by the binary's
//! [`crate::NodeGame`] ([`NodeConfig::check_game`]) and refuses a bundle
//! for another realm ([`NodeConfig::check_bundle`]). The binary's game id
//! is always used for its hello and rules; config cannot change it.
//! Run a second realm's game servers from their own config, bundle and
//! identity folders.
//!
//! `update_command` is the program (in the config's folder, else on the
//! path) and its first words; the updater adds `fetch VERSION`
//! ([`crate::update`]). On Windows: `powershell.exe -NoProfile
//! -NonInteractive -ExecutionPolicy Bypass -File node-ctl.ps1`. Without it the game server
//! shows an upgrade's lines but fetches nothing. `update_command = test`
//! (with `dev = test` only) stages every version at once.
//!
//! A game server keeps no characters and no accounts: `data`,
//! `characters`, `accounts` and `backups` are refused. `dev = test` is for
//! test setups. `hostile = MODE` is taken only by a test build (cargo
//! feature `hostile`).

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::Duration;

use d2_games::{GamesConfig, GamesSettings};
use d2_link::Bundle;
use d2_net::msg::node::valid_game_id;
use d2_service::config::{self, set_once, ConfigError};

use crate::update::Fetcher;

/// The default listen address: every interface, port 6113.
pub const DEFAULT_LISTEN: &str = "0.0.0.0:6113";

/// The highest player limit of a game (as the main server's).
pub const MAX_PLAYERS_LIMIT: u8 = 64;

/// The ways a test build misbehaves (contracts 11.10).
pub const HOSTILE_MODES: &[&str] = &[
    "drop-change",
    "invent-intent",
    "alter-save",
    "copy-item",
    "impossible-item",
    // Every item put in the stash goes up with an item ID never granted.
    "invent-stash-item",
];

/// The game server's settings.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NodeConfig {
    /// Its bundle (`node-ID.bundle`).
    pub bundle: PathBuf,
    /// Its identity folder.
    pub identity: PathBuf,
    /// Where players connect, and the link's local end.
    pub listen: SocketAddr,
    /// The address players are told, when set.
    pub public_addr: Option<SocketAddr>,
    pub game_data: PathBuf,
    /// The expected game id; must match the binary's `NodeGame`.
    pub game: Option<String>,
    pub max_games: u16,
    /// The most players of one game.
    pub max_players: u8,
    pub frame_pause: Duration,
    pub save_interval: Duration,
    pub grace: Duration,
    pub logs: PathBuf,
    /// The main server's address instead of the bundle's.
    pub realm_addr: Option<SocketAddr>,
    pub dev_test: bool,
    /// A test build's misbehaviour.
    pub hostile: Option<String>,
    /// How it fetches a version for an upgrade.
    pub update: Fetcher,
    /// The config file's folder: the game server's (its `run/swap`).
    pub folder: PathBuf,
}

/// Keys a game server refuses: it keeps no characters or accounts.
const REFUSED: &[&str] = &["data", "characters", "accounts", "backups"];

impl NodeConfig {
    /// Reads and parses the config file at `path`.
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        Self::parse(&config::read(path)?, config::folder(path))
    }

    /// Parses config text; relative paths are taken from `base`.
    pub fn parse(text: &str, base: &Path) -> Result<Self, ConfigError> {
        let mut bundle = None;
        let mut identity = None;
        let mut listen = None;
        let mut public_addr = None;
        let mut game_data = None;
        let mut game = None;
        let mut max_games = None;
        let mut max_players = None;
        let mut frame_ms = None;
        let mut save_interval_s = None;
        let mut grace_s = None;
        let mut logs = None;
        let mut realm_addr = None;
        let mut dev_test = None;
        let mut hostile = None;
        let mut update: Option<(usize, Vec<String>)> = None;
        for entry in config::entries(text) {
            let e = entry?;
            match e.key {
                "bundle" => set_once(&mut bundle, e.path(base), &e)?,
                "identity" => set_once(&mut identity, e.path(base), &e)?,
                "listen" => set_once(&mut listen, e.parse::<SocketAddr>()?, &e)?,
                "public_addr" => set_once(&mut public_addr, e.parse::<SocketAddr>()?, &e)?,
                "game_data" => set_once(&mut game_data, e.path(base), &e)?,
                "game" => {
                    if !valid_game_id(e.value) {
                        return Err(e.bad("1-15 of a-z, 0-9 and -"));
                    }
                    set_once(&mut game, e.value.to_owned(), &e)?;
                }
                "max_games" => set_once(&mut max_games, e.number(1..=1024u16)?, &e)?,
                "max_players" => {
                    set_once(&mut max_players, e.number(1..=MAX_PLAYERS_LIMIT)?, &e)?;
                }
                "frame_ms" => set_once(&mut frame_ms, e.number(0..=1000u32)?, &e)?,
                "save_interval_s" => {
                    set_once(&mut save_interval_s, e.number(1..=3600u32)?, &e)?;
                }
                "grace_s" => set_once(&mut grace_s, e.parse::<u32>()?, &e)?,
                "logs" => set_once(&mut logs, e.path(base), &e)?,
                "realm_addr" => set_once(&mut realm_addr, e.parse::<SocketAddr>()?, &e)?,
                "dev" => {
                    let v = match e.value {
                        "test" => true,
                        "off" => false,
                        _ => return Err(e.bad("`test` or `off`")),
                    };
                    set_once(&mut dev_test, v, &e)?;
                }
                "hostile" => {
                    if !cfg!(feature = "hostile") {
                        return Err(e.bad("only a test build takes it (cargo feature `hostile`)"));
                    }
                    if !HOSTILE_MODES.contains(&e.value) {
                        return Err(e.bad(HOSTILE_MODES.join(" | ")));
                    }
                    set_once(&mut hostile, e.value.to_owned(), &e)?;
                }
                "update_command" => {
                    let words: Vec<String> = e.value.split_whitespace().map(Into::into).collect();
                    if words.is_empty() {
                        return Err(e.bad("a program, as `server-ctl`"));
                    }
                    set_once(&mut update, (e.line, words), &e)?;
                }
                k if REFUSED.contains(&k) => {
                    return Err(e.bad(
                        "a game server keeps no characters or accounts: the main server does",
                    ));
                }
                _ => return Err(e.unknown()),
            }
        }
        let dev_test = dev_test.unwrap_or(false);
        let update = match update {
            None => Fetcher::Off,
            Some((_, w)) if w == ["test"] && dev_test => Fetcher::Test,
            Some((line, w)) if w == ["test"] => {
                return Err(ConfigError::Value {
                    line,
                    key: "update_command".into(),
                    reason: "`test` is for test setups (`dev = test`)".into(),
                })
            }
            Some((_, words)) => Fetcher::Run {
                words,
                folder: base.to_owned(),
            },
        };
        Ok(NodeConfig {
            bundle: bundle.ok_or(ConfigError::Missing {
                key: "bundle",
                why: "the node-ID.bundle file d2rust-realm --add-node wrote",
            })?,
            identity: identity.unwrap_or_else(|| base.join("identity")),
            listen: listen.unwrap_or_else(|| DEFAULT_LISTEN.parse().expect("a valid address")),
            public_addr,
            game_data: game_data.ok_or(ConfigError::NoGameData)?,
            game,
            max_games: max_games.unwrap_or(16),
            max_players: max_players.unwrap_or(8),
            frame_pause: Duration::from_millis(frame_ms.unwrap_or(1).into()),
            save_interval: Duration::from_secs(save_interval_s.unwrap_or(60).into()),
            grace: Duration::from_secs(grace_s.unwrap_or(30).into()),
            logs: logs.unwrap_or_else(|| base.join("logs")),
            realm_addr,
            dev_test,
            hostile,
            update,
            folder: base.to_owned(),
        })
    }

    /// The binary's `NodeGame` defines its game; config may only confirm it.
    pub fn check_game(&self, game: &str) -> Result<(), String> {
        match &self.game {
            Some(expected) if expected != game => Err(format!(
                "config game = {expected}, but this game server runs {game}"
            )),
            _ => Ok(()),
        }
    }

    /// Refuses a bundle for another mod realm than the one `game` names.
    pub fn check_bundle(&self, bundle: &Bundle) -> Result<(), String> {
        match &self.game {
            Some(g) if *g != bundle.game => Err(format!(
                "this game server serves the mod realm {g}, but its bundle is for {}",
                bundle.game
            )),
            _ => Ok(()),
        }
    }

    /// The most players of all its games together.
    pub fn max_players_total(&self) -> u16 {
        self.max_games.saturating_mul(self.max_players.into())
    }
}

impl GamesSettings for NodeConfig {
    fn games_config(&self) -> GamesConfig {
        GamesConfig {
            max_games: self.max_games.into(),
            max_players: self.max_players,
            grace: self.grace,
            frame_pause: self.frame_pause,
            save_interval: self.save_interval,
            unused_close: GamesConfig::UNUSED_CLOSE,
            dev_test: self.dev_test,
            invent_stash_items: self.hostile.as_deref() == Some("invent-stash-item"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> Result<NodeConfig, ConfigError> {
        NodeConfig::parse(text, Path::new("node"))
    }

    #[test]
    fn bundle_and_game_data_are_required() {
        let c = parse("bundle = b.bundle\ngame_data = d2\n").unwrap();
        assert_eq!(c.bundle, Path::new("node").join("b.bundle"));
        assert_eq!(c.listen, DEFAULT_LISTEN.parse().unwrap());
        assert_eq!(c.identity, Path::new("node").join("identity"));
        assert_eq!(c.logs, Path::new("node").join("logs"));
        assert_eq!((c.max_games, c.max_players), (16, 8));
        assert_eq!(c.max_players_total(), 128);
        assert_eq!(c.realm_addr, None);
        assert!(matches!(
            parse("game_data = d2\n"),
            Err(ConfigError::Missing { key: "bundle", .. })
        ));
        assert_eq!(parse("bundle = b\n"), Err(ConfigError::NoGameData));
    }

    #[test]
    fn the_game_key_names_the_realm_and_refuses_another_bundle() {
        let base = "bundle = b\ngame_data = d2\n";
        let bundle = |game: &str| Bundle {
            id: "eu-1".into(),
            realm_addr: "127.0.0.1:6112".parse().unwrap(),
            realm_cert: [7; 32],
            game: game.into(),
            ticket_keys: Vec::new(),
        };
        let none = parse(base).unwrap();
        assert_eq!(none.game, None);
        assert_eq!(none.check_bundle(&bundle("mymod")), Ok(()));
        let mymod = parse(&format!("{base}game = mymod\n")).unwrap();
        assert_eq!(mymod.game.as_deref(), Some("mymod"));
        assert_eq!(mymod.check_bundle(&bundle("mymod")), Ok(()));
        let e = mymod.check_bundle(&bundle("d2rust")).unwrap_err();
        assert!(e.contains("mymod") && e.contains("d2rust"), "{e}");
        for bad in [
            "game = ",
            "game = MYMOD",
            "game = a/b",
            "game = mymod\ngame = mymod",
        ] {
            assert!(parse(&format!("{base}{bad}\n")).is_err(), "{bad}");
        }
    }

    #[test]
    fn every_key_is_read_into_the_games_settings() {
        let c = parse(
            "bundle = b\ngame_data = d2\nidentity = id\nlisten = 127.0.0.1:0\n\
             public_addr = 203.0.113.20:6113\nmax_games = 4\nmax_players = 2\n\
             frame_ms = 5\nsave_interval_s = 10\ngrace_s = 3\nlogs = l\n\
             realm_addr = 127.0.0.1:6112\ndev = test\n",
        )
        .unwrap();
        assert_eq!(c.realm_addr, Some("127.0.0.1:6112".parse().unwrap()));
        assert_eq!(c.public_addr, Some("203.0.113.20:6113".parse().unwrap()));
        let g = c.games_config();
        assert_eq!((g.max_games, g.max_players), (4, 2));
        assert_eq!(g.frame_pause, Duration::from_millis(5));
        assert_eq!(g.save_interval, Duration::from_secs(10));
        assert_eq!(g.grace, Duration::from_secs(3));
        assert!(g.dev_test);
    }

    #[test]
    fn character_and_account_keys_are_refused() {
        for key in REFUSED {
            let text = format!("bundle = b\ngame_data = d2\n{key} = x\n");
            match parse(&text) {
                Err(ConfigError::Value {
                    line: 3, reason, ..
                }) => {
                    assert!(reason.contains("no characters"), "{reason}");
                }
                other => panic!("{key}: {other:?}"),
            }
        }
        assert!(matches!(
            parse("bundle = b\ngame_data = d2\nnope = 1\n"),
            Err(ConfigError::UnknownKey { .. })
        ));
    }

    #[test]
    fn the_update_command_is_read() {
        let c = parse(
            "bundle = b
game_data = d2
",
        )
        .unwrap();
        assert_eq!(c.update, Fetcher::Off);
        let c = parse(
            "bundle = b
game_data = d2
update_command = server-ctl
",
        )
        .unwrap();
        assert_eq!(
            c.update,
            Fetcher::Run {
                words: vec!["server-ctl".into()],
                folder: PathBuf::from("node"),
            }
        );
        let ps = "update_command = powershell.exe -NoProfile -File node-ctl.ps1
";
        let c = parse(&format!(
            "bundle = b
game_data = d2
{ps}"
        ))
        .unwrap();
        assert!(matches!(c.update, Fetcher::Run { ref words, .. } if words.len() == 4));
        // The example a game server's setup copies fetches its upgrades through server-ctl.
        let c = parse(include_str!(
            "../../../tools/server/d2rust-node.cfg.example"
        ))
        .unwrap();
        assert_eq!(
            c.update,
            Fetcher::Run {
                words: vec!["server-ctl".into()],
                folder: PathBuf::from("node"),
            }
        );
        let c = parse(
            "bundle = b
game_data = d2
update_command = test
dev = test
",
        )
        .unwrap();
        assert_eq!(c.update, Fetcher::Test);
        assert!(matches!(
            parse(
                "bundle = b
game_data = d2
update_command = test
"
            ),
            Err(ConfigError::Value { line: 3, .. })
        ));
    }

    #[test]
    fn hostile_needs_a_test_build() {
        let r = parse("bundle = b\ngame_data = d2\nhostile = alter-save\n");
        if cfg!(feature = "hostile") {
            assert_eq!(r.unwrap().hostile.as_deref(), Some("alter-save"));
        } else {
            assert!(matches!(r, Err(ConfigError::Value { .. })), "{r:?}");
        }
        assert!(parse("bundle = b\ngame_data = d2\nhostile = everything\n").is_err());
    }
}
