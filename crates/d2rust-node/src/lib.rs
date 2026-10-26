//! `d2rust-node`: an official game server (multiplayer v2, contracts
//! 11.1), as a library and D2Gamma's thin binary over it. It runs games
//! only: no accounts, no character folder, no SQLite.
//! It links out to its main server (`d2rust-realm`) over QUIC from the
//! UDP port its players connect to, and the main server sends it games
//! and characters.
//!
//! ```text
//! d2rust-node [--config FILE]                 run the game server
//! d2rust-node --version                       print `d2rust-node <game version>`
//! d2rust-node [--config FILE] --check         read the config and the bundle, open the log, stop
//! d2rust-node [--config FILE] --new-identity  make the certificate (once; kept across versions)
//! d2rust-node [--config FILE] --fingerprint   print the certificate's SHA-256
//! d2rust-node [--config FILE] --probe         load the game data, play one game with a new
//!                                             character of every class, check their saves
//! ```
//!
//! The config file defaults to `d2rust-node.cfg` in the current folder.
//! Running, it reads its bundle, hashes its game data, listens on its UDP
//! port, links to the main server and, once welcomed, prints one line
//! `ready program=node node=… listen=… cert_sha256=… realm=… linked=1
//! caps=…` (contracts 11.10). A lost link prints `link lost reason=…`,
//! links again and prints the ready line again. A refused link (another
//! version, game data or mod realm, an unknown certificate) exits 1 with
//! the code (with an updater and another version, `NodeBuild`: it links
//! again later). SIGTERM, Ctrl+C or the stop marker `run/stop` (Windows'
//! `node-ctl.ps1 stop`): every game saves and it exits 0. It writes
//! `logs/status.json` at the start and every 10 s (`server-ctl`'s health
//! check). An upgrade ([`update`]): the main server's countdown shows in
//! every game; at zero every game saves and closes, and with the new
//! version staged (`update_command`) it writes `run/swap` and exits 75 for
//! its supervisor to swap. Standard input is not read
//! (under a service manager it is empty), except by a test setup's game
//! server (`dev = test`): dev commands ([`devcmd`]); there
//! `D2RUST_TEST_VERSION` names the version it says it runs (the upgrade's
//! test plays two versions with one build).
//!
//! Players attach with the main server's join tickets, checked here with
//! the keys of the link's welcome ([`tickets`]), and play the games the
//! main server starts here ([`seats`]); their saves go to the main server.
//! On SIGTERM or Ctrl+C every game saves its players and closes first.
//!
//! **A mod game's game server** (`docs/plans/mods/seams.md` MS15) is this
//! program with its own [`NodeGame`]: a binary of its own whose `main`
//! calls [`main`] with the mod game's program name, game id and version,
//! data layers, rules and character format. Its games are built with the
//! mod game's rules and save in its format; its hello names its game, and
//! a main server of another game refuses it (`NodeGame`). D2Gamma's binary
//! passes [`NodeGame::own`].
//!
//! ```no_run
//! # use d2rust_node::NodeGame;
//! fn main() -> std::process::ExitCode {
//!     d2rust_node::main(NodeGame {
//!         program: "mygame-node".into(),
//!         game: "mygame".into(),
//!         ..NodeGame::own()
//!     })
//! }
//! ```

mod config;
mod devcmd;
mod endpoint;
mod journal;
mod link;
mod seats;
mod tickets;
mod update;

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;
use std::time::{Duration, Instant};

use d2_games::{GameRules, GamesConfig};
use d2_link::{Bundle, Versions};
use d2_net::wire::Timing;
use d2_quic::{Listen, QuicServer};
use d2_server::game::{GameData, Layers};
use d2_service::datahash::game_data_hash;
use d2_service::identity;
use d2_service::log::ServerLog;
use d2_service::status::{Status, STATUS_INTERVAL};
use d2_service::stop::{wait_for_stop, wait_for_stop_file};

use config::NodeConfig;
use link::LinkSetup;
use seats::NodeGames;
use tickets::Tickets;
use update::{Updater, SWAP_EXIT};

/// The game version, the same four parts as the game's (`build.rs`).
pub const VERSION: &str = env!("D2RUST_VERSION");

/// D2RUST's game id: the mod realm of the plain game (`Hello.game`).
pub const GAME: &str = "d2rust";

/// The game a game server serves: what its hello says it is, and what its
/// games are built with.
#[derive(Clone, Debug)]
pub struct NodeGame {
    /// The program's name: its `--version` line, its usage, its default
    /// config file (`NAME.cfg`) and its log file (`NAME.log`).
    pub program: String,
    /// The game id its hello names (`NodeHello.game`): the main server
    /// takes only a game server of its own game.
    pub game: String,
    /// The version its hello names and `status.json` shows.
    pub version: String,
    /// The layers a converted native game data folder stacks over its
    /// `base/`.
    pub layers: Layers,
    /// The rules every game is built with, and the characters' save
    /// format.
    pub rules: GameRules,
}

impl NodeGame {
    /// D2RUST's own game server: `d2rust-node`, game [`GAME`] at
    /// [`VERSION`], `base/` alone, D2RUST's rules and `.d2s` characters.
    pub fn own() -> Self {
        Self {
            program: "d2rust-node".into(),
            game: GAME.into(),
            version: VERSION.into(),
            layers: Layers::base(),
            rules: GameRules::default(),
        }
    }

    /// The version this game server says it runs to its main server and
    /// in `status.json`: the game's; on a test setup `D2RUST_TEST_VERSION`
    /// when set.
    fn link_version(&self, dev_test: bool) -> String {
        std::env::var("D2RUST_TEST_VERSION")
            .ok()
            .filter(|v| dev_test && d2_net::msg::node::valid_update_version(v))
            .unwrap_or_else(|| self.version.clone())
    }

    /// The log file in the log folder.
    fn log_file(&self) -> String {
        format!("{}.log", self.program)
    }

    /// What its hello says it runs: `version`, this build's protocol and
    /// tick rate, its game, and the hash of its game data.
    fn versions(&self, version: String, data_hash: [u8; 32]) -> Versions {
        Versions {
            version,
            protocol: d2_proto::PROTOCOL_VERSION,
            tick_rate: d2_sim::tick::TickRate::GAME.hz(),
            game: self.game.clone(),
            data_hash,
        }
    }

    /// `Err` when the bundle was written by a main server of another game:
    /// that main server refuses this game server (`NodeGame`).
    fn check_bundle(&self, bundle: &Bundle) -> Result<(), String> {
        if bundle.game == self.game {
            return Ok(());
        }
        Err(format!(
            "the bundle is for game {}; this game server runs {}",
            bundle.game, self.game
        ))
    }
}

/// How the game server stops.
enum Stopped {
    Refused(d2_link::Refusal),
    /// SIGTERM or Ctrl+C.
    Asked,
    /// An upgrade's version is staged: the supervisor swaps to it.
    Swap(String),
}

enum Command {
    Run,
    Check,
    NewIdentity,
    Fingerprint,
    Probe,
}

/// The game server of `game`, on the program's arguments: what a game
/// server binary's `main` returns.
pub fn main(game: NodeGame) -> ExitCode {
    let mut config = PathBuf::from(format!("{}.cfg", game.program));
    let mut command = None;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        let next = match a.as_str() {
            "--version" => {
                println!("{} {}", game.program, game.version);
                return ExitCode::SUCCESS;
            }
            "--config" => match args.next() {
                Some(p) => {
                    config = PathBuf::from(p);
                    continue;
                }
                None => return usage(&game),
            },
            "--check" => Command::Check,
            "--new-identity" => Command::NewIdentity,
            "--fingerprint" => Command::Fingerprint,
            "--probe" => Command::Probe,
            _ => return usage(&game),
        };
        if command.replace(next).is_some() {
            return usage(&game);
        }
    }
    match run(&game, &config, command.unwrap_or(Command::Run)) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("{}: {e}", game.program);
            ExitCode::FAILURE
        }
    }
}

fn usage(game: &NodeGame) -> ExitCode {
    eprintln!(
        "usage: {} [--config FILE] [--check | --new-identity | --fingerprint | --probe] | --version",
        game.program
    );
    ExitCode::from(2)
}

fn run(game: &NodeGame, path: &Path, command: Command) -> Result<ExitCode, String> {
    let cfg = NodeConfig::load(path).map_err(|e| e.to_string())?;
    cfg.check_game(&game.game)?;
    match command {
        Command::Run => serve(game, path, &cfg),
        Command::Check => check(game, path, &cfg),
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
        Command::Probe => {
            let log = Arc::new(
                ServerLog::open_named(&cfg.logs, &game.log_file())
                    .map_err(|e| format!("opening the log in {}: {e}", cfg.logs.display()))?,
            );
            let data = game_data(&cfg.game_data, &game.layers)?;
            let games = GamesConfig {
                grace: Duration::ZERO,
                ..GamesConfig::from_server(&cfg)
            };
            let rules = game.rules.clone();
            println!("{}", d2_games::probe::probe_with(data, games, rules, log)?);
            Ok(ExitCode::SUCCESS)
        }
    }
}

/// `--check` (`server-ctl install` runs it before it switches): the
/// config, the bundle (written for this game), the log and the game data
/// folder, without the network.
fn check(game: &NodeGame, path: &Path, cfg: &NodeConfig) -> Result<ExitCode, String> {
    let log = ServerLog::open_named(&cfg.logs, &game.log_file())
        .map_err(|e| format!("opening the log in {}: {e}", cfg.logs.display()))?;
    let bundle =
        Bundle::load(&cfg.bundle).map_err(|e| format!("bundle {}: {e}", cfg.bundle.display()))?;
    cfg.check_bundle(&bundle)?;
    game.check_bundle(&bundle)
        .map_err(|e| format!("bundle {}: {e}", cfg.bundle.display()))?;
    if !cfg.game_data.is_dir() {
        return Err(format!(
            "game data folder {} doesn't exist",
            cfg.game_data.display()
        ));
    }
    log.line(&format!("config {}: checked", path.display()));
    println!("version:    {}", game.version);
    println!("game:       {}", game.game);
    println!("config:     {}", path.display());
    println!("node:       {}", bundle.id);
    println!(
        "realm:      {}",
        cfg.realm_addr.unwrap_or(bundle.realm_addr)
    );
    println!("listen:     {}", cfg.listen);
    println!("game data:  {}", cfg.game_data.display());
    println!("log:        {}", log.path().display());
    Ok(ExitCode::SUCCESS)
}

/// What its games serve (the ready line's `caps`): the main server's
/// games (`node-games`) and the game's own rules once its game data
/// loaded, dev commands on a test setup's game server, and `hostile` in
/// a hostile test build (its misbehaviours can be asked for).
fn caps(can_play: bool, dev_test: bool) -> Vec<&'static str> {
    if !can_play {
        return Vec::new();
    }
    let mut caps = vec![
        "node-games",
        "game",
        "walk",
        "combat",
        "items",
        "waypoints",
        "acts",
        "journal",
    ];
    if dev_test {
        caps.push("dev");
    }
    if cfg!(feature = "hostile") {
        caps.push("hostile");
    }
    caps
}

/// The game data: a converted native folder (it has `base/`) with
/// `layers` over it, or a game install's archives.
fn game_data(dir: &Path, layers: &Layers) -> Result<GameData, String> {
    let data = if dir.join("base").is_dir() {
        GameData::select_native_with(dir, layers)
    } else {
        GameData::select(Some(dir))
    };
    data.map_err(|e| format!("game data {}: {e}", dir.display()))
}

/// Runs the game server until SIGTERM or Ctrl+C, or until the main server
/// refuses it.
fn serve(game: &NodeGame, path: &Path, cfg: &NodeConfig) -> Result<ExitCode, String> {
    let log = Arc::new(
        ServerLog::open_named(&cfg.logs, &game.log_file())
            .map_err(|e| format!("opening the log in {}: {e}", cfg.logs.display()))?,
    );
    let version = game.link_version(cfg.dev_test);
    log.line(&format!(
        "starting {} {version} with {}",
        game.game,
        path.display()
    ));
    if cfg.dev_test {
        log.line("WARNING: dev = test (test setups only)");
    }
    if let Some(mode) = &cfg.hostile {
        log.line(&format!("WARNING: hostile = {mode} (a test build)"));
    }
    let bundle =
        Bundle::load(&cfg.bundle).map_err(|e| format!("bundle {}: {e}", cfg.bundle.display()))?;
    cfg.check_bundle(&bundle)?;
    if let Err(e) = game.check_bundle(&bundle) {
        // Its main server says so too: it refuses the link (`NodeGame`).
        log.line(&format!("WARNING: {e}"));
    }
    let (identity, made) = identity::load_or_make(&cfg.identity)?;
    let cert = identity::fingerprint_of(&identity);
    if made {
        log.line(&format!(
            "made the identity in {}; the main server must know cert_sha256 {cert}",
            cfg.identity.display()
        ));
    }
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
    let t = Instant::now();
    let data = game_data(&cfg.game_data, &game.layers);
    if data.is_ok() {
        log.line(&format!(
            "game data loaded in {} ms",
            t.elapsed().as_millis()
        ));
    }
    let can_play = data.is_ok();
    let versions = game.versions(version.clone(), data_hash);
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .thread_name("d2rust-node-net")
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;
    let quic = rt.block_on(async {
        QuicServer::bind_with(
            cfg.listen,
            identity.clone(),
            Timing::default(),
            tokio::runtime::Handle::current(),
            Listen::Players,
        )
    });
    let quic = Arc::new(quic.map_err(|e| format!("listening on {}: {e}", cfg.listen))?);
    let addr = quic.local_addr().map_err(|e| e.to_string())?;
    let realm_addr = cfg.realm_addr.unwrap_or(bundle.realm_addr);
    log.line(&format!(
        "game server {} listening on {addr} (players told {}), cert_sha256 {cert}; linking to {realm_addr}",
        bundle.id,
        cfg.public_addr.map_or("the address the link comes from".into(), |a| a.to_string()),
    ));
    let tickets = Arc::new(Tickets::default());
    let games = NodeGames::new(
        GamesConfig::from_server(cfg),
        data,
        game.rules.clone(),
        (version.clone(), data_hash),
        tickets.clone(),
        log.clone(),
        rt.handle().clone(),
    );
    if cfg.dev_test {
        let dev = games.clone();
        std::thread::Builder::new()
            .name("d2rust-node-dev".into())
            .spawn(move || devcmd::serve(|character, cmd| dev.dev_command(character, cmd)))
            .map_err(|e| format!("starting the dev commands: {e}"))?;
    }
    let updater = Updater::new(cfg.update.clone(), &version, &cfg.folder, log.clone());
    if updater.managed() {
        log.line("updates: fetched with update_command");
    }
    let setup = LinkSetup {
        id: bundle.id.clone(),
        realm_addr,
        realm_cert: bundle.realm_cert,
        identity,
        cert_sha256: cert,
        listen: addr,
        hello: versions.hello(
            &bundle.id,
            addr.port(),
            cfg.max_games,
            cfg.max_players_total(),
        ),
        caps: caps(can_play, cfg.dev_test),
        tickets: tickets.clone(),
        games: games.clone(),
        updater: updater.clone(),
        log: log.clone(),
    };
    let started = Instant::now();
    let stopped = rt.block_on(async {
        tokio::spawn(endpoint::serve(
            quic.clone(),
            tickets,
            games.clone(),
            log.clone(),
        ));
        let status = tokio::spawn(write_status(
            games.clone(),
            version.clone(),
            started,
            cfg.logs.clone(),
            log.clone(),
        ));
        let mut link = tokio::spawn(link::run(quic.clone(), setup));
        let stopped = tokio::select! {
            r = &mut link => match r {
                Ok(r) => Stopped::Refused(r),
                Err(_) => Stopped::Asked,
            },
            why = stop_asked(cfg.folder.join(STOP_FILE)) => {
                log.line(&format!("stopping: {why}"));
                // The games save their players while the link is still up.
                let all = games.clone();
                let _ = tokio::task::spawn_blocking(move || all.shutdown_all()).await;
                Stopped::Asked
            }
            // Its games saved and closed at the restart.
            v = updater.swapping() => Stopped::Swap(v),
        };
        link.abort();
        status.abort();
        stopped
    });
    quic.close();
    match stopped {
        Stopped::Refused(r) => Err(format!(
            "the main server at {realm_addr} refused the link: {}: {}",
            r.code, r.text
        )),
        Stopped::Asked => {
            log.line("stopped");
            Ok(ExitCode::SUCCESS)
        }
        Stopped::Swap(v) => {
            log.line(&format!("stopped for the swap to {v}"));
            Ok(ExitCode::from(SWAP_EXIT))
        }
    }
}

/// The stop marker in the game server's folder: Windows' `node-ctl.ps1
/// stop` writes it (the game server it starts has no console for a
/// Ctrl+Break).
const STOP_FILE: &str = "run/stop";

/// A stop signal or the stop marker: which.
async fn stop_asked(marker: PathBuf) -> &'static str {
    tokio::select! {
        why = wait_for_stop() => why,
        why = wait_for_stop_file(marker) => why,
    }
}

/// Writes `logs/status.json` now and every [`STATUS_INTERVAL`]: the
/// version, the games and players (its players are its sessions).
async fn write_status(
    games: Arc<NodeGames>,
    version: String,
    started: Instant,
    logs: PathBuf,
    log: Arc<ServerLog>,
) {
    let mut every = tokio::time::interval(STATUS_INTERVAL);
    loop {
        every.tick().await;
        let load = games.load();
        let s = Status {
            version: version.clone(),
            uptime_s: started.elapsed().as_secs(),
            sessions: usize::from(load.players),
            games: usize::from(load.games),
            players: usize::from(load.players),
            tick_overruns: u64::from(load.tick_overruns),
        };
        if let Err(e) = s.write(&logs) {
            log.line(&format!("status.json: {e}"));
        }
    }
}
