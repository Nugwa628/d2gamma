//! The v2 setups (plan N1, contracts 11.10): a main server and its game
//! servers, **each its own process** (owner rule 11: never one program
//! with roles, never both in one process).
//!
//! - `split`: the main server and one official game server, `n1`;
//! - `split:N`: N official game servers, `n1` .. `nN`;
//! - `host`: the main server and one player-hosted game server, started
//!   like the game client starts it (`d2rust-node --host`, its start line
//!   on stdin, contracts 11.12), named after its hosting bot, `b0`.
//!
//! Every game server reaches the main server through the setup's
//! [`Fabric`], at an address of its own, behind its NAT mode (`nat NAME=MODE`
//! in a scenario), so `cut-link` can cut one link and a restarted main
//! server is found at the same address.
//!
//! The setups run `d2rust-realm` and `d2rust-node`, or stand-ins that need
//! no game files: the synthetic `mp-test-synth` for the main server and
//! `mp-test-stub-node`, a stand-in game server that links with a v1
//! session handshake and speaks the ready lines and the hosting lines;
//! that way the setup code is tested.
//!
//! **The command lines** (agreed through the lanes' STATUS files):
//!
//! ```text
//! d2rust-realm --config d2rust-realm.cfg   listen, game_data, data, logs, dev = test,
//!                                          bundle_dir = bundles, test_nodes = n1,n2 (writes
//!                                          bundles/node-ID.bundle and bundles/node-ID.identity
//!                                          before its ready line)
//! d2rust-node --config d2rust-node.cfg     bundle, identity, listen, game_data, logs,
//!                                          realm_addr (the fabric's address for it), hostile, grace_s,
//!                                          dev = test (dev commands on its stdin)
//! d2rust-node --host                       stdin: start realm=ADDR realm_cert=HEX64 token=HEX32
//!                                          version=V port=0
//! ```
//!
//! Each runs in its own folder and stops on SIGTERM. A game server prints
//! its ready line once linked, `link lost reason=..` when the link drops,
//! and its ready line again when it relinks; an official one runs on when
//! its stdin ends, a hosting one exits.
//!
//! **Upgrades** (`server upgrade`): an official `d2rust-node` fetches with
//! `update_command = test`, which stages every version at once. When it
//! exits [`SWAP_EXIT`] with its [`SWAP_FILE`] marker, the setup starts it
//! again as the marker's version (`D2RUST_TEST_VERSION`, read with
//! `dev = test`), as its supervisor would after a swap. `start-node ID
//! version=V update=off` starts one as another version, without the
//! updater.

use std::collections::BTreeSet;
use std::io::Write;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::net::nat::{Fabric, NatMode};
use crate::server::{
    parse_ready, read_output, terminate, wait_ready, Ready, ReadyLine, StartError, Target,
    NOT_BUILT, START_TIMEOUT, STOP_TIMEOUT,
};

/// The most official game servers a setup starts.
pub const MAX_NODES: usize = 8;

/// The host token the hosting bot hands a stand-in host. A real one comes
/// from the main server (`HostStart` → `HostGrant`, plan N8).
pub const TEST_TOKEN: &str = "00000000000000000000000000000000";

/// How long a game server may take to answer a dev line.
const DEV_WAIT: Duration = Duration::from_secs(5);

/// How long a hosting process may take to exit after `stop` or a closed
/// stdin (contracts 11.12: the client kills it after 10 s).
pub const HOST_EXIT: Duration = Duration::from_secs(10);

/// The exit code of a game server that staged an upgrade (d2rust-node's).
pub const SWAP_EXIT: i32 = 75;

/// The marker naming the staged version, in a game server's folder.
pub const SWAP_FILE: &str = "run/swap";

/// How many swaps one `start-node` follows before its ready line.
const START_SWAPS: u32 = 2;

/// Which programs, and how many.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Setup {
    /// The main server and N official game servers.
    Split(usize),
    /// The main server and one player-hosted game server.
    Host,
}

impl Setup {
    /// Parses `split`, `split:N` or `host`.
    pub fn parse(s: &str) -> Result<Self, String> {
        match s {
            "split" => Ok(Setup::Split(1)),
            "host" => Ok(Setup::Host),
            _ => s
                .strip_prefix("split:")
                .and_then(|n| n.parse::<usize>().ok())
                .filter(|n| (1..=MAX_NODES).contains(n))
                .map(Setup::Split)
                .ok_or_else(|| {
                    format!("setup {s}: unknown (split, split:N with N 1..={MAX_NODES}, host)")
                }),
        }
    }

    /// `split` or `host`.
    pub fn kind(&self) -> &'static str {
        match self {
            Setup::Split(_) => "split",
            Setup::Host => "host",
        }
    }

    /// Whether this setup has what a scenario that names `need` needs:
    /// the hosting setup, or at least as many official game servers.
    pub fn fits(&self, need: &Setup) -> bool {
        match (self, need) {
            (Setup::Split(have), Setup::Split(want)) => have >= want,
            (Setup::Host, Setup::Host) => true,
            _ => false,
        }
    }

    /// The game servers' names: `n1`.. or the hosting bot's `b0`.
    pub fn names(&self) -> Vec<String> {
        match self {
            Setup::Split(n) => (1..=*n).map(|i| format!("n{i}")).collect(),
            Setup::Host => vec!["b0".into()],
        }
    }
}

impl std::fmt::Display for Setup {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Setup::Split(1) => write!(f, "split"),
            Setup::Split(n) => write!(f, "split:{n}"),
            Setup::Host => write!(f, "host"),
        }
    }
}

/// The game-server program.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NodeProgram {
    /// `d2rust-node` (plan N0b).
    D2rustNode {
        /// The binary (a test build with the `hostile` feature for the
        /// audit scenarios).
        bin: PathBuf,
        /// The game data folder it reads.
        game_dir: PathBuf,
    },
    /// `mp-test-stub-node`, the stand-in until then.
    Stub {
        /// The binary.
        bin: PathBuf,
    },
}

impl NodeProgram {
    /// Its name in reports.
    pub fn name(&self) -> &'static str {
        match self {
            NodeProgram::D2rustNode { .. } => "d2rust-node",
            NodeProgram::Stub { .. } => "stub",
        }
    }

    /// The binary.
    pub fn bin(&self) -> &Path {
        match self {
            NodeProgram::D2rustNode { bin, .. } | NodeProgram::Stub { bin } => bin,
        }
    }
}

/// A setup's game servers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Nodes {
    /// Their program.
    pub program: NodeProgram,
    /// How many, and how they run.
    pub setup: Setup,
}

impl Nodes {
    /// Adds the settings the main server needs for this setup to the
    /// scenario's: `d2rust-realm` writes the official servers' bundles
    /// itself (contracts 11.10), which it takes only with `dev = test`; a
    /// stand-in needs none. A key the scenario sets stays as it is.
    pub fn add_realm_settings(&self, realm: &Target, settings: &mut Vec<(String, String)>) {
        let add: Vec<(&str, String)> = match (realm, self.setup) {
            (Target::D2rustRealm { .. }, Setup::Split(_)) => vec![
                ("dev", "test".into()),
                ("bundle_dir", "bundles".into()),
                ("test_nodes", self.setup.names().join(",")),
            ],
            _ => Vec::new(),
        };
        for (k, v) in add {
            if !settings.iter().any(|(have, _)| have == k) {
                settings.push((k.into(), v));
            }
        }
    }
}

/// What a scenario asks of its setup's game servers.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NodeOpts {
    /// NAT modes by game-server name (the others are `open`).
    pub nat: Vec<(String, NatMode)>,
    /// The misbehaviour of a hostile test build (contracts 11.10).
    pub hostile: Option<String>,
    /// Held slots, in ms (the scenario's `server grace=`).
    pub grace_ms: Option<u64>,
}

/// The main server as its game servers need it.
#[derive(Clone, Debug)]
struct RealmInfo {
    /// Its certificate's SHA-256, in hex.
    cert_hex: String,
    /// Its folder (where `d2rust-realm` writes the bundles).
    dir: PathBuf,
    /// The hello a stand-in game server says to a v1 stand-in.
    game: String,
    build: String,
    protocol: u32,
    tick: u32,
}

/// One game-server process.
#[derive(Debug)]
pub struct NodeProc {
    /// `n1`.., or the hosting bot's name.
    pub name: String,
    /// Player-hosted (`--host`).
    pub host: bool,
    /// Its party in the fabric.
    pub party: usize,
    dir: PathBuf,
    child: Option<Child>,
    stdin: Option<ChildStdin>,
    output: Arc<Mutex<Vec<String>>>,
    /// Where this start's output begins.
    from: usize,
    /// Its first ready line of this start.
    pub ready: Option<ReadyLine>,
    /// How many times it was started.
    pub starts: u32,
    /// The scenario ended it (kill, stop, quit, crash): its exit is no
    /// failure.
    pub may_exit: bool,
    /// The version it says it runs (`D2RUST_TEST_VERSION`); `None`: its
    /// build's.
    pub version: Option<String>,
    /// It fetches upgrades (`update_command = test`).
    pub update: bool,
    /// How many times it was started again for a swap.
    pub swaps: u32,
}

impl NodeProc {
    /// Its process id, while it runs.
    pub fn pid(&self) -> Option<u32> {
        self.child.as_ref().map(Child::id)
    }

    /// How its process ended, once it has (waits a moment for the exit
    /// that closed its output).
    fn exit_status(&mut self) -> Option<std::process::ExitStatus> {
        let c = self.child.as_mut()?;
        for _ in 0..50 {
            match c.try_wait() {
                Ok(Some(s)) => return Some(s),
                Ok(None) => std::thread::sleep(Duration::from_millis(20)),
                Err(_) => return None,
            }
        }
        None
    }

    /// Whether its process runs.
    pub fn running(&mut self) -> bool {
        match self.child.as_mut().map(Child::try_wait) {
            Some(Ok(None)) => true,
            Some(_) => {
                self.child = None;
                self.stdin = None;
                false
            }
            None => false,
        }
    }

    /// This start's output lines, without their tags.
    fn lines(&self) -> Vec<String> {
        let out = self.output.lock().expect("node output");
        out.get(self.from..)
            .unwrap_or_default()
            .iter()
            .map(|l| {
                l.split_once("] ")
                    .map_or(l.as_str(), |(_, t)| t)
                    .to_string()
            })
            .collect()
    }

    /// Whether it says it is linked now: its last ready line, unless a
    /// `link lost` or `error` line came after it.
    pub fn linked(&self) -> bool {
        let mut linked = false;
        for l in self.lines() {
            if let Some(Ok(r)) = parse_ready(&l) {
                linked = r.linked.unwrap_or(true);
            } else if l.starts_with("link lost") || l.starts_with("error ") {
                linked = false;
            }
        }
        linked
    }

    /// How many times it linked since this start: its ready lines.
    pub fn links(&self) -> usize {
        self.lines()
            .iter()
            .filter(|l| matches!(parse_ready(l), Some(Ok(_))))
            .count()
    }

    /// The reason of its last `closed reason=` line (contracts 11.12).
    pub fn closed(&self) -> Option<String> {
        self.lines().iter().rev().find_map(|l| {
            l.strip_prefix("closed ")?
                .split_whitespace()
                .find_map(|w| w.strip_prefix("reason="))
                .map(str::to_string)
        })
    }

    /// The code of its last `refused the link: CODE` line: the main server
    /// refused it.
    pub fn refusal(&self) -> Option<String> {
        self.lines().iter().rev().find_map(|l| {
            let (_, rest) = l.split_once("refused the link: ")?;
            rest.split_whitespace().next().map(str::to_string)
        })
    }

    /// After its exit with `status`: when that was a swap (its code
    /// [`SWAP_EXIT`] and its [`SWAP_FILE`] marker), its next start runs the
    /// marker's version.
    fn take_swap(&mut self, status: std::process::ExitStatus) -> bool {
        if status.code() != Some(SWAP_EXIT) {
            return false;
        }
        let marker = self.dir.join(SWAP_FILE);
        let Ok(text) = std::fs::read_to_string(&marker) else {
            return false;
        };
        let _ = std::fs::remove_file(&marker);
        let v = text.trim();
        if v.is_empty() {
            return false;
        }
        self.version = Some(v.to_string());
        self.swaps += 1;
        true
    }

    /// Writes one line to its stdin.
    fn send_line(&mut self, line: &str) -> Result<(), String> {
        match &mut self.stdin {
            Some(s) => writeln!(s, "{line}")
                .and_then(|()| s.flush())
                .map_err(|e| format!("{}: writing to its stdin: {e}", self.name)),
            None => Err(format!("{} isn't running", self.name)),
        }
    }

    /// The last `n` lines it printed.
    pub fn output_tail(&self, n: usize) -> String {
        let out = self.output.lock().expect("node output");
        let from = out.len().saturating_sub(n);
        out[from..].join("\n")
    }
}

impl Drop for NodeProc {
    fn drop(&mut self) {
        self.stdin = None;
        if let Some(mut c) = self.child.take() {
            let _ = c.kill();
            let _ = c.wait();
        }
    }
}

/// A running setup's game servers and the fabric between them and the
/// main server (which the runner holds, as "the server").
#[derive(Debug)]
pub struct TopoRun {
    /// Every game server, in setup order.
    pub nodes: Vec<NodeProc>,
    /// The simulated internet.
    pub fabric: Fabric,
    program: NodeProgram,
    realm: RealmInfo,
    opts: NodeOpts,
}

/// The loopback address of a game server's outside sockets: its own, so
/// the main server's per-IP limits see one address per game server.
fn node_ip(i: usize) -> IpAddr {
    IpAddr::V4(Ipv4Addr::new(127, 0, 1, 1 + i as u8))
}

impl TopoRun {
    /// Starts every game server of `nodes` against the main server that
    /// printed `realm` and lives in `realm_dir`; each in its own folder
    /// under `dir`.
    pub fn start(
        nodes: &Nodes,
        realm: &Ready,
        realm_dir: &Path,
        dir: &Path,
        opts: &NodeOpts,
        seed: u64,
    ) -> Result<Self, StartError> {
        if !nodes.program.bin().exists() {
            return Err(StartError::NotServing(format!(
                "{} is {NOT_BUILT} ({})",
                nodes.program.name(),
                nodes.program.bin().display()
            )));
        }
        if nodes.setup == Setup::Host && matches!(nodes.program, NodeProgram::D2rustNode { .. }) {
            return Err(StartError::NotServing(format!(
                "the hosting bot's host token (HostStart, plan N8) is {NOT_BUILT}"
            )));
        }
        let cert = realm
            .cert_sha256
            .or_else(|| realm.cert.as_deref().map(d2_quic::fingerprint))
            .ok_or_else(|| StartError::Failed("the main server named no certificate".into()))?;
        let fabric = Fabric::start(realm.listen, seed)
            .map_err(|e| StartError::Failed(format!("the fabric: {e}")))?;
        let own = (
            d2_proto::PROTOCOL_VERSION,
            d2_sim::tick::TickRate::BASE.hz(),
        );
        let mut run = Self {
            nodes: Vec::new(),
            fabric,
            program: nodes.program.clone(),
            realm: RealmInfo {
                cert_hex: d2_quic::fingerprint_hex(&cert),
                dir: realm_dir.to_path_buf(),
                game: realm.game.clone().unwrap_or_else(|| "d2rust".into()),
                build: realm.build.clone().unwrap_or_else(|| "0.0.0".into()),
                protocol: realm.protocol.unwrap_or(own.0),
                tick: realm.tick.unwrap_or(own.1),
            },
            opts: opts.clone(),
        };
        let names = nodes.setup.names();
        for (name, _) in &opts.nat {
            if !names.contains(name) {
                return Err(StartError::Failed(format!(
                    "nat {name}: the setup's game servers are {}",
                    names.join(", ")
                )));
            }
        }
        for (i, name) in names.iter().enumerate() {
            let mode = opts
                .nat
                .iter()
                .find(|(n, _)| n == name)
                .map_or(NatMode::Open, |(_, m)| *m);
            let party = run
                .fabric
                .add_party(name, mode, node_ip(i))
                .map_err(|e| StartError::Failed(format!("the fabric for {name}: {e}")))?;
            let node_dir = dir.join(name);
            std::fs::create_dir_all(&node_dir)
                .map_err(|e| StartError::Failed(format!("{}: {e}", node_dir.display())))?;
            run.nodes.push(NodeProc {
                name: name.clone(),
                host: nodes.setup == Setup::Host,
                party,
                dir: node_dir,
                child: None,
                stdin: None,
                output: Arc::default(),
                from: 0,
                ready: None,
                starts: 0,
                may_exit: false,
                version: None,
                update: true,
                swaps: 0,
            });
            run.start_node(name)?;
        }
        Ok(run)
    }

    fn index(&self, name: &str) -> Result<usize, String> {
        self.nodes
            .iter()
            .position(|n| n.name == name)
            .ok_or_else(|| {
                let all: Vec<&str> = self.nodes.iter().map(|n| n.name.as_str()).collect();
                format!("no game server {name} (the setup has {})", all.join(", "))
            })
    }

    /// A game server by name.
    pub fn node(&self, name: &str) -> Option<&NodeProc> {
        self.nodes.iter().find(|n| n.name == name)
    }

    /// The command line of a game server (see the module docs).
    fn command(&self, n: &NodeProc) -> std::io::Result<Command> {
        let ingress = self
            .fabric
            .ingress(n.party)
            .ok_or_else(|| std::io::Error::other("no fabric address"))?;
        let mut c = Command::new(self.program.bin());
        if n.host {
            c.arg("--host");
            // The stand-in's hello for a v1 stand-in main server; a real
            // hosting process has its own (contracts 11.12).
            if let NodeProgram::Stub { .. } = &self.program {
                let r = &self.realm;
                c.arg(format!("hello_game={}", r.game))
                    .arg(format!("hello_protocol={}", r.protocol))
                    .arg(format!("hello_tick={}", r.tick));
            }
        } else {
            let cfg = n.dir.join("d2rust-node.cfg");
            let mut text = format!("listen = 127.0.0.1:0\nlogs = logs\nrealm_addr = {ingress}\n");
            match &self.program {
                NodeProgram::D2rustNode { game_dir, .. } => {
                    let bundles = self.realm.dir.join("bundles");
                    text.push_str(&format!(
                        "bundle = {}\nidentity = {}\ngame_data = {}\n",
                        bundles.join(format!("node-{}.bundle", n.name)).display(),
                        bundles.join(format!("node-{}.identity", n.name)).display(),
                        game_dir.display()
                    ));
                    if let Some(h) = &self.opts.hostile {
                        text.push_str(&format!("hostile = {h}\n"));
                    }
                    if let Some(ms) = self.opts.grace_ms {
                        text.push_str(&format!("grace_s = {}\n", ms.div_ceil(1000)));
                    }
                    // A test setup's game servers take dev commands on
                    // stdin, like its main server (contracts 10.1).
                    text.push_str("dev = test\n");
                    if n.update {
                        text.push_str("update_command = test\n");
                    }
                }
                // The stand-in has no bundle: it is told its id and the
                // main server's certificate, and the hello a v1 stand-in
                // main server wants.
                NodeProgram::Stub { .. } => {
                    let r = &self.realm;
                    text.push_str(&format!(
                        "node_id = {}\nrealm_cert_sha256 = {}\nhello_game = {}\nhello_build = {}\nhello_protocol = {}\nhello_tick = {}\n",
                        n.name, r.cert_hex, r.game, r.build, r.protocol, r.tick
                    ));
                }
            }
            std::fs::write(&cfg, text)?;
            c.arg("--config").arg(cfg);
        }
        match &n.version {
            Some(v) => c.env("D2RUST_TEST_VERSION", v),
            None => c.env_remove("D2RUST_TEST_VERSION"),
        };
        c.current_dir(&n.dir);
        Ok(c)
    }

    /// Starts one game server and waits for its ready line. One that exits
    /// for a swap first (a main server of another version told it which)
    /// starts again as that version.
    fn start_node(&mut self, name: &str) -> Result<(), StartError> {
        let i = self.index(name).map_err(StartError::Failed)?;
        if self.nodes[i].running() {
            return Err(StartError::Failed(format!("{name} is already running")));
        }
        let mut swaps = 0;
        loop {
            let rx = self.spawn_node(i)?;
            let n = &mut self.nodes[i];
            let child = n.child.as_mut().expect("just set");
            let answer = match wait_ready(child, &rx, START_TIMEOUT) {
                Ok(a) => a,
                Err(e) => {
                    let tail = n.output_tail(8);
                    return Err(StartError::Failed(format!(
                        "{name}: {e}; its output: {tail}"
                    )));
                }
            };
            match answer {
                Some(Ok(r)) => {
                    n.ready = Some(r);
                    return Ok(());
                }
                Some(Err(e)) => {
                    n.child = None;
                    return Err(StartError::Failed(format!("{name}: {e}")));
                }
                None => {
                    let status = n.exit_status();
                    n.running();
                    if swaps < START_SWAPS && status.is_some_and(|s| n.take_swap(s)) {
                        swaps += 1;
                        continue;
                    }
                    std::thread::sleep(Duration::from_millis(100));
                    let tail = n.output_tail(8);
                    return Err(if tail.contains(NOT_BUILT) {
                        StartError::NotServing(tail)
                    } else {
                        StartError::Failed(format!("{name} exited before its ready line: {tail}"))
                    });
                }
            }
        }
    }

    /// Spawns game server `i` (not running): its output is read from now
    /// on; the receiver gets its ready lines.
    fn spawn_node(
        &mut self,
        i: usize,
    ) -> Result<std::sync::mpsc::Receiver<Result<ReadyLine, String>>, StartError> {
        let name = self.nodes[i].name.clone();
        let mut cmd = self
            .command(&self.nodes[i])
            .map_err(|e| StartError::Failed(format!("{name}: writing its config: {e}")))?;
        let host_line = format!(
            "start realm={} realm_cert={} token={TEST_TOKEN} version={} port=0",
            self.fabric
                .ingress(self.nodes[i].party)
                .unwrap_or_else(|| SocketAddr::from(([127, 0, 0, 1], 0))),
            self.realm.cert_hex,
            self.realm.build
        );
        let n = &mut self.nodes[i];
        let mut child = cmd
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| {
                StartError::Failed(format!(
                    "starting {name} ({}): {e}",
                    self.program.bin().display()
                ))
            })?;
        n.starts += 1;
        n.may_exit = false;
        n.from = n.output.lock().expect("node output").len();
        let rx = read_output(&mut child, &n.output, format!("{name} start {}", n.starts));
        n.stdin = child.stdin.take();
        n.child = Some(child);
        if n.host {
            // Contracts 11.12: the token only ever on stdin.
            n.send_line(&host_line).map_err(StartError::Failed)?;
        }
        Ok(rx)
    }

    /// `start-node ID [version=V] [update=off]`: as that version (else the
    /// one it ran), with the updater unless `update=off`.
    pub fn start_one(
        &mut self,
        name: &str,
        version: Option<String>,
        update: bool,
    ) -> Result<(), String> {
        let i = self.index(name)?;
        let n = &mut self.nodes[i];
        if version.is_some() {
            n.version = version;
        }
        n.update = update;
        self.start_node(name).map_err(|e| e.to_string())
    }

    /// `kill-node ID`: the process dies at once, as a crash.
    pub fn kill(&mut self, name: &str) -> Result<(), String> {
        let i = self.index(name)?;
        let n = &mut self.nodes[i];
        n.may_exit = true;
        n.stdin = None;
        match n.child.take() {
            Some(mut c) => {
                let _ = c.kill();
                let _ = c.wait();
                Ok(())
            }
            None => Err(format!("{name} isn't running")),
        }
    }

    /// `pause-node ID` (`stop` true) and `resume-node ID`: freezes the
    /// process or lets it run on (SIGSTOP, SIGCONT; Unix only). A paused
    /// game server answers nothing, so a test can crash it between two
    /// steps of a move.
    pub fn pause(&mut self, name: &str, stop: bool) -> Result<(), String> {
        let i = self.index(name)?;
        let Some(c) = self.nodes[i].child.as_ref() else {
            return Err(format!("{name} isn't running"));
        };
        if !cfg!(unix) {
            return Err("pausing a process needs Unix signals".into());
        }
        let signal = if stop { "-STOP" } else { "-CONT" };
        match Command::new("kill")
            .arg(signal)
            .arg(c.id().to_string())
            .status()
        {
            Ok(s) if s.success() => Ok(()),
            Ok(s) => Err(format!("kill {signal}: {s}")),
            Err(e) => Err(format!("kill {signal}: {e}")),
        }
    }

    /// `stop-node ID`: a clean stop (SIGTERM where there is one).
    pub fn stop(&mut self, name: &str) -> Result<(), String> {
        let i = self.index(name)?;
        let n = &mut self.nodes[i];
        n.may_exit = true;
        n.stdin = None;
        match n.child.take() {
            Some(c) => {
                terminate(c, STOP_TIMEOUT);
                Ok(())
            }
            None => Err(format!("{name} isn't running")),
        }
    }

    /// `cut-link ID on|off`: only its link to the main server.
    pub fn cut_link(&mut self, name: &str, cut: bool) -> Result<(), String> {
        let i = self.index(name)?;
        let party = self.nodes[i].party;
        self.fabric.cut_link(party, cut);
        Ok(())
    }

    /// `host-sleep`, `host-quit`: a line to a hosting process
    /// (contracts 11.12). After `stop` it must exit by itself.
    pub fn host_line(&mut self, name: &str, line: &str) -> Result<(), String> {
        let i = self.index(name)?;
        let n = &mut self.nodes[i];
        if !n.host {
            return Err(format!("{name} is an official game server, not a host"));
        }
        if line == "stop" {
            n.may_exit = true;
        }
        n.send_line(line)
    }

    /// `host-client-crash`: the game client is gone, so the hosting
    /// process's stdin closes; it must save, close and exit by itself.
    pub fn host_client_crash(&mut self, name: &str) -> Result<(), String> {
        let i = self.index(name)?;
        let n = &mut self.nodes[i];
        if !n.host {
            return Err(format!("{name} is an official game server, not a host"));
        }
        if n.stdin.take().is_none() {
            return Err(format!("{name} isn't running"));
        }
        n.may_exit = true;
        Ok(())
    }

    /// What a `node ID` query reports.
    pub fn facts(&mut self, name: &str) -> Result<Vec<(String, String)>, String> {
        let i = self.index(name)?;
        let public = self.fabric.public(self.nodes[i].party);
        let mode = self.fabric.mode(self.nodes[i].party).unwrap_or_default();
        let cut = self.fabric.link_cut(self.nodes[i].party);
        let chars = self.char_files(i);
        let n = &mut self.nodes[i];
        let running = n.running();
        let yes = |b: bool| if b { "yes" } else { "no" }.to_string();
        let mut f = vec![
            ("running".into(), yes(running)),
            ("linked".into(), yes(running && n.linked())),
            ("links".into(), n.links().to_string()),
            ("starts".into(), n.starts.to_string()),
            ("nat".into(), mode.name().into()),
            ("cut".into(), yes(cut)),
            ("host".into(), yes(n.host)),
            ("chars".into(), chars.to_string()),
            ("swaps".into(), n.swaps.to_string()),
        ];
        if let Some(v) = &n.version {
            f.push(("version".into(), v.clone()));
        }
        if let Some(r) = n.refusal() {
            f.push(("refusal".into(), r));
        }
        if let Some(id) = n.ready.as_ref().and_then(|r| r.node.clone()) {
            f.push(("node".into(), id));
        }
        if let Some(c) = n.closed() {
            f.push(("closed".into(), c));
        }
        if let Some(p) = public {
            f.push(("public".into(), p.to_string()));
        }
        Ok(f)
    }

    /// Where game server `name` takes players: its outside address in the
    /// fabric, the one the main server sees and hands out.
    pub fn public_of(&self, name: &str) -> Result<SocketAddr, String> {
        let i = self.index(name)?;
        self.fabric
            .public(self.nodes[i].party)
            .ok_or_else(|| format!("{name} has no outside address yet"))
    }

    /// Sends a dev line (`dev CHARACTER COMMAND`) to every running
    /// official game server: the one whose game holds the character
    /// answers `dev ok`; the others say `dev err`. The first `dev ok`'s
    /// text, else the first `dev err`.
    pub fn dev_line(&mut self, line: &str) -> Result<String, String> {
        let mut asked = Vec::new();
        for n in &mut self.nodes {
            if n.host || !n.running() {
                continue;
            }
            let from = n.output.lock().expect("node output").len();
            if n.send_line(line).is_ok() {
                asked.push((n.name.clone(), from));
            }
        }
        if asked.is_empty() {
            return Err("no game server is running".into());
        }
        let deadline = std::time::Instant::now() + DEV_WAIT;
        loop {
            let mut errs = Vec::new();
            for (name, from) in &asked {
                let n = self.nodes.iter().find(|n| &n.name == name).expect("asked");
                let out = n.output.lock().expect("node output");
                for l in out.get(*from..).unwrap_or_default() {
                    if let Some(i) = l.find("dev ok") {
                        return Ok(l[i + "dev ok".len()..].trim().to_string());
                    }
                    if let Some(i) = l.find("dev err") {
                        errs.push(format!("{name}: {}", &l[i..]));
                        break;
                    }
                }
            }
            if errs.len() == asked.len() {
                return Err(errs.join("; "));
            }
            if std::time::Instant::now() >= deadline {
                return Err(format!("no dev answer to {line:?} in time"));
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    /// The character files (`.d2s`) in game server `name`'s folder: an
    /// official game server keeps none (the main server stores them).
    fn char_files(&self, i: usize) -> usize {
        fn count(dir: &Path) -> usize {
            let Ok(rd) = std::fs::read_dir(dir) else {
                return 0;
            };
            rd.flatten()
                .map(|e| {
                    let p = e.path();
                    if p.is_dir() {
                        count(&p)
                    } else {
                        usize::from(p.extension().is_some_and(|x| x.eq_ignore_ascii_case("d2s")))
                    }
                })
                .sum()
        }
        count(&self.nodes[i].dir)
    }

    /// The main server moved (a restart): the fabric follows it.
    pub fn realm_moved(&self, listen: SocketAddr) {
        self.fabric.retarget_realm(listen);
    }

    /// Game servers that exited by themselves while they should run:
    /// (name, how). Each is reported once. One that exited for a swap is
    /// started again as the staged version (it links once the main server
    /// runs it too).
    pub fn exited(&mut self) -> Vec<(String, String)> {
        let mut out = Vec::new();
        let mut swapped = Vec::new();
        for (i, n) in self.nodes.iter_mut().enumerate() {
            if n.may_exit {
                continue;
            }
            if let Some(c) = n.child.as_mut() {
                if let Ok(Some(s)) = c.try_wait() {
                    n.child = None;
                    n.stdin = None;
                    if !n.host && n.take_swap(s) {
                        swapped.push(i);
                        continue;
                    }
                    n.may_exit = true;
                    out.push((n.name.clone(), s.to_string()));
                }
            }
        }
        for i in swapped {
            if let Err(e) = self.spawn_node(i) {
                self.nodes[i].may_exit = true;
                out.push((
                    self.nodes[i].name.clone(),
                    format!("its start after a swap: {e}"),
                ));
            }
        }
        out
    }

    /// Every line a game server printed that reports a panic.
    pub fn panics(&self) -> Vec<String> {
        self.nodes
            .iter()
            .flat_map(|n| {
                n.output
                    .lock()
                    .expect("node output")
                    .iter()
                    .filter(|l| l.contains("panicked"))
                    .cloned()
                    .collect::<Vec<_>>()
            })
            .collect()
    }

    /// The capabilities the game servers announce, joined: an official
    /// one's `caps=`, and `hosting` once a hosting process is ready.
    pub fn caps(&self) -> BTreeSet<String> {
        let mut caps = BTreeSet::from(["setup".to_string()]);
        for n in &self.nodes {
            if let Some(r) = &n.ready {
                caps.extend(r.caps.iter().flatten().cloned());
                if n.host {
                    caps.insert("hosting".into());
                }
            }
        }
        caps
    }

    /// The last lines of every game server, for a report.
    pub fn output_tail(&self, n: usize) -> String {
        self.nodes
            .iter()
            .map(|p| p.output_tail(n))
            .filter(|t| !t.is_empty())
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Stops every game server cleanly (hosts with `stop`).
    pub fn stop_all(&mut self) {
        let names: Vec<String> = self.nodes.iter().map(|n| n.name.clone()).collect();
        for name in names {
            let Ok(i) = self.index(&name) else { continue };
            if self.nodes[i].host && self.nodes[i].running() {
                let _ = self.host_line(&name, "stop");
                let n = &mut self.nodes[i];
                n.stdin = None;
                if let Some(c) = n.child.take() {
                    terminate(c, HOST_EXIT);
                }
            } else {
                let _ = self.stop(&name);
            }
        }
    }
}
