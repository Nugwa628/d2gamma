//! `d2rust-realm`: the main server (multiplayer v2, contracts 11.1).
//!
//! ```text
//! d2rust-realm [--config FILE]                 run the main server
//! d2rust-realm [--config FILE] --check         read the config, open the folders and the log, stop
//! d2rust-realm --version                       print `d2rust-realm <game version>`
//! d2rust-realm [--config FILE] --new-identity  make the certificate (once; kept across versions)
//! d2rust-realm [--config FILE] --fingerprint   print the certificate's SHA-256 for the manifest
//! d2rust-realm [--config FILE] --add-node ID --name NAME --location TEXT --realm MOD
//!              [--max-games N] [--cert-sha256 HEX] [--realm-addr ADDR] [--public-addr ADDR] [--out DIR]
//!                                              add an official game server: its entry in the
//!                                              game-server list and its bundle (in DIR, default
//!                                              the config's folder)
//! d2rust-realm [--config FILE] --node-name ID NAME
//!                                              the name players see game server ID by (the
//!                                              picker, the game list), e.g. "Finland"; the
//!                                              main server stopped
//! d2rust-realm [--config FILE] --list-nodes    the game-server list, each with that name
//! d2rust-realm [--config FILE] --backup [DIR]  back up accounts and characters (default: a dated
//!                                              folder in `backups`), the main server stopped
//! d2rust-realm [--config FILE] --set-password ACCOUNT     (the new password on standard input)
//! d2rust-realm [--config FILE] --set-progression CHARACTER N
//! d2rust-realm [--config FILE] --disable ACCOUNT | --enable ACCOUNT | --reindex
//! d2rust-realm [--config FILE] --season-start NAME [test] | --season-end
//!                                              open a ladder season (ending the open one), or
//!                                              end it (`ladder = on`; its final ranks stay, its
//!                                              characters become non-ladder when next loaded)
//! ```
//!
//! The config file defaults to `d2rust-realm.cfg` in the current folder.
//! Running, the main server hashes its game data, opens the accounts,
//! takes `data/server.lock`, listens on its UDP port (players and game
//! servers) and prints one line `ready program=realm listen=… cert_sha256=…
//! game=… build=… protocol=… tick=… caps=…` (contracts 11.10). It writes
//! `logs/status.json` every 10 s and answers backup and upgrade requests
//! from `server-ctl` (`run/upgrade.request`: the countdown,
//! [`d2rust_realm::upgrade`]). SIGTERM or Ctrl+C: it stops taking connections and exits
//! 0. The admin commands change files the running main server holds, so
//! they are refused while it runs.
//!
//! It runs no games: its game servers (`d2rust-node`) do. With
//! `dev = test` (test setups only) it reads `set-progression CHARACTER N`
//! lines (and the market's, the ladder's and the upgrade's test lines) on
//! standard input
//! and answers `dev ok TEXT` or `dev err TEXT` (contracts 10.1,
//! [`d2rust_realm::console`]); there `D2RUST_TEST_VERSION` names the
//! version it says it runs to its game servers (the upgrade's test plays
//! two versions with one build).

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use d2_link::Versions;
use d2_net::wire::{ServerInfo, Timing};
use d2_quic::{fingerprint, parse_fingerprint, Listen, QuicServer};
use d2_service::datahash::game_data_hash;
use d2_service::identity;
use d2_service::ready::ReadyLine;
use d2_service::status::{ServerLock, Status, STATUS_INTERVAL};
use d2_service::stop::wait_for_stop;
use d2rust_realm::accounts::backup::{answer_backup_request, backup_now};
use d2rust_realm::accounts::Accounts;
use d2rust_realm::config::RealmConfig;
use d2rust_realm::console::Line;
use d2rust_realm::ladder::Ladder;
use d2rust_realm::lobby::{self, Lobby};
use d2rust_realm::log::ServerLog;
use d2rust_realm::market::facts::ItemReader;
use d2rust_realm::market::{Market, SWEEP_S};
use d2rust_realm::net::{Net, NetConfig};
use d2rust_realm::realm::bundle::{add_node, AddNode};
use d2rust_realm::realm::guard::{self, SaveGuard};
use d2rust_realm::realm::journals::Journals;
use d2rust_realm::realm::nodes::{self, NodeRegistry};
use d2rust_realm::realm::saves::{LeaseTable, SaveDesk, SeatLoaded};
use d2rust_realm::realm::stash::Stash;
use d2rust_realm::realm::tickets::RealmTickets;
use d2rust_realm::realm::Realm;
use d2rust_realm::session::{LobbyApi, Sessions};
use d2rust_realm::social::{Social, SocialService};
use d2rust_realm::upgrade::{self, Outlet};

/// Seconds between the chat's sweeps (old DMs, mutes, invites).
const SOCIAL_SWEEP_S: u64 = 3600;

/// How often `logs/nodes.json` is written (`server-ctl nodes`): a game
/// server's heartbeat.
const NODES_FILE_S: u64 = 2;

/// The game version, the same four parts as the game's (`build.rs`).
const VERSION: &str = env!("D2RUST_VERSION");

/// The version this main server says it runs to its game servers, in
/// `status.json` and in its upgrades: the build's; on a test setup
/// `D2RUST_TEST_VERSION` when set (the players' handshake keeps the
/// build's).
fn link_version(dev_test: bool) -> String {
    std::env::var("D2RUST_TEST_VERSION")
        .ok()
        .filter(|v| dev_test && d2_net::msg::node::valid_update_version(v))
        .unwrap_or_else(|| VERSION.into())
}

/// Where the upgrade's lines go: Global, and every linked game server.
struct UpgradeOutlet {
    realm: Arc<Realm>,
    social: Arc<Social>,
}

impl Outlet for UpgradeOutlet {
    fn global(&self, text: &str) {
        if let Err(e) = self.social.notice(text) {
            self.social.log_line(&format!("upgrade notice: {e}"));
        }
    }

    fn nodes(&self, m: &d2_net::msg::node::RealmToNode) {
        self.realm.send_all(m);
    }

    fn linked(&self) -> Vec<String> {
        self.realm
            .nodes
            .linked_now()
            .into_iter()
            .map(|l| l.entry.id)
            .collect()
    }
}

/// The log file in the log folder.
const LOG_FILE: &str = "d2rust-realm.log";

const USAGE: &str = "usage: d2rust-realm [--config FILE] [--check | --new-identity | --fingerprint | --add-node ID --name NAME --location TEXT --realm MOD [--max-games N] [--cert-sha256 HEX] [--realm-addr ADDR] [--public-addr ADDR] [--out DIR] | --node-name ID NAME | --list-nodes | --backup [DIR] | --set-password ACCOUNT | --set-progression CHARACTER N | --disable ACCOUNT | --enable ACCOUNT | --reindex | --season-start NAME [test] | --season-end] | --version";

enum Command {
    Run,
    Check,
    NewIdentity,
    Fingerprint,
    AddNode(Box<AddNodeArgs>),
    Backup(Option<PathBuf>),
    SetPassword(String),
    SetProgression(String, u8),
    Disable(String, bool),
    Reindex,
    SeasonStart(String, bool),
    SeasonEnd,
    NodeName(String, String),
    ListNodes,
}

/// `--add-node`'s arguments.
#[derive(Default)]
struct AddNodeArgs {
    id: String,
    name: Option<String>,
    location: Option<String>,
    realm: Option<String>,
    max_games: Option<u16>,
    cert_sha256: Option<[u8; 32]>,
    realm_addr: Option<SocketAddr>,
    public_addr: Option<SocketAddr>,
    out: Option<PathBuf>,
}

fn main() -> ExitCode {
    let mut config = PathBuf::from("d2rust-realm.cfg");
    let mut command = None;
    let mut add: Option<AddNodeArgs> = None;
    let mut args = std::env::args().skip(1).peekable();
    while let Some(a) = args.next() {
        let next = match a.as_str() {
            "--version" => {
                println!("d2rust-realm {VERSION}");
                return ExitCode::SUCCESS;
            }
            "--config" => match args.next() {
                Some(p) => {
                    config = PathBuf::from(p);
                    continue;
                }
                None => return usage(),
            },
            "--check" => Command::Check,
            "--new-identity" => Command::NewIdentity,
            "--fingerprint" => Command::Fingerprint,
            "--reindex" => Command::Reindex,
            "--season-end" => Command::SeasonEnd,
            "--list-nodes" => Command::ListNodes,
            "--node-name" => {
                let (Some(id), Some(name)) = (args.next(), args.next()) else {
                    return usage();
                };
                Command::NodeName(id, name)
            }
            "--season-start" => {
                let Some(name) = args.next() else {
                    return usage();
                };
                let test = args.next_if(|n| n == "test").is_some();
                Command::SeasonStart(name, test)
            }
            "--backup" => {
                Command::Backup(args.next_if(|n| !n.starts_with("--")).map(PathBuf::from))
            }
            "--set-password" | "--disable" | "--enable" => {
                let Some(name) = args.next() else {
                    return usage();
                };
                match a.as_str() {
                    "--set-password" => Command::SetPassword(name),
                    "--disable" => Command::Disable(name, true),
                    _ => Command::Disable(name, false),
                }
            }
            "--set-progression" => {
                let (Some(name), Some(level)) = (args.next(), args.next()) else {
                    return usage();
                };
                match level.parse::<u8>() {
                    Ok(n) => Command::SetProgression(name, n),
                    Err(_) => return usage(),
                }
            }
            "--add-node" => match args.next() {
                Some(id) => {
                    add = Some(AddNodeArgs {
                        id,
                        ..AddNodeArgs::default()
                    });
                    continue;
                }
                None => return usage(),
            },
            flag @ ("--name" | "--location" | "--realm" | "--max-games" | "--cert-sha256"
            | "--realm-addr" | "--public-addr" | "--out") => {
                let (Some(n), Some(value)) = (add.as_mut(), args.next()) else {
                    return usage();
                };
                let ok = match flag {
                    "--name" => n.name.replace(value).is_none(),
                    "--location" => n.location.replace(value).is_none(),
                    "--realm" => n.realm.replace(value).is_none(),
                    "--out" => n.out.replace(PathBuf::from(value)).is_none(),
                    "--max-games" => match value.parse() {
                        Ok(v) if v > 0 => n.max_games.replace(v).is_none(),
                        _ => false,
                    },
                    "--cert-sha256" => match parse_fingerprint(&value) {
                        Some(fp) => n.cert_sha256.replace(fp).is_none(),
                        None => false,
                    },
                    "--realm-addr" => match value.parse() {
                        Ok(v) => n.realm_addr.replace(v).is_none(),
                        Err(_) => false,
                    },
                    _ => match value.parse() {
                        Ok(v) => n.public_addr.replace(v).is_none(),
                        Err(_) => false,
                    },
                };
                if !ok {
                    return usage();
                }
                continue;
            }
            _ => return usage(),
        };
        if command.replace(next).is_some() {
            return usage();
        }
    }
    if let Some(a) = add {
        if command.is_some() || a.name.is_none() || a.location.is_none() || a.realm.is_none() {
            return usage();
        }
        command = Some(Command::AddNode(Box::new(a)));
    }
    match run(&config, command.unwrap_or(Command::Run)) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("d2rust-realm: {e}");
            ExitCode::FAILURE
        }
    }
}

fn usage() -> ExitCode {
    eprintln!("{USAGE}");
    ExitCode::from(2)
}

fn run(path: &Path, command: Command) -> Result<ExitCode, String> {
    let cfg = RealmConfig::load(path).map_err(|e| e.to_string())?;
    match command {
        Command::Run => serve(path, &cfg),
        Command::Check => check(path, &cfg),
        Command::NewIdentity => {
            let (id, made) = identity::load_or_make(&cfg.identity)?;
            println!(
                "{} {}",
                if made { "made" } else { "kept" },
                cfg.identity.display()
            );
            println!("cert_sha256 = {}", identity::fingerprint_of(&id));
            Ok(ExitCode::SUCCESS)
        }
        Command::Fingerprint => {
            let id = identity::load(&cfg.identity)?;
            println!("cert_sha256 = {}", identity::fingerprint_of(&id));
            Ok(ExitCode::SUCCESS)
        }
        Command::AddNode(a) => {
            let a = *a;
            let realm = a.realm.unwrap_or_default();
            cfg.check_game(&realm)?;
            let realm_addr = match a.realm_addr {
                Some(addr) => addr,
                None if !cfg.listen.ip().is_unspecified() => cfg.listen,
                None => {
                    return Err(format!(
                        "listen = {} is every address: give --realm-addr, the address the game server reaches",
                        cfg.listen
                    ));
                }
            };
            let (realm_id, _) = identity::load_or_make(&cfg.identity)?;
            let (tickets, _) = RealmTickets::load_or_make(&cfg.identity)?;
            let max_games = a.max_games.unwrap_or(16);
            let n = AddNode {
                id: a.id,
                name: a.name.unwrap_or_default(),
                location: a.location.unwrap_or_default(),
                game: realm,
                max_games,
                max_players_total: max_players_total(max_games, &cfg),
                cert_sha256: a.cert_sha256,
                public_addr: a.public_addr,
            };
            let out = a.out.unwrap_or_else(|| cfg.folder.clone());
            let added = add_node(
                &n,
                &cfg.nodes,
                &out,
                realm_addr,
                fingerprint(&realm_id.cert),
                tickets.public_keys(),
            )?;
            println!("added game server {} to {}", n.id, cfg.nodes.display());
            println!("bundle:      {}", added.bundle.display());
            if let Some(dir) = &added.identity {
                println!(
                    "identity:    {} (copy it to the game server)",
                    dir.display()
                );
            }
            println!("cert_sha256 = {}", added.cert_sha256);
            Ok(ExitCode::SUCCESS)
        }
        Command::Backup(dest) => {
            offline(&cfg)?;
            let accounts = Accounts::open(&cfg)
                .map_err(|e| e.to_string())?
                .with_backup_db(realm_db_in_data(&cfg.game));
            let dir = match dest {
                Some(d) => {
                    let n = accounts.backup_into(&d)?;
                    println!("backed up {n} characters to {}", d.display());
                    d
                }
                None => backup_now(&accounts, &cfg.backups, cfg.backup_keep)?,
            };
            println!("ok {}", dir.display());
            Ok(ExitCode::SUCCESS)
        }
        Command::SetPassword(name) => {
            offline(&cfg)?;
            let mut password = String::new();
            std::io::stdin()
                .read_line(&mut password)
                .map_err(|e| e.to_string())?;
            let password = password.trim_end_matches(['\r', '\n']);
            let accounts = Accounts::open(&cfg).map_err(|e| e.to_string())?;
            accounts.set_password(&name, password)?;
            println!("password of {name} set");
            Ok(ExitCode::SUCCESS)
        }
        Command::Disable(name, disabled) => {
            offline(&cfg)?;
            let accounts = Accounts::open(&cfg).map_err(|e| e.to_string())?;
            accounts.set_disabled(&name, disabled)?;
            println!("{name} {}", if disabled { "disabled" } else { "enabled" });
            Ok(ExitCode::SUCCESS)
        }
        Command::SetProgression(name, progression) => {
            offline(&cfg)?;
            let accounts = Accounts::open(&cfg).map_err(|e| e.to_string())?;
            accounts.set_progression(&name, progression)?;
            println!("{name} progression set to {progression}");
            Ok(ExitCode::SUCCESS)
        }
        Command::SeasonStart(..) | Command::SeasonEnd => {
            offline(&cfg)?;
            let accounts = Accounts::open(&cfg).map_err(|e| e.to_string())?;
            let log = Arc::new(
                ServerLog::open_named(&cfg.logs, LOG_FILE)
                    .map_err(|e| format!("opening the log in {}: {e}", cfg.logs.display()))?,
            );
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|e| e.to_string())?;
            let ladder = Ladder::new(accounts, &cfg.game, cfg.ladder.clone(), None, log);
            let done = rt.block_on(async {
                match &command {
                    Command::SeasonStart(name, test) => ladder.season_start(name, *test).await,
                    _ => ladder.season_end().await,
                }
            })?;
            println!("{done}");
            Ok(ExitCode::SUCCESS)
        }
        Command::NodeName(id, name) => {
            offline(&cfg)?;
            let n = d2rust_realm::realm::nodes::rename(&cfg.nodes, &id, &name)?;
            println!(
                "game server {} shows as \"{}\" ({})",
                n.id,
                n.display_name,
                cfg.nodes.display()
            );
            Ok(ExitCode::SUCCESS)
        }
        Command::ListNodes => {
            let nodes = d2rust_realm::realm::nodes::load(&cfg.nodes)?;
            println!("{} game servers in {}", nodes.len(), cfg.nodes.display());
            for n in &nodes {
                println!("{}", node_line(n));
            }
            Ok(ExitCode::SUCCESS)
        }
        Command::Reindex => {
            offline(&cfg)?;
            let accounts = Accounts::open(&cfg).map_err(|e| e.to_string())?;
            let r = accounts.reindex()?;
            println!("{} characters reindexed", r.rows);
            for s in &r.skipped {
                println!("skipped: {s}");
            }
            Ok(ExitCode::SUCCESS)
        }
    }
}

/// Admin commands change files the running main server holds: refused
/// while it runs.
fn offline(cfg: &RealmConfig) -> Result<(), String> {
    match ServerLock::running(&cfg.accounts.data) {
        Some(pid) => Err(format!(
            "the main server runs (process {pid}); stop it first (server-ctl stop)"
        )),
        None => Ok(()),
    }
}

/// One game server of `--list-nodes`: its id, the name players see, and
/// what it was added with.
fn node_line(n: &d2rust_realm::realm::nodes::NodeEntry) -> String {
    let public = if n.public_addr.is_empty() {
        "the link's address"
    } else {
        n.public_addr.as_str()
    };
    format!(
        "{:<15} shown as \"{}\"  (name \"{}\", location \"{}\", players reach {public}, {} games)",
        n.id,
        d2rust_realm::realm::listing::display_name(n),
        n.name,
        n.location,
        n.max_games
    )
}

/// The most players of `max_games` games of the main server's size.
fn max_players_total(max_games: u16, cfg: &RealmConfig) -> u16 {
    max_games.saturating_mul(u16::try_from(cfg.max_players).unwrap_or(u16::MAX))
}

/// `--check`: the config, the folders, the log and the game-server list,
/// without the network.
fn check(path: &Path, cfg: &RealmConfig) -> Result<ExitCode, String> {
    if !cfg.game_data.is_dir() {
        return Err(format!(
            "game data folder {} doesn't exist",
            cfg.game_data.display()
        ));
    }
    let log = ServerLog::open_named(&cfg.logs, LOG_FILE)
        .map_err(|e| format!("opening the log in {}: {e}", cfg.logs.display()))?;
    let accounts = Accounts::open(cfg).map_err(|e| e.to_string())?;
    let nodes = d2rust_realm::realm::nodes::load(&cfg.nodes)?;
    let identity = match identity::load(&cfg.identity) {
        Ok(id) => identity::fingerprint_of(&id),
        Err(_) => "none yet (--new-identity)".into(),
    };
    log.line(&format!(
        "config {}: listen {}, {} game servers",
        path.display(),
        cfg.listen,
        nodes.len()
    ));
    println!("version:    {VERSION}");
    println!("config:     {}", path.display());
    println!("listen:     {}", cfg.listen);
    println!("game data:  {}", cfg.game_data.display());
    println!("data:       {}", accounts.config().data.display());
    println!("nodes:      {} ({})", cfg.nodes.display(), nodes.len());
    println!("identity:   {identity}");
    println!("log:        {}", log.path().display());
    Ok(ExitCode::SUCCESS)
}

/// The address a game server on this machine reaches `addr` at.
fn reachable(addr: SocketAddr) -> SocketAddr {
    let ip = match addr.ip() {
        IpAddr::V4(ip) if ip.is_unspecified() => IpAddr::V4(Ipv4Addr::LOCALHOST),
        IpAddr::V6(ip) if ip.is_unspecified() => IpAddr::V6(Ipv6Addr::LOCALHOST),
        ip => ip,
    };
    SocketAddr::new(ip, addr.port())
}

/// The economy's database (`realm.sqlite`: the online stash and the
/// market) in the data folder, which every backup holds.
fn realm_db_in_data(game: &str) -> PathBuf {
    Path::new("realms").join(game).join("realm.sqlite")
}

/// What this build serves (the ready line's `caps`): the accounts and
/// characters it keeps across restarts (`persist`) and its offline admin
/// commands (`admin`), the online shared stash (`stash`), the market's
/// messages (`market`: served even with the switch off, whose state says
/// so), the ladder's (`ladder`: with it off, season 0 alone) and what its
/// social service serves (`social`, `guilds`, `friends`); the games'
/// capabilities are its game servers'.
fn caps<L: LobbyApi>(lobby: &L, social: Option<&dyn SocialService>) -> Vec<&'static str> {
    let mut caps = vec!["connect", "accounts", "characters"];
    caps.extend_from_slice(lobby.caps());
    caps.extend([
        "nodes", "persist", "admin", "journal", "stash", "market", "ladder", "upgrade",
    ]);
    caps.extend(social.map(|s| s.caps()).unwrap_or_default());
    caps
}

/// Runs the main server until SIGTERM or Ctrl+C.
fn serve(path: &Path, cfg: &RealmConfig) -> Result<ExitCode, String> {
    let started = Instant::now();
    let log = Arc::new(
        ServerLog::open_named(&cfg.logs, LOG_FILE)
            .map_err(|e| format!("opening the log in {}: {e}", cfg.logs.display()))?,
    );
    let _lock = ServerLock::take(&cfg.accounts.data)?;
    let version = link_version(cfg.dev_test);
    log.line(&format!("starting {version} with {}", path.display()));
    if cfg.dev_test {
        log.line("WARNING: dev = test (test servers only)");
    }
    let (identity, made) = identity::load_or_make(&cfg.identity)?;
    let cert = identity::fingerprint_of(&identity);
    if made {
        log.line(&format!("made the identity in {}", cfg.identity.display()));
    }
    let (tickets, made) = RealmTickets::load_or_make(&cfg.identity)?;
    log.line(&format!(
        "join tickets signed with key {}{}; game servers get key ids {:?}",
        tickets.key_id(),
        if made { " (made now)" } else { "" },
        tickets
            .public_keys()
            .iter()
            .map(|k| k.key_id)
            .collect::<Vec<_>>()
    ));
    if !cfg.game_data.is_dir() {
        return Err(format!(
            "game data folder {} doesn't exist",
            cfg.game_data.display()
        ));
    }
    let t = Instant::now();
    let data_hash = game_data_hash(&cfg.game_data, Some(&cfg.logs))
        .map_err(|e| format!("hashing the game data {}: {e}", cfg.game_data.display()))?;
    log.line(&format!(
        "game data hashed in {} ms",
        t.elapsed().as_millis()
    ));
    let accounts = Accounts::open(cfg)
        .map_err(|e| e.to_string())?
        .with_log(log.clone())
        .with_backup_db(realm_db_in_data(&cfg.game));
    let market_on = cfg.market.economy.market;
    if market_on {
        log.line("the market is on");
    }
    if cfg.ladder.on {
        log.line("the ladder is on");
    }
    // The save checks' tables (the ladder reads the saves' experience with
    // them too), and the market's item reader (its names and the type
    // tree).
    let (tables, reader) = if cfg.guard.needs_tables() || market_on || cfg.ladder.on {
        let t = Instant::now();
        let data = guard::load_game_data(&cfg.game_data)?;
        let tables = Arc::new(data.tables);
        let reader = if market_on {
            let r = ItemReader::new(&data.fixed, data.strings, tables.clone())
                .map_err(|e| format!("the market's item names: {e}"))?;
            Some(Arc::new(r))
        } else {
            None
        };
        log.line(&format!(
            "game data tables loaded in {} ms",
            t.elapsed().as_millis()
        ));
        (Some(tables), reader)
    } else {
        (None, None)
    };
    let ladder = Ladder::new(
        accounts.clone(),
        &cfg.game,
        cfg.ladder.clone(),
        tables.clone(),
        log.clone(),
    );
    let tables = tables.filter(|_| cfg.guard.needs_tables());
    let economy = cfg
        .market
        .economy_config(reader.as_deref())
        .map_err(|e| format!("the market: {e}"))?;
    let realm_db = guard::realm_db(&cfg.accounts.data, &cfg.game);
    let save_guard = Arc::new(SaveGuard::open(
        &realm_db,
        &cfg.game,
        tables,
        cfg.guard.clone(),
        log.clone(),
    )?);
    log.line(&format!(
        "save checks {:?}, item registry {:?} ({})",
        cfg.guard.checks,
        cfg.guard.registry,
        realm_db.display()
    ));
    let stash = Stash::open_with(
        &realm_db,
        &cfg.game,
        &save_guard,
        economy.clone(),
        cfg.stash.clone(),
        log.clone(),
    )?;
    // The market's requests run on the stash's economy worker.
    let market = Market::new(
        stash.bank().clone(),
        &cfg.game,
        economy,
        reader,
        log.clone(),
    );
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(cfg.net_threads)
        .thread_name("d2rust-realm-net")
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;
    // Before anything is served: the records of a former run's stash
    // moves go to their files, and its transfers in flight back.
    rt.block_on(stash.recover(&accounts))
        .map_err(|e| format!("the online stash: {e}"))?;
    let server = ServerInfo {
        protocol: d2_proto::PROTOCOL_VERSION,
        game: cfg.game.clone(),
        build: VERSION.into(),
        tick_rate: d2_sim::tick::TickRate::GAME.hz(),
    };
    let realm = Arc::new(
        Realm::new(
            NodeRegistry::new(cfg.nodes.clone(), log.clone()),
            Versions {
                version: version.clone(),
                protocol: server.protocol,
                tick_rate: server.tick_rate,
                game: cfg.game.clone(),
                data_hash,
            },
            cfg.save_interval.as_secs() as u32,
            tickets,
            // A character of an ended ladder season becomes non-ladder as
            // its record is loaded for a game.
            SaveDesk::new(LeaseTable::starting_now(), accounts.clone(), log.clone())
                .on_seat_load(SeatLoaded(ladder.clone())),
            save_guard,
            Journals::new(cfg.journals.clone(), log.clone()),
            log.clone(),
        )
        .with_stash(stash.clone()),
    );
    let (news_tx, news_rx) = tokio::sync::mpsc::unbounded_channel();
    realm.set_news(news_tx);
    let sessions = Sessions::default();
    let social = Social::new(
        cfg.social,
        accounts.clone(),
        &cfg.game,
        &cfg.mod_label,
        cfg.accounts.limits == d2rust_realm::accounts::LimitMode::Test,
        log.clone(),
    );
    let lobby = Arc::new(
        Lobby::new(lobby::limits(cfg), sessions.clone(), realm.clone()).with_social(social.clone()),
    );
    let games: std::sync::Weak<Lobby> = Arc::downgrade(&lobby);
    social.friends().set_games(games);
    let caps = caps(lobby.as_ref(), Some(social.as_ref()));
    let outlet = Arc::new(UpgradeOutlet {
        realm: realm.clone(),
        social: social.clone(),
    });
    // `dev = test`: the test tool's lines on standard input (contracts
    // 10.1); a character's progression changes while it is out of a game,
    // the market's and the chat's test lines.
    // The network side's part of the `stats` line, once it exists.
    let net_stats: Arc<OnceLock<Box<dyn Fn() -> String + Send + Sync>>> = Arc::default();
    if cfg.dev_test {
        let (lobby, accounts, market) = (lobby.clone(), accounts.clone(), market.clone());
        let (ladder, social, rt) = (ladder.clone(), social.clone(), rt.handle().clone());
        let net_stats = net_stats.clone();
        let (realm, outlet, log) = (realm.clone(), outlet.clone(), log.clone());
        std::thread::Builder::new()
            .name("d2rust-realm-dev".into())
            .spawn(move || {
                d2rust_realm::console::serve(|line| match line {
                    Line::SetProgression(r) => {
                        if lobby.character_in_game(&r.character.to_ascii_lowercase()) {
                            return Err(format!(
                                "{} is in a game; the progression is set while it is out",
                                r.character
                            ));
                        }
                        accounts.set_progression(&r.character, r.progression)?;
                        Ok(format!(
                            "{} progression set to {}",
                            r.character, r.progression
                        ))
                    }
                    Line::EconSeed(s) => {
                        let id = accounts
                            .account_id(&s.account)?
                            .ok_or_else(|| format!("no account named `{}`", s.account))?;
                        market.seed(id, &s.split, s.goods.clone())
                    }
                    Line::EconClock { ms } => {
                        let offset = market.clock().advance(*ms);
                        market.sweep_now()?;
                        Ok(format!("market clock +{} s; swept", offset / 1000))
                    }
                    Line::SeasonStart { name, test } => {
                        rt.block_on(ladder.season_start(name, *test))
                    }
                    Line::SeasonEnd => rt.block_on(ladder.season_end()),
                    Line::LadderSeed(s) => {
                        rt.block_on(ladder.seed(&s.character, s.level, s.experience, s.dead))
                    }
                    Line::Notice { text } => social.notice(text),
                    Line::Mute {
                        account,
                        minutes,
                        reason,
                    } => rt.block_on(social.mute(account, *minutes, reason)),
                    Line::Unmute { account } => rt.block_on(social.unmute(account)),
                    Line::SocialClock { ms } => rt.block_on(social.advance_clock(*ms)),
                    // No package to fetch on a test setup: ready at once.
                    Line::Upgrade(r) => realm.upgrade.start(r, Instant::now(), true),
                    Line::UpgradeCancel => {
                        let a = realm.upgrade.cancel("the dev console")?;
                        upgrade::act(&[a], outlet.as_ref(), &log);
                        Ok("upgrade called off".into())
                    }
                    Line::Stats => {
                        let db = accounts.db_use();
                        Ok(format!(
                            "{} {} db_locks={} db_wait_us={} db_wait_max_us={} db_held_us={} db_held_max_us={}",
                            net_stats.get().map_or_else(|| "sessions=0".into(), |f| f()),
                            social.stats(),
                            db.locks,
                            db.waited_us,
                            db.max_wait_us,
                            db.held_us,
                            db.max_held_us
                        ))
                    }
                })
            })
            .map_err(|e| format!("starting the dev line reader: {e}"))?;
    }
    let node_list = lobby.clone();
    let net = Arc::new(
        Net::new(
            NetConfig {
                server: server.clone(),
                server_name: cfg.server_name.clone(),
                max_players: cfg.max_players as u32,
                max_sessions: cfg.max_sessions,
                grace: cfg.grace,
            },
            accounts.clone(),
            lobby,
            sessions,
            log.clone(),
        )
        .with_stash(realm.clone())
        .with_market(market.clone())
        .with_ladder(ladder.clone())
        .with_social(social.clone()),
    );
    let _ = net_stats.set({
        let net = net.clone();
        Box::new(move || {
            let n = net.stats();
            format!(
                "sessions={} ctxs={} held={} tokens={} logins={} unauthenticated={} out_peak={}",
                n.open, n.ctxs, n.held, n.tokens, n.by_account, n.unauthenticated, n.out_peak
            )
        })
    });
    let realm_cert = fingerprint(&identity.cert);
    let quic = rt.block_on(async {
        QuicServer::bind_with(
            cfg.listen,
            identity,
            Timing::default(),
            tokio::runtime::Handle::current(),
            Listen::PlayersAndNodes,
        )
    });
    let quic = Arc::new(quic.map_err(|e| format!("listening on {}: {e}", cfg.listen))?);
    let addr = quic.local_addr().map_err(|e| e.to_string())?;
    log.line(&format!("listening on {addr}, cert_sha256 {cert}"));
    // Test setups: their game servers, added at each start (contracts 11.10).
    if let Some(dir) = &cfg.bundle_dir {
        for id in &cfg.test_nodes {
            let n = AddNode {
                id: id.clone(),
                name: id.clone(),
                location: id.clone(),
                game: cfg.game.clone(),
                max_games: u16::try_from(cfg.max_games).unwrap_or(u16::MAX),
                max_players_total: max_players_total(
                    u16::try_from(cfg.max_games).unwrap_or(u16::MAX),
                    cfg,
                ),
                cert_sha256: None,
                public_addr: None,
            };
            let added = add_node(
                &n,
                &cfg.nodes,
                dir,
                reachable(addr),
                realm_cert,
                realm.tickets.public_keys(),
            )?;
            log.line(&format!(
                "test game server {id}: bundle {}",
                added.bundle.display()
            ));
        }
    }
    ReadyLine::new()
        .field("program", "realm")
        .field("listen", addr)
        .field("cert_sha256", &cert)
        .field("game", &cfg.game)
        .field("build", VERSION)
        .field("protocol", server.protocol)
        .field("tick", server.tick_rate)
        .field("caps", caps.join(","))
        .print();
    let stop = rt.block_on(async {
        tokio::spawn(net.clone().run(quic.clone(), realm.clone()));
        tokio::spawn(net.clone().run_news(news_rx));
        tokio::spawn(node_list.run_node_list());
        tokio::spawn(ladder.clone().run_saves());
        tokio::spawn(upgrade::drive(
            realm.upgrade.clone(),
            outlet.clone(),
            cfg.folder.clone(),
            cfg.logs.clone(),
            cfg.dev_test,
            log.clone(),
        ));
        let tick = {
            let (net, realm, logs, log) =
                (net.clone(), realm.clone(), cfg.logs.clone(), log.clone());
            let (folder, backups, keep) =
                (cfg.folder.clone(), cfg.backups.clone(), cfg.backup_keep);
            let (market, social) = (market.clone(), social.clone());
            tokio::spawn(async move {
                let mut every = tokio::time::interval(Duration::from_secs(1));
                let mut n: u64 = 0;
                loop {
                    every.tick().await;
                    net.expire_held().await;
                    n += 1;
                    if n.is_multiple_of(SWEEP_S) {
                        market.sweep();
                    }
                    if n.is_multiple_of(SOCIAL_SWEEP_S) {
                        let social = social.clone();
                        tokio::spawn(async move {
                            if let Err(e) = social.sweep().await {
                                social.log_line(&format!("sweep: {e}"));
                            }
                        });
                    }
                    if n.is_multiple_of(NODES_FILE_S) {
                        let rows = realm.nodes.rows(&realm.versions.version, Instant::now());
                        let json = nodes::nodes_json(&realm.versions.version, &rows);
                        if let Err(e) = nodes::write_nodes_json(&logs, &json) {
                            log.line(&format!("{}: {e}", nodes::NODES_FILE));
                        }
                    }
                    if n.is_multiple_of(STATUS_INTERVAL.as_secs()) {
                        let linked = realm.nodes.linked_now();
                        let s = Status {
                            version: version.clone(),
                            uptime_s: started.elapsed().as_secs(),
                            sessions: net.session_count(),
                            games: linked.iter().map(|l| usize::from(l.load.games)).sum(),
                            players: linked.iter().map(|l| usize::from(l.load.players)).sum(),
                            tick_overruns: linked
                                .iter()
                                .map(|l| u64::from(l.load.tick_overruns))
                                .sum(),
                        };
                        if let Err(e) = s.write(&logs) {
                            log.line(&format!("status.json: {e}"));
                        }
                        let (folder, accounts) = (folder.clone(), accounts.clone());
                        let (backups, keep) = (backups.clone(), keep);
                        let _ = tokio::task::spawn_blocking(move || {
                            answer_backup_request(&folder, &accounts, &backups, keep)
                        })
                        .await;
                    }
                }
            })
        };
        let why = wait_for_stop().await;
        tick.abort();
        why
    });
    log.line(&format!("stopping ({stop})"));
    quic.close();
    rt.shutdown_timeout(Duration::from_secs(2));
    log.line("stopped");
    Ok(ExitCode::SUCCESS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_wildcard_listen_is_reached_on_loopback() {
        let any: SocketAddr = "0.0.0.0:6112".parse().unwrap();
        assert_eq!(reachable(any), "127.0.0.1:6112".parse().unwrap());
        let any6: SocketAddr = "[::]:6112".parse().unwrap();
        assert_eq!(reachable(any6), "[::1]:6112".parse().unwrap());
        let one: SocketAddr = "203.0.113.10:6112".parse().unwrap();
        assert_eq!(reachable(one), one);
    }

    #[test]
    fn the_main_server_lists_nodes_and_no_games() {
        let listed = caps(&d2rust_realm::session::NoLobby, None);
        assert!(listed.contains(&"nodes"), "{listed:?}");
        assert!(listed.contains(&"persist") && listed.contains(&"admin"));
        assert!(
            listed.contains(&"stash") && listed.contains(&"market") && listed.contains(&"ladder"),
            "{listed:?}"
        );
        for cap in ["game", "walk", "dev"] {
            assert!(!listed.contains(&cap), "{cap}: {listed:?}");
        }
    }
}
