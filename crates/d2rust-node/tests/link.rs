//! Both programs as processes on loopback (plan N0b): a main server
//! (`d2rust-realm`) with a test game server's bundle, and game servers
//! (`d2rust-node`) linking to it.
//!
//! - an official game server with its bundle links and prints its ready
//!   line, the main server's `logs/nodes.json` shows it linked, and no key
//!   bytes appear in any log or output;
//! - a killed game server is lost within 10 s;
//! - the stop marker `run/stop` stops a game server: it exits 0;
//! - a foreign certificate is refused (`NodeUnknown`), other game data
//!   (`NodeData`), another version (`NodeBuild`, shown as refused with its
//!   version in `nodes.json`) and another mod realm (`NodeGame`);
//! - a save for a seat without a live lease is refused (`LeaseRevoked`,
//!   plan N2);
//! - a game server whose standard output is closed keeps running;
//! - a game server checks a player's join ticket with the keys of its
//!   welcome, and refuses one that isn't the main server's
//!   (`TicketInvalid`, plan N3).
//!
//! The main server's binary is the one beside this package's
//! (`target/<profile>/d2rust-realm`); each test process builds it first
//! (nothing to do when it is up to date). The game data is a small made-up folder: the link
//! only hashes it.

use std::io::{BufRead, BufReader};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use d2_link::streams::{self, What};
use d2_link::{Bundle, LinkRx, LinkTx, Versions, HEARTBEAT, LOST_AFTER};
use d2_net::msg::account::CharSummary;
use d2_net::msg::node::{NodeToRealm, RealmToNode, SaveInfo};
use d2_net::msg::ErrorCode;
use d2_net::wire::{Attach, Refused, ResumeToken, Timing};
use d2_quic::{Identity, Listen, QuicClient, QuicConn, QuicServer, ServerTrust, NODE_ALPN};

/// How long a program has to print its ready line (debug builds, a busy
/// test machine).
const READY_WAIT: Duration = Duration::from_secs(60);

fn node_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_d2rust-node"))
}

/// The main server's binary beside this one, built (or found up to date)
/// once per test process: a test run of this package doesn't build
/// another package's binary, and an old one would test old code.
fn realm_bin() -> PathBuf {
    static BUILT: OnceLock<Mutex<bool>> = OnceLock::new();
    let node = node_bin();
    let bin = node.with_file_name(format!("d2rust-realm{}", std::env::consts::EXE_SUFFIX));
    let mut built = BUILT
        .get_or_init(|| Mutex::new(false))
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    if !*built {
        let profile_dir = node.parent().expect("the profile folder");
        let target = profile_dir.parent().expect("the target folder");
        let mut cmd = Command::new(env!("CARGO"));
        cmd.args([
            "build",
            "--locked",
            "-p",
            "d2rust-realm",
            "--bin",
            "d2rust-realm",
        ])
        .arg("--target-dir")
        .arg(target);
        if profile_dir.file_name().is_some_and(|p| p == "release") {
            cmd.arg("--release");
        }
        let status = cmd.status().expect("cargo runs");
        assert!(status.success(), "building d2rust-realm failed");
        *built = true;
    }
    bin
}

fn temp_dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("d2rust-node-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// A made-up game-data folder.
fn game_data(dir: &Path, tag: &str) -> PathBuf {
    let g = dir.join(format!("game-{tag}"));
    std::fs::create_dir_all(g.join("data/global")).unwrap();
    std::fs::write(g.join("data/global/a.txt"), "rows").unwrap();
    std::fs::write(g.join("d2data.mpq"), format!("archive {tag}")).unwrap();
    g
}

/// A running program, its standard output read line by line.
struct Proc {
    child: Child,
    lines: mpsc::Receiver<String>,
    seen: Vec<String>,
}

impl Proc {
    fn start(bin: &Path, dir: &Path) -> Self {
        let mut child = Command::new(bin)
            .arg("--config")
            .arg("program.cfg")
            .current_dir(dir)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let (tx, lines) = mpsc::channel();
        let out = child.stdout.take().unwrap();
        std::thread::spawn(move || {
            for line in BufReader::new(out).lines().map_while(Result::ok) {
                if tx.send(line).is_err() {
                    break;
                }
            }
        });
        Proc {
            child,
            lines,
            seen: Vec::new(),
        }
    }

    /// The first line starting with `prefix`, waiting at most `wait`.
    fn line(&mut self, prefix: &str, wait: Duration) -> Option<String> {
        let deadline = Instant::now() + wait;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            match self.lines.recv_timeout(left) {
                Ok(l) => {
                    self.seen.push(l.clone());
                    if l.starts_with(prefix) {
                        return Some(l);
                    }
                }
                Err(_) => return None,
            }
        }
    }

    /// Waits for the program to end by itself; its exit success and
    /// standard error.
    fn finish(mut self, wait: Duration) -> (bool, String) {
        let deadline = Instant::now() + wait;
        loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                let mut err = String::new();
                if let Some(mut e) = self.child.stderr.take() {
                    std::io::Read::read_to_string(&mut e, &mut err).unwrap();
                }
                return (status.success(), err);
            }
            assert!(Instant::now() < deadline, "the program didn't end");
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    fn kill(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }

    /// Kills the program and keeps the rest of its output.
    fn stop(&mut self) {
        self.kill();
        while let Ok(l) = self.lines.recv_timeout(Duration::from_secs(5)) {
            self.seen.push(l);
        }
    }
}

impl Drop for Proc {
    fn drop(&mut self) {
        self.kill();
    }
}

/// One `key=value` of a ready line.
fn field<'a>(line: &'a str, key: &str) -> &'a str {
    line.split(' ')
        .find_map(|kv| kv.strip_prefix(key)?.strip_prefix('='))
        .unwrap_or_else(|| panic!("no {key} in {line}"))
}

/// A main server in `dir/realm` with test game server `n1`, and its ready
/// line.
fn realm(dir: &Path, game: &Path) -> (Proc, String) {
    let r = dir.join("realm");
    std::fs::create_dir_all(&r).unwrap();
    std::fs::write(
        r.join("program.cfg"),
        format!(
            "listen = 127.0.0.1:0\ngame_data = {}\ndev = test\nbundle_dir = bundles\ntest_nodes = n1\n\
             save_checks = off\nitem_registry = off\n",
            game.display()
        ),
    )
    .unwrap();
    let mut p = Proc::start(&realm_bin(), &r);
    let ready = p
        .line("ready ", READY_WAIT)
        .expect("the main server's ready line");
    (p, ready)
}

/// A game server in `dir/name` with `n1`'s bundle, the identity `identity`
/// and the game data `game`.
fn node(dir: &Path, name: &str, identity: &Path, game: &Path) -> Proc {
    Proc::start(&node_bin(), &node_folder(dir, name, identity, game))
}

/// The folder and config of [`node`]'s game server.
fn node_folder(dir: &Path, name: &str, identity: &Path, game: &Path) -> PathBuf {
    let n = dir.join(name);
    std::fs::create_dir_all(&n).unwrap();
    std::fs::write(
        n.join("program.cfg"),
        format!(
            "bundle = {}\nidentity = {}\nlisten = 127.0.0.1:0\ngame_data = {}\n",
            dir.join("realm/bundles/node-n1.bundle").display(),
            identity.display(),
            game.display()
        ),
    )
    .unwrap();
    n
}

/// Game server `id`'s line in the main server's `logs/nodes.json` once it
/// has `text`, waiting at most `wait`.
fn nodes_line(dir: &Path, id: &str, text: &str, wait: Duration) -> String {
    let path = dir.join("realm/logs/nodes.json");
    let start = Instant::now();
    loop {
        let json = std::fs::read_to_string(&path).unwrap_or_default();
        let line = json
            .lines()
            .find(|l| l.starts_with(&format!("{{\"id\": \"{id}\"")))
            .map(str::to_owned);
        if let Some(l) = line.filter(|l| l.contains(text)) {
            return l;
        }
        assert!(
            start.elapsed() < wait,
            "no {text} for {id} in nodes.json:\n{json}"
        );
        std::thread::sleep(Duration::from_millis(200));
    }
}

fn n1_identity(dir: &Path) -> PathBuf {
    dir.join("realm/bundles/node-n1.identity")
}

fn realm_log(dir: &Path) -> String {
    std::fs::read_to_string(dir.join("realm/logs/d2rust-realm.log")).unwrap_or_default()
}

/// Waits until the main server's log has `text`; how long it took.
fn wait_log(dir: &Path, text: &str, wait: Duration) -> Option<Duration> {
    let start = Instant::now();
    while start.elapsed() < wait {
        if realm_log(dir).contains(text) {
            return Some(start.elapsed());
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    None
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn base64(b: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut s = String::new();
    for c in b.chunks(3) {
        let n = c
            .iter()
            .enumerate()
            .fold(0u32, |n, (i, &x)| n | u32::from(x) << (16 - 8 * i));
        for i in 0..=c.len() {
            s.push(T[(n >> (18 - 6 * i) & 63) as usize] as char);
        }
    }
    s
}

/// Whether `text` holds any 16 bytes of `key`: raw, as hex or as base64.
fn leaks(text: &[u8], key: &[u8]) -> bool {
    let lower = String::from_utf8_lossy(text).to_ascii_lowercase();
    let raw = String::from_utf8_lossy(text);
    key.windows(16).step_by(4).any(|w| {
        text.windows(16).any(|t| t == w)
            || lower.contains(&hex(w))
            || (0..3).any(|skip| {
                let b = base64(&w[skip..]);
                raw.contains(&b[..b.len() - 4])
            })
    })
}

/// Every file below `dir`, read.
fn all_files(dir: &Path, out: &mut Vec<(PathBuf, Vec<u8>)>) {
    for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let p = e.path();
        if p.is_dir() {
            all_files(&p, out);
        } else {
            out.push((p.clone(), std::fs::read(&p).unwrap()));
        }
    }
}

/// An official game server with its bundle links and prints its ready
/// line; the main server logs it. No key bytes appear in any log or
/// output of either.
#[test]
fn an_official_game_server_links_and_logs_no_key() {
    let dir = temp_dir("links");
    let game = game_data(&dir, "a");
    let (mut realm, realm_ready) = realm(&dir, &game);
    assert_eq!(field(&realm_ready, "program"), "realm");
    assert!(field(&realm_ready, "caps").split(',').any(|c| c == "nodes"));
    let realm_addr = field(&realm_ready, "listen").to_owned();
    let mut n = node(&dir, "n1", &n1_identity(&dir), &game);
    let ready = n
        .line("ready ", READY_WAIT)
        .unwrap_or_else(|| panic!("no ready line; realm log:\n{}", realm_log(&dir)));
    assert_eq!(field(&ready, "program"), "node");
    assert_eq!(field(&ready, "node"), "n1");
    assert_eq!(field(&ready, "linked"), "1");
    assert_eq!(field(&ready, "realm"), realm_addr);
    let node_addr: SocketAddr = field(&ready, "listen").parse().unwrap();
    assert_ne!(node_addr.port(), 0);
    let cert = field(&ready, "cert_sha256");
    assert_eq!(cert.len(), 64);
    assert!(
        wait_log(
            &dir,
            &format!("node n1 linked from 127.0.0.1:{}", node_addr.port()),
            READY_WAIT
        )
        .is_some(),
        "{}",
        realm_log(&dir)
    );
    // The list names the certificate the game server linked with.
    let list = std::fs::read_to_string(dir.join("realm/data/nodes.toml")).unwrap();
    assert!(list.contains(cert), "{list}");
    // A few heartbeats, then both stop.
    std::thread::sleep(Duration::from_secs(3));
    let line = nodes_line(&dir, "n1", "\"state\": \"linked\"", READY_WAIT);
    assert!(
        line.contains(&format!(
            "\"version\": \"{}\"",
            field(&realm_ready, "build")
        )),
        "{line}"
    );
    assert!(
        line.contains(&format!("\"addr\": \"127.0.0.1:{}\"", node_addr.port())),
        "{line}"
    );
    n.stop();
    realm.stop();
    // No key bytes in any log, status file or output.
    let mut keys = Vec::new();
    for id in [dir.join("realm/identity"), n1_identity(&dir)] {
        keys.push(Identity::load(&id).unwrap().key);
    }
    keys.push(std::fs::read(dir.join("realm/identity/ticket-1.key")).unwrap());
    let mut texts = Vec::new();
    all_files(&dir.join("realm/logs"), &mut texts);
    all_files(&dir.join("n1/logs"), &mut texts);
    texts.push((dir.join("realm stdout"), realm.seen.join("\n").into_bytes()));
    texts.push((dir.join("node stdout"), n.seen.join("\n").into_bytes()));
    assert!(texts.len() >= 4, "{texts:?}");
    for (path, text) in &texts {
        for key in &keys {
            assert!(!leaks(text, key), "key bytes in {}", path.display());
        }
    }
    // The check itself finds a key.
    let planted = format!("x {} y", hex(&keys[0][20..60]));
    assert!(leaks(planted.as_bytes(), &keys[0]));
    let planted = format!("x {} y", base64(&keys[1][9..70]));
    assert!(leaks(planted.as_bytes(), &keys[1]));
    let _ = std::fs::remove_dir_all(&dir);
}

/// A killed game server sends no more `Load`: the main server counts it
/// lost 10 s after the last one ([`LOST_AFTER`]). Killed between two
/// heartbeats, it is lost within 10 s of the kill, and not before 10 s
/// without a `Load` (the last left at most [`HEARTBEAT`] before the kill).
#[test]
fn a_killed_game_server_is_lost_within_10_s() {
    let dir = temp_dir("killed");
    let game = game_data(&dir, "a");
    let (_realm, _) = realm(&dir, &game);
    let mut n = node(&dir, "n1", &n1_identity(&dir), &game);
    n.line("ready ", READY_WAIT).expect("linked");
    assert!(wait_log(&dir, "node n1 linked", READY_WAIT).is_some());
    // The first Load leaves with the ready line, the next 2 s later.
    std::thread::sleep(Duration::from_millis(1000));
    n.kill();
    let took = wait_log(&dir, "node n1 lost", Duration::from_secs(20));
    let took = took.unwrap_or_else(|| panic!("not lost; log:\n{}", realm_log(&dir)));
    assert!(took <= LOST_AFTER, "lost {took:?} after the kill");
    assert!(
        took + Duration::from_millis(500) >= LOST_AFTER - HEARTBEAT,
        "lost {took:?} after the kill: before 10 s without a Load"
    );
    let log = realm_log(&dir);
    assert!(log.contains("node n1 lost: no heartbeat in 10 s"), "{log}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// The stop marker in its folder (`node-ctl.ps1 stop` on Windows, where
/// there is no signal to send) stops a linked game server: it saves,
/// exits 0 and removes the marker.
#[test]
fn the_stop_marker_stops_a_game_server() {
    let dir = temp_dir("stop-marker");
    let game = game_data(&dir, "a");
    let (_realm, _) = realm(&dir, &game);
    let mut n = node(&dir, "n1", &n1_identity(&dir), &game);
    n.line("ready ", READY_WAIT).expect("linked");
    let run = dir.join("n1/run");
    std::fs::create_dir_all(&run).unwrap();
    std::fs::write(run.join("stop"), "stop\n").unwrap();
    let (ok, err) = n.finish(Duration::from_secs(20));
    assert!(ok, "exit 0; stderr: {err}");
    assert!(!run.join("stop").exists(), "the marker is removed");
    let log = std::fs::read_to_string(dir.join("n1/logs/d2rust-node.log")).unwrap();
    assert!(log.contains("stopping: stop marker"), "{log}");
    assert!(log.contains("stopped"), "{log}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// A game server with a certificate the main server doesn't list is
/// refused (`NodeUnknown`), one with other game data `NodeData`; each
/// exits 1 naming the code.
#[test]
fn a_foreign_certificate_and_other_game_data_are_refused() {
    let dir = temp_dir("refused");
    let game = game_data(&dir, "a");
    let other_game = game_data(&dir, "b");
    let (_realm, _) = realm(&dir, &game);
    let foreign = node(&dir, "foreign", &dir.join("foreign/identity"), &game);
    let (ok, err) = foreign.finish(READY_WAIT);
    assert!(!ok);
    assert!(err.contains("NodeUnknown (100)"), "{err}");
    let data = node(&dir, "data", &n1_identity(&dir), &other_game);
    let (ok, err) = data.finish(READY_WAIT);
    assert!(!ok);
    assert!(err.contains("NodeData (102)"), "{err}");
    let log = realm_log(&dir);
    assert!(log.contains("refused: NodeUnknown (100)"), "{log}");
    assert!(log.contains("refused: NodeData (102)"), "{log}");
    assert!(!log.contains("node n1 linked"), "{log}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// A game server of another version (`NodeBuild`) or mod realm
/// (`NodeGame`), with the right certificate, is refused. The test speaks
/// the link itself: one build can't be another version.
#[test]
fn another_version_or_mod_realm_is_refused() {
    let dir = temp_dir("version");
    let game = game_data(&dir, "a");
    let (_realm, ready) = realm(&dir, &game);
    let n1 = FakeN1::new(&dir, &game, &ready);
    // The answer, and the catch-up ahead of it, if any.
    let answers = |v: &Versions| {
        n1.rt.block_on(async {
            let (_conn, _tx, mut rx) = n1.link(v).await;
            let first = rx.recv().await.unwrap();
            match first {
                RealmToNode::Update { .. } => (Some(first), rx.recv().await.unwrap()),
                _ => (None, first),
            }
        })
    };
    let answer = |v: &Versions| answers(v).1;
    let code = |v: Versions| match answers(&v) {
        (None, RealmToNode::Refused { code, .. }) => code,
        other => panic!("{other:?}"),
    };
    let ours = n1.ours.clone();
    // Another version: the main server's version to catch up with, then
    // the refusal.
    match answers(&Versions {
        version: "0.0.0.1".into(),
        ..ours.clone()
    }) {
        (Some(RealmToNode::Update { phase, version, .. }), RealmToNode::Refused { code, text }) => {
            assert_eq!(phase, d2_net::msg::node::update_phase::CATCH_UP);
            assert_eq!(version, ours.version);
            assert_eq!(code, ErrorCode::NODE_BUILD);
            assert!(text.contains("0.0.0.1"), "{text}");
        }
        other => panic!("{other:?}"),
    }
    nodes_line(
        &dir,
        "n1",
        "\"state\": \"refused\", \"version\": \"0.0.0.1\"",
        Duration::from_secs(10),
    );
    assert_eq!(
        code(Versions {
            protocol: ours.protocol + 1,
            ..ours.clone()
        }),
        ErrorCode::NODE_BUILD
    );
    assert_eq!(
        code(Versions {
            game: "mod-x".into(),
            ..ours.clone()
        }),
        ErrorCode::NODE_GAME
    );
    // The same hello with this build's versions links.
    assert!(matches!(answer(&ours), RealmToNode::Welcome(_)));
    let _ = std::fs::remove_dir_all(&dir);
}

/// Game server n1 played by the test: its bundle and identity, on its own
/// endpoint, with this build's versions.
struct FakeN1 {
    rt: tokio::runtime::Runtime,
    ep: QuicServer,
    bundle: Bundle,
    identity: Identity,
    ours: Versions,
}

impl FakeN1 {
    fn new(dir: &Path, game: &Path, ready: &str) -> Self {
        let bundle = Bundle::load(&dir.join("realm/bundles/node-n1.bundle")).unwrap();
        let identity = Identity::load(&n1_identity(dir)).unwrap();
        let ours = Versions {
            version: field(ready, "build").to_owned(),
            protocol: field(ready, "protocol").parse().unwrap(),
            tick_rate: field(ready, "tick").parse().unwrap(),
            game: field(ready, "game").to_owned(),
            data_hash: d2_service::datahash::game_data_hash(game, None).unwrap(),
        };
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .unwrap();
        let ep = QuicServer::bind_with(
            "127.0.0.1:0".parse().unwrap(),
            identity.clone(),
            Timing::default(),
            rt.handle().clone(),
            Listen::Players,
        )
        .unwrap();
        Self {
            rt,
            ep,
            bundle,
            identity,
            ours,
        }
    }

    /// Links to the main server and says hello with `v`.
    async fn link(&self, v: &Versions) -> (QuicConn, LinkTx<NodeToRealm>, LinkRx<RealmToNode>) {
        let (conn, control) = self
            .ep
            .connect(
                self.bundle.realm_addr,
                ServerTrust::Pinned(self.bundle.realm_cert),
                NODE_ALPN,
                &self.identity,
                Timing::default(),
            )
            .await
            .unwrap();
        let (tx, rx) = d2_link::start::<NodeToRealm, RealmToNode>(control);
        tx.send_msg(&NodeToRealm::Hello(v.hello("n1", 6113, 1, 8)))
            .unwrap();
        (conn, tx, rx)
    }
}

/// A linked game server's save for a seat it holds no lease on (none was
/// granted, or it ended) is answered `LeaseRevoked` and logged; the main
/// server writes nothing (plan N2).
#[test]
fn a_save_without_a_lease_is_refused() {
    let dir = temp_dir("lease");
    let game = game_data(&dir, "a");
    let (_realm, ready) = realm(&dir, &game);
    let n1 = FakeN1::new(&dir, &game, &ready);
    let ack = n1.rt.block_on(async {
        let (conn, tx, mut rx) = n1.link(&n1.ours).await;
        assert!(matches!(rx.recv().await.unwrap(), RealmToNode::Welcome(_)));
        let summary = CharSummary {
            name: "Kashya".into(),
            class: 3,
            level: 99,
            status: 0,
            difficulties: 7,
            look: Vec::new(),
            last_played: 0,
            season: 0,
        };
        tx.send_msg(&NodeToRealm::Save(SaveInfo {
            game: 1,
            lease: 42,
            seq: 1,
            reason: 0,
            journal_tick: 0,
            summary,
        }))
        .unwrap();
        streams::send(&conn, What::Record { lease: 42 }, 1, &[0; 64])
            .await
            .unwrap();
        loop {
            match rx.recv().await {
                Ok(RealmToNode::SaveAck { lease, seq, code }) => break (lease, seq, code),
                Ok(_) => continue,
                Err(e) => panic!(
                    "no SaveAck ({e}); the main server's log:
{}",
                    realm_log(&dir)
                ),
            }
        }
    });
    assert_eq!(ack, (42, 1, ErrorCode::LEASE_REVOKED));
    assert!(
        wait_log(&dir, "refused: LeaseRevoked", Duration::from_secs(5)).is_some(),
        "{}",
        realm_log(&dir)
    );
    assert!(
        !dir.join("realm/data/characters").exists()
            || all_names(&dir.join("realm/data/characters")).is_empty(),
        "no character file was written"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// Every file name under `dir`.
fn all_names(dir: &Path) -> Vec<String> {
    let mut out = Vec::new();
    for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let p = e.path();
        if p.is_dir() {
            out.extend(all_names(&p));
        } else {
            out.push(p.display().to_string());
        }
    }
    out
}

/// A game server whose standard output nobody reads any more (the test
/// tool or the client that started it went away) keeps running: the lost
/// link's line is dropped, not a panic.
#[test]
fn a_closed_standard_output_does_not_stop_a_game_server() {
    let dir = temp_dir("stdout");
    let game = game_data(&dir, "a");
    let (mut realm, _) = realm(&dir, &game);
    let n = node_folder(&dir, "n1", &n1_identity(&dir), &game);
    let mut child = Command::new(node_bin())
        .arg("--config")
        .arg("program.cfg")
        .current_dir(&n)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    {
        let mut out = BufReader::new(child.stdout.take().unwrap());
        let mut line = String::new();
        while !line.starts_with("ready ") {
            line.clear();
            assert!(out.read_line(&mut line).unwrap() > 0, "no ready line");
        }
        // The reader goes away.
    }
    realm.kill();
    let log = n.join("logs/d2rust-node.log");
    let until = Instant::now() + LOST_AFTER + Duration::from_secs(10);
    while !std::fs::read_to_string(&log)
        .unwrap_or_default()
        .contains("again in")
    {
        assert!(
            Instant::now() < until,
            "the game server never retried: {}",
            std::fs::read_to_string(&log).unwrap_or_default()
        );
        assert!(
            child.try_wait().unwrap().is_none(),
            "the game server stopped writing to a closed output"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
    std::thread::sleep(Duration::from_millis(300));
    assert!(
        child.try_wait().unwrap().is_none(),
        "the game server stopped writing to a closed output"
    );
    let _ = child.kill();
    let _ = child.wait();
    let _ = std::fs::remove_dir_all(&dir);
}

/// The game server's log.
fn node_log(dir: &Path, name: &str) -> String {
    std::fs::read_to_string(dir.join(name).join("logs/d2rust-node.log")).unwrap_or_default()
}

/// The main server made ticket key 1 and gave its public half to the game
/// server (its bundle and its welcome). A player's attach is checked with
/// it: a ticket the main server didn't sign is refused with
/// `TicketInvalid`, the dedicated server's attach with `Malformed`, a
/// resume with `UnknownResume`. A ticket is logged only by its first two
/// bytes (plan N3).
#[test]
fn a_game_server_checks_join_tickets() {
    let dir = temp_dir("tickets");
    let game = game_data(&dir, "a");
    let (_realm, _) = realm(&dir, &game);
    let mut n = node(&dir, "n1", &n1_identity(&dir), &game);
    let ready = n
        .line("ready ", READY_WAIT)
        .unwrap_or_else(|| panic!("no ready line; realm log:\n{}", realm_log(&dir)));
    assert!(
        realm_log(&dir).contains("join tickets signed with key 1 (made now)"),
        "{}",
        realm_log(&dir)
    );
    let key = d2_tickets::TicketKey::load(&dir.join("realm/identity"), 1)
        .unwrap()
        .public();
    let bundle = Bundle::load(&dir.join("realm/bundles/node-n1.bundle")).unwrap();
    assert_eq!(
        bundle.ticket_keys,
        [d2_net::msg::node::TicketKey {
            key_id: 1,
            public: key.bytes
        }]
    );
    let until = Instant::now() + READY_WAIT;
    while !node_log(&dir, "n1").contains("join tickets: key ids [1]") {
        assert!(Instant::now() < until, "{}", node_log(&dir, "n1"));
        std::thread::sleep(Duration::from_millis(50));
    }
    let addr: SocketAddr = field(&ready, "listen").parse().unwrap();
    let cert = d2_quic::parse_fingerprint(field(&ready, "cert_sha256")).unwrap();
    let answer = |a: Attach| {
        let (_client, mut stream) =
            QuicClient::connect(addr, ServerTrust::Pinned(cert), Timing::default()).unwrap();
        stream.send(a.body()).unwrap();
        let body = stream
            .recv_timeout(Duration::from_secs(10))
            .unwrap()
            .expect("an answer to the attach");
        Refused::parse(&body).unwrap().code
    };
    let mut forged = vec![1, 1];
    forged.extend([0xC3; 150]);
    assert_eq!(answer(Attach::Node(forged)), ErrorCode::TICKET_INVALID);
    assert_eq!(answer(Attach::Ticket([7; 16])), ErrorCode::MALFORMED);
    assert_eq!(
        answer(Attach::Resume(ResumeToken([7; 16]))),
        ErrorCode::UNKNOWN_RESUME
    );
    let log = node_log(&dir, "n1");
    assert!(
        log.contains("ticket 01:01.. refused: the signature doesn't match"),
        "{log}"
    );
    assert!(!log.contains("c3c3") && !log.contains("C3C3"), "{log}");
    let _ = std::fs::remove_dir_all(&dir);
}
