//! The main server's settings (contracts 11.8): the dedicated server's
//! keys plus the game-server list's, read with the server programs' shared
//! reader ([`d2_service::config`]).
//!
//! The file has one `key = value` per line. Blank lines and lines that
//! start with `#` are skipped. Every key is optional except one of
//! `diablo2` and `game_data`.
//! A relative path is taken from the config file's folder, so a server
//! folder can be moved as a whole.
//!
//! ```text
//! # d2rust-realm.cfg
//! listen      = 0.0.0.0:6112        # players and game servers
//! nodes       = data/nodes.toml     # the official game servers
//! max_players = 8
//! diablo2     = ~/d2rust-server/diablo2  # your Diablo II (its MPQs): the game data is made from it
//! base_data   = ~/d2rust-server/base     # where it is made, shared by every server on the machine
//! game_data   = game                 # a mod's own layer (mods/), or an older converted copy
//! characters  = characters
//! logs        = logs
//! grace_s     = 30
//! frame_ms    = 1
//! max_games   = 16
//! save_interval_s = 60
//! server_name = D2RUST Europe
//! identity    = identity
//! max_sessions = 256
//! net_threads = 2
//! backups     = backups
//! backup_keep = 14
//! ```
//!
//! The game data: `diablo2` names the owner's Diablo II (an install or a
//! folder of its MPQs), from which the machine's shared base is made once
//! per engine format ([`d2_base_data`]); `game_data` then holds only a mod's
//! own layer, if any. Without `diablo2`, a `game_data` that is an install or
//! a converted folder of its own is read as before, else Diablo II is looked
//! for on the machine.
//!
//! `dev = test` is for test servers only (the MP test tool). With it, and
//! only with it, `bundle_dir = PATH` and `test_nodes = ID,ID` make the
//! main server add those official game servers itself at each start: it
//! makes their certificates and writes their bundles into `bundle_dir`
//! (contracts 11.10), so a test setup needs no `--add-node` step.
//!
//! The games' keys (`max_players`, `grace_s`, `frame_ms`,
//! `save_interval_s`) are read as before: the lobby's limits use
//! `max_players` and `max_games`, the rest go to the game servers with
//! their games (plan N4).
//!
//! `game = ID` is the mod realm this main server runs (default `d2rust`):
//! it names the realm's folder under `data/realms/`, the ladder's and the
//! economy's realm, and the realm every `--add-node` and game server must
//! be for. `mod_label = TEXT` is its name in the chat's Mod tab (default
//! `D2Gamma`). A second realm on the same box (a mod's) sets both, with
//! its own `listen`, `data`, `nodes` and `identity`.
//!
//! The accounts keys (`data`, `argon2`, `limits`, `max_characters`,
//! `registration`, `records`) are described at [`AccountsConfig`].
//! `records = game` is for a mod realm whose game saves its characters in
//! a format of its own: the realm stores them as the game server sends
//! them, takes their summaries from the game server and skips its `.d2s`
//! checks (`d2_accounts::RecordFormat`; default `d2s`). It runs no item
//! registry either (the registry reads `.d2s` records only, so it would
//! take every item for an invented one): `item_registry` is `off` there,
//! and `flag` or `refuse` is refused.
//!
//! `journals = PATH` is where the games' journals are kept
//! ([`crate::realm::journals`]; default `data/journals`).
//!
//! The save path's keys ([`crate::realm::guard`]): `save_checks = flag`
//! (the default: every check only flags), `standard` (the checks' own
//! modes) or `off`; `check.NAME = refuse|flag|off` for one check; and
//! `item_registry = flag` (the default), `refuse` or `off`.
//!
//! The market's and the online stash's keys ([`MarketConfig::KEYS`];
//! contract section 9): `market = off` (the default) or `on`, its limits
//! (`market_max_listings`, `market_max_offers`, `market_offer_items`,
//! `market_days`, `market_posts_per_min`, `market_searches_per_min`), its
//! fees (`market_fee_pct`, `market_listing_fee`; `market_buyer_fee` must
//! be 0), `stash_items`, `stash_gold_max` and `stackables = CODE,CODE`
//! (default: the game data's runes and gems).
//!
//! The online stash's pages ([`StashSettings`]): `stash_pages`,
//! `stash_cols`, `stash_rows`, `stash_renamable`, `stash_cap` and
//! `stash_category` (one line per category page). D2Gamma's realm sets
//! none of them.
//!
//! The ladder's keys ([`LadderConfig`]; lobby contract section 6):
//! `ladder = off` (the default: only the non-ladder season 0 is listed)
//! or `on`, and `ladder_refresh_s` (60: how often the rank tables are
//! computed again, at most; 0 at every query).
//!
//! The chat's keys ([`SocialConfig::KEYS`]; lobby contract sections 9 and
//! 11): `chat_lines` and `chat_window_s` (5 lines per 10 s, all rooms),
//! `global_lines` and `global_window_s` (Global: 2 per 30 s),
//! `chat_join_lines_max` (50), `dm_keep_days` (30), `dm_waiting_max` (200),
//! `guild_max_members` (100), `friends_max` (100), `ignore_max` (200).

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::Duration;

use d2_net::msg::node::{valid_game_id, valid_node_id};
use d2_service::config::{self, set_once};

use crate::accounts::AccountsConfig;
use crate::ladder::LadderConfig;
use crate::market::MarketConfig;
use crate::realm::guard::{ChecksMode, GuardConfig, RegistryMode};
use crate::realm::stash::StashSettings;
use crate::social::SocialConfig;

pub use d2_service::config::ConfigError;

/// The highest player limit a game may be configured with. Nothing in the
/// game has a fixed player count; this bound is where per-tick costs have
/// been measured (64 scripted clients).
pub const MAX_PLAYERS_LIMIT: usize = 64;

/// The default players of one game (as the dedicated server's).
pub const DEFAULT_MAX_PLAYERS: usize = 8;

/// The default listen address: every interface, port 6112.
pub const DEFAULT_LISTEN: &str = "0.0.0.0:6112";

/// The default reconnect grace period, in seconds.
pub const DEFAULT_GRACE_S: u32 = 30;

/// The default number of games at once.
pub const DEFAULT_MAX_GAMES: usize = 16;

/// The default time between two saves of a game's players, in seconds.
pub const DEFAULT_SAVE_INTERVAL_S: u32 = 60;

/// The default number of sessions at once.
pub const DEFAULT_MAX_SESSIONS: usize = 256;

/// The default number of network threads.
pub const DEFAULT_NET_THREADS: usize = 2;

/// The default number of dated backups kept.
pub const DEFAULT_BACKUP_KEEP: usize = 14;

/// The default mod realm.
pub const DEFAULT_GAME: &str = "d2rust";

/// The default name of the realm in the chat's Mod tab.
pub const DEFAULT_MOD_LABEL: &str = "D2Gamma";

/// The default server name.
pub const DEFAULT_SERVER_NAME: &str = "D2RUST";

/// The server's settings.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RealmConfig {
    /// The address and UDP port the QUIC endpoint listens on.
    pub listen: SocketAddr,
    /// The most players one game takes.
    pub max_players: usize,
    /// The game folder the server reads its game data from. Nothing
    /// from it is ever sent to clients.
    pub game_data: PathBuf,
    /// `diablo2`: the Diablo II the shared base is converted from.
    pub diablo2: Option<PathBuf>,
    /// `base_data`: the shared base's folder, when not the default.
    pub base_data: Option<PathBuf>,
    /// The character folder.
    pub characters: PathBuf,
    /// The log folder.
    pub logs: PathBuf,
    /// How long a lost player's slot is held for a reconnect.
    pub grace: Duration,
    /// The pause between two server frames.
    pub frame_pause: Duration,
    /// Accounts and online characters: where they are stored and their
    /// rules.
    pub accounts: AccountsConfig,
    /// The most games at once.
    pub max_games: usize,
    /// How often a game saves its players' characters.
    pub save_interval: Duration,
    /// The config file's folder: the server folder (`run/` is there).
    pub folder: PathBuf,
    /// The mod realm this main server runs.
    pub game: String,
    /// Optional mod-kit version; absent preserves legacy handshakes and listings.
    pub mod_version: Option<String>,
    /// Optional deployed mod.toml; binds identity, economy namespace and maker policy.
    pub mod_manifest: Option<PathBuf>,
    /// The realm's name in the chat's Mod tab.
    pub mod_label: String,
    /// The name clients see in the server's welcome.
    pub server_name: String,
    /// The folder of the server's certificate and key.
    pub identity: PathBuf,
    /// The most sessions at once.
    pub max_sessions: usize,
    /// The network side's threads.
    pub net_threads: usize,
    /// The folder of the dated backups.
    pub backups: PathBuf,
    /// How many dated backups are kept.
    pub backup_keep: usize,
    /// Test servers only: dev commands on standard input.
    pub dev_test: bool,
    /// The official game servers' list (`data/nodes.toml`).
    pub nodes: PathBuf,
    /// Test only: where the test game servers' bundles go.
    pub bundle_dir: Option<PathBuf>,
    /// Test only: the game servers added at each start.
    pub test_nodes: Vec<String>,
    /// The save checks and the item registry.
    pub guard: GuardConfig,
    /// Where the games' journals are kept.
    pub journals: PathBuf,
    /// The market and the online stash's limits.
    pub market: MarketConfig,
    /// The ladder's seasons and rank tables.
    pub ladder: LadderConfig,
    /// The chat's limits, the DMs' and the guilds' and friends' sizes.
    pub social: SocialConfig,
    /// The online stash's pages, slot cap and category pages (D2Gamma's
    /// by default).
    pub stash: StashSettings,
}

impl RealmConfig {
    /// Reads and parses the config file at `path`.
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        let mut cfg = Self::parse(&config::read(path)?, config::folder(path))?;
        if let Some(manifest_path) = cfg.mod_manifest.clone() {
            let text = config::read(&manifest_path)?;
            let manifest = d2_modkit::Manifest::parse(&text).map_err(|e| ConfigError::Read {
                path: manifest_path.clone(),
                reason: e.to_string(),
            })?;
            cfg.apply_mod(&manifest)
                .map_err(|reason| ConfigError::Read {
                    path: manifest_path.clone(),
                    reason,
                })?;
        }
        Ok(cfg)
    }

    /// Parses config text; relative paths are taken from `base`.
    pub fn parse(text: &str, base: &Path) -> Result<Self, ConfigError> {
        let mut listen = None;
        let mut max_players = None;
        let mut game_data = None;
        let mut diablo2 = None;
        let mut base_data = None;
        let mut characters = None;
        let mut logs = None;
        let mut grace_s = None;
        let mut frame_ms = None;
        let mut accounts = AccountsConfig::new(base);
        let mut accounts_keys: Vec<&str> = Vec::new();
        let mut market = MarketConfig::default();
        let mut market_keys: Vec<&str> = Vec::new();
        let mut social = SocialConfig::default();
        let mut social_keys: Vec<&str> = Vec::new();
        let mut stash = StashSettings::default();
        let mut stash_keys: Vec<&str> = Vec::new();
        let mut max_games = None;
        let mut save_interval_s = None;
        let mut server_name = None;
        let mut game = None;
        let mut mod_version = None;
        let mut mod_manifest = None;
        let mut mod_label = None;
        let mut identity = None;
        let mut max_sessions = None;
        let mut net_threads = None;
        let mut backups = None;
        let mut backup_keep = None;
        let mut dev_test = None;
        let mut nodes = None;
        let mut bundle_dir = None;
        let mut test_nodes: Option<Vec<String>> = None;
        let mut test_only = None;
        let mut save_checks = None;
        let mut item_registry = None;
        let mut item_registry_line = 0;
        let mut journals = None;
        let mut check_modes: Vec<(String, d2_save_checks::Mode)> = Vec::new();
        let mut ladder = None;
        let mut ladder_refresh_s = None;
        for entry in config::entries(text) {
            let e = entry?;
            match e.key {
                "listen" => set_once(&mut listen, e.parse::<SocketAddr>()?, &e)?,
                "max_players" => set_once(&mut max_players, e.number(1..=MAX_PLAYERS_LIMIT)?, &e)?,
                "game_data" => set_once(&mut game_data, e.path(base), &e)?,
                "diablo2" => set_once(&mut diablo2, e.home_path(base), &e)?,
                "base_data" => set_once(&mut base_data, e.home_path(base), &e)?,
                "characters" => set_once(&mut characters, e.path(base), &e)?,
                "logs" => set_once(&mut logs, e.path(base), &e)?,
                "grace_s" => set_once(&mut grace_s, e.parse::<u32>()?, &e)?,
                "frame_ms" => set_once(&mut frame_ms, e.number(0..=1000u32)?, &e)?,
                k if AccountsConfig::KEYS.contains(&k) => {
                    if accounts_keys.contains(&k) {
                        return Err(e.repeated());
                    }
                    accounts_keys.push(k);
                    accounts.set(k, e.value, base).map_err(|r| e.bad(r))?;
                }
                k if MarketConfig::KEYS.contains(&k) => {
                    if market_keys.contains(&k) {
                        return Err(e.repeated());
                    }
                    market_keys.push(k);
                    market.set(k, e.value).map_err(|r| e.bad(r))?;
                }
                k if StashSettings::KEYS.contains(&k) => {
                    if stash_keys.contains(&k) {
                        return Err(e.repeated());
                    }
                    stash_keys.push(k);
                    stash.set(k, e.value).map_err(|r| e.bad(r))?;
                }
                StashSettings::CATEGORY_KEY => {
                    stash.set(e.key, e.value).map_err(|r| e.bad(r))?;
                }
                k if SocialConfig::KEYS.contains(&k) => {
                    if social_keys.contains(&k) {
                        return Err(e.repeated());
                    }
                    social_keys.push(k);
                    social.set(k, e.value).map_err(|r| e.bad(r))?;
                }
                "max_games" => set_once(&mut max_games, e.number(1..=1024usize)?, &e)?,
                "server_name" => {
                    if e.value.len() > 64 || e.value.chars().any(char::is_control) {
                        return Err(e.bad("at most 64 printable bytes"));
                    }
                    set_once(&mut server_name, e.value.to_owned(), &e)?;
                }
                "game" => {
                    if !valid_game_id(e.value) {
                        return Err(e.bad("1-40 of a-z, 0-9 and -"));
                    }
                    set_once(&mut game, e.value.to_owned(), &e)?;
                }
                "mod_manifest" => set_once(&mut mod_manifest, e.path(base), &e)?,
                "mod_version" => {
                    let identity = d2_net::mod_identity::ModIdentity {
                        id: "test-mod".into(),
                        version: e.value.into(),
                    };
                    if !identity.valid() {
                        return Err(
                            e.bad("1-64 version characters: letters, digits, dots, + and -")
                        );
                    }
                    set_once(&mut mod_version, e.value.to_owned(), &e)?;
                }
                "mod_label" => {
                    if e.value.is_empty()
                        || e.value.len() > 32
                        || e.value.chars().any(char::is_control)
                    {
                        return Err(e.bad("1-32 printable bytes"));
                    }
                    set_once(&mut mod_label, e.value.to_owned(), &e)?;
                }
                "identity" => set_once(&mut identity, e.path(base), &e)?,
                "backups" => set_once(&mut backups, e.path(base), &e)?,
                "journals" => set_once(&mut journals, e.path(base), &e)?,
                "max_sessions" => set_once(&mut max_sessions, e.number(1..=4096usize)?, &e)?,
                "net_threads" => set_once(&mut net_threads, e.number(1..=16usize)?, &e)?,
                "backup_keep" => set_once(&mut backup_keep, e.number(1..=365usize)?, &e)?,
                "dev" => {
                    let v = match e.value {
                        "test" => true,
                        "off" => false,
                        _ => return Err(e.bad("`test` or `off`")),
                    };
                    set_once(&mut dev_test, v, &e)?;
                }
                "save_interval_s" => {
                    set_once(&mut save_interval_s, e.number(1..=3600u32)?, &e)?;
                }
                "nodes" => set_once(&mut nodes, e.path(base), &e)?,
                "ladder" => {
                    let v = match e.value {
                        "on" => true,
                        "off" => false,
                        _ => return Err(e.bad("`on` or `off`")),
                    };
                    set_once(&mut ladder, v, &e)?;
                }
                "ladder_refresh_s" => {
                    set_once(&mut ladder_refresh_s, e.number(0..=3600u32)?, &e)?;
                }
                "bundle_dir" => {
                    set_once(&mut bundle_dir, e.path(base), &e)?;
                    test_only.get_or_insert(e.line);
                }
                "test_nodes" => {
                    let ids: Vec<String> =
                        e.value.split(',').map(|s| s.trim().to_owned()).collect();
                    if !ids.iter().all(|id| valid_node_id(id)) {
                        return Err(e.bad("node ids of 1-15 of a-z, 0-9 and -, comma separated"));
                    }
                    set_once(&mut test_nodes, ids, &e)?;
                    test_only.get_or_insert(e.line);
                }
                "save_checks" => {
                    let m = ChecksMode::parse(e.value)
                        .ok_or_else(|| e.bad("`flag`, `standard` or `off`"))?;
                    set_once(&mut save_checks, m, &e)?;
                }
                "item_registry" => {
                    let m = RegistryMode::parse(e.value)
                        .ok_or_else(|| e.bad("`flag`, `refuse` or `off`"))?;
                    set_once(&mut item_registry, m, &e)?;
                    item_registry_line = e.line;
                }
                k if k.starts_with("check.") => {
                    let name = &k["check.".len()..];
                    if !d2_save_checks::STANDARD.iter().any(|(n, _)| *n == name) {
                        return Err(e.bad(format!("no save check named `{name}`")));
                    }
                    if check_modes.iter().any(|(n, _)| n == name) {
                        return Err(e.repeated());
                    }
                    let m = d2_save_checks::Mode::parse(e.value)
                        .ok_or_else(|| e.bad("`refuse`, `flag` or `off`"))?;
                    check_modes.push((name.to_owned(), m));
                }
                _ => return Err(e.unknown()),
            }
        }
        let dev_test = dev_test.unwrap_or(false);
        if let Some(line) = test_only {
            let bad = |reason: &str| ConfigError::Value {
                line,
                key: if bundle_dir.is_some() {
                    "bundle_dir"
                } else {
                    "test_nodes"
                }
                .into(),
                reason: reason.into(),
            };
            if !dev_test {
                return Err(bad("test setups only: needs `dev = test`"));
            }
            if bundle_dir.is_none() || test_nodes.is_none() {
                return Err(bad("`bundle_dir` and `test_nodes` go together"));
            }
        }
        let registry = match (accounts.records.is_d2s(), item_registry) {
            (true, m) => m.unwrap_or_default(),
            (false, None | Some(RegistryMode::Off)) => RegistryMode::Off,
            (false, Some(_)) => {
                return Err(ConfigError::Value {
                    line: item_registry_line,
                    key: "item_registry".into(),
                    reason: "a realm of the game's own records (`records = game`) runs no \
                             item registry: `off`"
                        .into(),
                })
            }
        };
        let data = accounts.data.clone();
        Ok(RealmConfig {
            listen: listen.unwrap_or_else(|| DEFAULT_LISTEN.parse().expect("a valid address")),
            max_players: max_players.unwrap_or(DEFAULT_MAX_PLAYERS),
            game_data: match (game_data, &diablo2) {
                (Some(g), _) => g,
                (None, Some(_)) => base.join("game"),
                (None, None) => return Err(ConfigError::NoGameData),
            },
            diablo2,
            base_data,
            characters: characters.unwrap_or_else(|| base.join("characters")),
            logs: logs.unwrap_or_else(|| base.join("logs")),
            grace: Duration::from_secs(grace_s.unwrap_or(DEFAULT_GRACE_S).into()),
            frame_pause: Duration::from_millis(frame_ms.unwrap_or(1).into()),
            accounts,
            max_games: max_games.unwrap_or(DEFAULT_MAX_GAMES),
            save_interval: Duration::from_secs(
                save_interval_s.unwrap_or(DEFAULT_SAVE_INTERVAL_S).into(),
            ),
            folder: base.to_path_buf(),
            game: game.unwrap_or_else(|| DEFAULT_GAME.to_owned()),
            mod_version,
            mod_manifest,
            mod_label: mod_label.unwrap_or_else(|| DEFAULT_MOD_LABEL.to_owned()),
            server_name: server_name.unwrap_or_else(|| DEFAULT_SERVER_NAME.to_owned()),
            identity: identity.unwrap_or_else(|| base.join("identity")),
            max_sessions: max_sessions.unwrap_or(DEFAULT_MAX_SESSIONS),
            net_threads: net_threads.unwrap_or(DEFAULT_NET_THREADS),
            backups: backups.unwrap_or_else(|| base.join("backups")),
            backup_keep: backup_keep.unwrap_or(DEFAULT_BACKUP_KEEP),
            dev_test,
            nodes: nodes.unwrap_or_else(|| data.join("nodes.toml")),
            bundle_dir,
            test_nodes: test_nodes.unwrap_or_default(),
            guard: GuardConfig {
                checks: save_checks.unwrap_or_default(),
                check_modes,
                registry,
            },
            journals: journals.unwrap_or_else(|| data.join("journals")),
            market,
            ladder: LadderConfig {
                on: ladder.unwrap_or(false),
                refresh: Duration::from_secs(
                    ladder_refresh_s
                        .unwrap_or(crate::ladder::DEFAULT_REFRESH_S)
                        .into(),
                ),
            },
            social,
            stash,
        })
    }
}

impl RealmConfig {
    /// Binds every existing per-realm service to the full mod id, never its version.
    pub fn apply_mod(&mut self, manifest: &d2_modkit::Manifest) -> Result<(), String> {
        if manifest.kind != d2_modkit::Kind::Full {
            return Err("mod_manifest must name a full mod".into());
        }
        match d2_modkit::api::check_api_now(manifest.api) {
            d2_modkit::ApiCheck::Ok => {}
            d2_modkit::ApiCheck::NeedsUpdate(message) | d2_modkit::ApiCheck::TooNew(message) => {
                return Err(message)
            }
        }
        if self.game != DEFAULT_GAME && self.game != manifest.id {
            return Err(format!(
                "realm game {} differs from mod {}",
                self.game, manifest.id
            ));
        }
        let version = manifest.version.to_string();
        if self.mod_version.as_ref().is_some_and(|v| v != &version) {
            return Err("mod_version differs from mod_manifest".into());
        }
        self.game = manifest.id.clone();
        self.mod_version = Some(version);
        self.mod_label = manifest.name.clone();
        self.market.economy.market &= manifest.market;
        self.ladder.on &= manifest.ladder;
        Ok(())
    }

    pub fn mod_identity(&self) -> Option<d2_net::mod_identity::ModIdentity> {
        self.mod_version
            .as_ref()
            .map(|version| d2_net::mod_identity::ModIdentity {
                id: self.game.clone(),
                version: version.clone(),
            })
    }
    /// Checks that a game server (`--add-node --realm`) is for this
    /// main server's mod realm.
    pub fn check_game(&self, realm: &str) -> Result<(), String> {
        if realm == self.game {
            Ok(())
        } else {
            Err(format!(
                "--realm {realm}: this main server runs the mod realm {}",
                self.game
            ))
        }
    }
}

impl AsRef<AccountsConfig> for RealmConfig {
    fn as_ref(&self) -> &AccountsConfig {
        &self.accounts
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> Result<RealmConfig, ConfigError> {
        RealmConfig::parse(&format!("game_data = game\n{text}"), Path::new("/srv"))
    }

    fn kit_manifest() -> d2_modkit::Manifest {
        d2_modkit::Manifest::parse("id='ghost-world'\nname='Ghost World'\nversion='1.0.0'\nauthor='Nugwa'\ndescription='Test'\napi=1\nkind='full'").unwrap()
    }

    #[test]
    fn kit_mod_stash_and_market_reject_cross_namespace_requests_for_same_account() {
        use d2_economy::{
            AccountId, Economy, EconomyConfig, EconomyKey, Goods, MarketQuery, Owner, Req,
        };
        let config = EconomyConfig {
            market: true,
            ..EconomyConfig::default()
        };
        let mut ghost = Economy::in_memory(&"g".repeat(40), config.clone()).unwrap();
        let ember = Economy::in_memory("ember-world", config).unwrap();
        let a = Owner::new(7, EconomyKey::new("g".repeat(40)));
        let b = Owner::new(7, EconomyKey::new("ember-world"));
        ghost
            .seed(&a, &Req::account(AccountId(7), 1), &Goods::Gold(123), 0)
            .unwrap();
        assert_eq!(ghost.stash_gold(&a).unwrap(), 123);
        assert_eq!(ember.stash_gold(&b).unwrap(), 0);
        assert!(ghost.stash_gold(&b).is_err());
        assert!(ember.stash_gold(&a).is_err());
        assert!(ghost
            .search(&b.economy, Some(a.account), &MarketQuery::default(), 0)
            .is_err());
        assert!(ember
            .search(&a.economy, Some(b.account), &MarketQuery::default(), 0)
            .is_err());
    }

    #[test]
    fn mod_policy_uses_stable_id_namespace_and_can_only_disable_services() {
        let mut cfg = parse("market = on\nladder = on").unwrap();
        let mut manifest = kit_manifest();
        manifest.market = false;
        manifest.ladder = false;
        cfg.apply_mod(&manifest).unwrap();
        assert_eq!(cfg.game, "ghost-world");
        assert_eq!(cfg.mod_identity().unwrap().version, "1.0.0");
        assert!(!cfg.market.economy.market && !cfg.ladder.on);
        let path = crate::realm::guard::realm_db(&cfg.accounts.data, &cfg.game);
        manifest.version = d2_modkit::Version::parse("2.0.0").unwrap();
        let mut upgraded = parse("").unwrap();
        upgraded.apply_mod(&manifest).unwrap();
        assert_eq!(
            path,
            crate::realm::guard::realm_db(&upgraded.accounts.data, &upgraded.game)
        );
        assert_ne!(
            path,
            crate::realm::guard::realm_db(&cfg.accounts.data, DEFAULT_GAME)
        );
        let mut disabled_by_owner = parse("").unwrap();
        manifest.market = true;
        manifest.ladder = true;
        disabled_by_owner.apply_mod(&manifest).unwrap();
        assert!(!disabled_by_owner.market.economy.market && !disabled_by_owner.ladder.on);
        assert!(parse("game = another-mod")
            .unwrap()
            .apply_mod(&manifest)
            .is_err());
        assert!(parse("mod_version = 9.0.0")
            .unwrap()
            .apply_mod(&manifest)
            .is_err());
    }

    #[test]
    fn deployed_manifest_load_enforces_policy_and_reports_bad_files() {
        let dir = std::env::temp_dir().join(format!("realm-kit-policy-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("mod.toml"), kit_manifest().to_toml()).unwrap();
        let path = dir.join("realm.cfg");
        std::fs::write(&path, "game_data = game\nmod_manifest = mod.toml").unwrap();
        let cfg = RealmConfig::load(&path).unwrap();
        assert_eq!(cfg.game, "ghost-world");
        assert_eq!(cfg.mod_version.as_deref(), Some("1.0.0"));
        std::fs::write(dir.join("mod.toml"), "broken").unwrap();
        assert!(RealmConfig::load(&path).is_err());
        std::fs::remove_file(dir.join("mod.toml")).unwrap();
        assert!(RealmConfig::load(&path).is_err());
    }

    #[test]
    fn mod_version_is_optional_and_preserves_the_plain_realm() {
        assert_eq!(parse("").unwrap().mod_identity(), None);
        let c = parse("game = ghost\nmod_version = 1.2.3\n").unwrap();
        assert_eq!(
            c.mod_identity().unwrap(),
            d2_net::mod_identity::ModIdentity {
                id: "ghost".into(),
                version: "1.2.3".into()
            }
        );
        assert!(parse("mod_version = bad version\n").is_err());
        assert!(parse("mod_version = 1\nmod_version = 2\n").is_err());
    }

    #[test]
    fn the_save_path_keys_default_to_flag() {
        let c = parse("").unwrap();
        assert_eq!(c.guard, GuardConfig::default());
        assert_eq!(c.guard.checks, ChecksMode::Flag);
        assert_eq!(c.guard.registry, RegistryMode::Flag);
    }

    /// A realm of the game's own records runs no item registry: it can't
    /// read the records the registry tracks items in.
    #[test]
    fn a_game_records_realm_runs_no_item_registry() {
        let c = parse("records = game\n").unwrap();
        assert_eq!(c.guard.registry, RegistryMode::Off);
        let c = parse("records = game\nitem_registry = off\n").unwrap();
        assert_eq!(c.guard.registry, RegistryMode::Off);
        for mode in ["flag", "refuse"] {
            let e = parse(&format!("records = game\nitem_registry = {mode}\n")).unwrap_err();
            assert!(
                matches!(&e, ConfigError::Value { line: 3, key, .. } if key == "item_registry"),
                "{e:?}"
            );
        }
        assert_eq!(
            parse("records = d2s\n").unwrap().guard.registry,
            RegistryMode::Flag
        );
    }

    #[test]
    fn the_save_path_keys_read() {
        let c = parse(
            "save_checks = standard\nitem_registry = refuse\ncheck.item-ranges = refuse\ncheck.gold = off\n",
        )
        .unwrap();
        assert_eq!(c.guard.checks, ChecksMode::Standard);
        assert_eq!(c.guard.registry, RegistryMode::Refuse);
        assert_eq!(
            c.guard.check_modes,
            [
                ("item-ranges".to_owned(), d2_save_checks::Mode::Refuse),
                ("gold".to_owned(), d2_save_checks::Mode::Off)
            ]
        );
        for (bad, line) in [
            ("save_checks = maybe", 2),
            ("item_registry = on", 2),
            ("check.nope = flag", 2),
            ("check.gold = loud", 2),
            ("check.gold = off\ncheck.gold = flag", 3),
        ] {
            let e = parse(bad).unwrap_err().to_string();
            assert!(e.contains(&format!("line {line}")), "{bad}: {e}");
        }
    }

    #[test]
    fn diablo2_or_game_data_names_the_game_data() {
        let c =
            RealmConfig::parse("diablo2 = ~/d2rust-server/diablo2\n", Path::new("/srv")).unwrap();
        assert_eq!(c.game_data, Path::new("/srv/game"));
        assert!(c.diablo2.is_some());
        assert_eq!(c.base_data, None);
        let c =
            RealmConfig::parse("game_data = mods\nbase_data = /b\n", Path::new("/srv")).unwrap();
        assert_eq!(c.game_data, Path::new("/srv/mods"));
        assert_eq!(
            (c.diablo2, c.base_data.as_deref()),
            (None, Some(Path::new("/b")))
        );
        assert_eq!(
            RealmConfig::parse("listen = 0.0.0.0:6112\n", Path::new("/srv")).unwrap_err(),
            ConfigError::NoGameData
        );
    }

    #[test]
    fn the_game_keys_default_to_the_engines_realm() {
        let c = parse("").unwrap();
        assert_eq!(c.game, "d2rust");
        assert_eq!(c.mod_label, "D2Gamma");
    }

    #[test]
    fn the_game_keys_read() {
        let c = parse("game = mymod\nmod_label = My Mod\n").unwrap();
        assert_eq!(c.game, "mymod");
        assert_eq!(c.mod_label, "My Mod");
        for len in [16, 40] {
            let id = "x".repeat(len);
            assert_eq!(parse(&format!("game = {id}")).unwrap().game, id);
        }
        for (bad, line) in [
            ("game = ", 2),
            ("game = MYMOD", 2),
            ("game = a/b", 2),
            ("game = ../x", 2),
            ("game = xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx", 2),
            ("game = mymod\ngame = d2rust", 3),
            ("mod_label = \u{7}", 2),
            ("mod_label = mymod\nmod_label = x", 3),
        ] {
            let e = parse(bad).unwrap_err().to_string();
            assert!(e.contains(&format!("line {line}")), "{bad}: {e}");
        }
        let long = format!("mod_label = {}", "x".repeat(33));
        assert!(parse(&long).is_err());
        let node = format!("test_nodes = {}", "x".repeat(16));
        let error = parse(&node).unwrap_err().to_string();
        assert!(error.contains("node ids of 1-15"), "{error}");
    }

    #[test]
    fn a_game_server_for_another_realm_is_refused() {
        let d2 = parse("").unwrap();
        assert_eq!(d2.check_game("d2rust"), Ok(()));
        let e = d2.check_game("mymod").unwrap_err();
        assert!(e.contains("mymod") && e.contains("d2rust"), "{e}");
        assert!(d2.check_game("").is_err());
        let mymod = parse("game = mymod\n").unwrap();
        assert_eq!(mymod.check_game("mymod"), Ok(()));
        let e = mymod.check_game("d2rust").unwrap_err();
        assert!(
            e.contains("d2rust") && e.contains("runs the mod realm mymod"),
            "{e}"
        );
    }

    #[test]
    fn the_ladder_keys_read() {
        let c = parse("").unwrap();
        assert!(!c.ladder.on, "off by default");
        assert_eq!(c.ladder.refresh, Duration::from_secs(60));
        let c = parse(
            "ladder = on
ladder_refresh_s = 0
",
        )
        .unwrap();
        assert!(c.ladder.on);
        assert_eq!(c.ladder.refresh, Duration::ZERO);
        for (bad, line) in [
            ("ladder = yes", 2),
            ("ladder_refresh_s = 3601", 2),
            (
                "ladder = on
ladder = off",
                3,
            ),
        ] {
            let e = parse(bad).unwrap_err().to_string();
            assert!(e.contains(&format!("line {line}")), "{bad}: {e}");
        }
    }

    #[test]
    fn the_market_keys_read() {
        assert!(!parse("").unwrap().market.economy.market, "off by default");
        let c = parse("market = on\nmarket_fee_pct = 0\nmarket_days = 3\nstackables = r01,r02\n")
            .unwrap();
        assert!(c.market.economy.market);
        assert_eq!(c.market.economy.market_days, 3);
        assert_eq!(c.market.stackables, ["r01", "r02"]);
        for (bad, line) in [
            ("market = maybe", 2),
            ("market_buyer_fee = 5", 2),
            ("market = on\nmarket = off", 3),
        ] {
            let e = parse(bad).unwrap_err().to_string();
            assert!(e.contains(&format!("line {line}")), "{bad}: {e}");
        }
    }

    #[test]
    fn the_social_keys_read() {
        assert_eq!(parse("").unwrap().social, SocialConfig::default());
        let c = parse(
            "chat_lines = 7
global_window_s = 60
dm_keep_days = 7
friends_max = 20
",
        )
        .unwrap();
        assert_eq!(c.social.chat_lines, 7);
        assert_eq!(c.social.global_window_s, 60);
        assert_eq!(c.social.dm_keep_days, 7);
        assert_eq!(c.social.friends_max, 20);
        for (bad, line) in [
            ("chat_lines = 0", 2),
            ("dm_keep_days = x", 2),
            (
                "chat_lines = 5
chat_lines = 6",
                3,
            ),
            ("chat_colour = red", 2),
        ] {
            let e = parse(bad).unwrap_err().to_string();
            assert!(e.contains(&format!("line {line}")), "{bad}: {e}");
        }
    }

    #[test]
    fn the_stash_keys_default_to_d2gamma() {
        let c = parse("").unwrap();
        assert_eq!(c.stash, StashSettings::default());
        let c = parse(
            "game = mymod
mod_label = My Mod
stash_cols = 14
stash_rows = 14
stash_renamable = on
stash_cap = 9999999
stash_category = Currency;14;140
stash_category = Gems;14;0
",
        )
        .unwrap();
        assert_eq!((c.game.as_str(), c.mod_label.as_str()), ("mymod", "My Mod"));
        assert_eq!((c.stash.shape.cols, c.stash.shape.rows), (14, 14));
        assert!(c.stash.shape.renamable);
        assert_eq!(c.stash.cap, 9_999_999);
        assert_eq!(c.stash.categories.len(), 2);
        assert!(parse(
            "stash_cols = 14
stash_cols = 12
"
        )
        .is_err());
        assert!(parse(
            "stash_cap = lots
"
        )
        .is_err());
    }
}
