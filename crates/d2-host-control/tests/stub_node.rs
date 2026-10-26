//! `HostProcess` against real processes on loopback: the MP test tool's
//! stand-in main server (`mp-test-synth`) and its stand-in game server
//! (`mp-test-stub-node --host`, which speaks the lines of contracts 11.12),
//! each its own process, the game server started by `HostProcess` as the
//! game client starts it.
//!
//! The two programs are the `mp-test` package's binaries. A workspace test
//! run has built them; a run of this package alone builds them first (or
//! set `MP_TEST_BIN_DIR` to the folder that holds them).

use std::io::{BufRead, BufReader};
use std::net::{SocketAddr, UdpSocket};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use d2_host_control::{
    parse_hex32, ExitKind, HostConfig, HostEvent, HostProcess, HostToken, Phase, ReachHow,
    ReachView, StartLine,
};

/// The token every test host gets: never seen in a command line.
const TOKEN: [u8; 16] = *b"\x5a\xa5test-token-hc\x01";
const WAIT: Duration = Duration::from_secs(20);

fn token_hex() -> String {
    TOKEN.iter().map(|b| format!("{b:02x}")).collect()
}

/// The folder of this run's binaries (`target/<profile>`).
fn bin_dir() -> PathBuf {
    if let Some(d) = std::env::var_os("MP_TEST_BIN_DIR") {
        return PathBuf::from(d);
    }
    let exe = std::env::current_exe().expect("test exe");
    // target/<profile>/deps/<this test>
    exe.parent()
        .and_then(Path::parent)
        .expect("target/<profile>")
        .to_path_buf()
}

/// An `mp-test` binary, built first when this package's tests run alone.
fn mp_bin(name: &str) -> PathBuf {
    static BUILT: OnceLock<()> = OnceLock::new();
    let dir = bin_dir();
    let path = dir.join(format!("{name}{}", std::env::consts::EXE_SUFFIX));
    if !path.exists() {
        BUILT.get_or_init(|| {
            let profile = dir.file_name().and_then(|n| n.to_str()).unwrap_or("debug");
            let mut c = Command::new(option_env!("CARGO").unwrap_or("cargo"));
            c.current_dir(env!("CARGO_MANIFEST_DIR"))
                .args(["build", "-q", "-p", "mp-test"])
                .args(["--bin", "mp-test-stub-node", "--bin", "mp-test-synth"])
                .arg("--target-dir")
                .arg(dir.parent().expect("target dir"));
            match profile {
                "debug" => {}
                "release" => {
                    c.arg("--release");
                }
                p => {
                    c.args(["--profile", p]);
                }
            }
            let ok = c.status().map(|s| s.success()).unwrap_or(false);
            assert!(ok, "building the mp-test binaries failed");
        });
    }
    assert!(path.exists(), "{} is missing", path.display());
    path
}

fn work(name: &str) -> PathBuf {
    let d = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("host-control-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).expect("work folder");
    d
}

/// What the stand-in main server's ready line said.
struct RealmInfo {
    addr: SocketAddr,
    cert: String,
    game: String,
    build: String,
    protocol: String,
    tick: String,
}

/// The stand-in main server, its own process.
struct Realm {
    child: Child,
    _stdin: ChildStdin,
    info: RealmInfo,
}

impl std::ops::Deref for Realm {
    type Target = RealmInfo;
    fn deref(&self) -> &RealmInfo {
        &self.info
    }
}

impl Realm {
    fn start(name: &str) -> Realm {
        let dir = work(&format!("{name}-realm"));
        let mut child = Command::new(mp_bin("mp-test-synth"))
            .args(["--listen", "127.0.0.1:0", "--data"])
            .arg(dir.join("data"))
            // It stops at the end of its stdin: held open while it serves.
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("starting mp-test-synth");
        let stdin = child.stdin.take().expect("stdin");
        let out = child.stdout.take().expect("stdout");
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            // Reads its ready line, then keeps its pipe drained.
            for l in BufReader::new(out).lines().map_while(Result::ok) {
                if l.starts_with("ready ") {
                    let _ = tx.send(l);
                }
            }
        });
        let ready = rx.recv_timeout(WAIT).expect("the main server's ready line");
        let get = |k: &str| {
            ready
                .split_whitespace()
                .find_map(|w| w.strip_prefix(&format!("{k}=")).map(str::to_string))
                .unwrap_or_else(|| panic!("no {k} in {ready}"))
        };
        Realm {
            info: RealmInfo {
                addr: get("listen").parse().expect("listen"),
                // A path to its certificate (DER): pinned by its SHA-256.
                cert: {
                    let der = std::fs::read(get("cert")).expect("the main server's certificate");
                    d2_quic::fingerprint_hex(&d2_quic::fingerprint(&der))
                },
                game: get("game"),
                build: get("build"),
                protocol: get("protocol"),
                tick: get("tick"),
            },
            child,
            _stdin: stdin,
        }
    }
}

impl RealmInfo {
    /// A hosting process for this main server, started like the game
    /// client starts it (`--host`, the start line on stdin), with the
    /// stand-in's hello words on its command line (no secrets).
    fn host_config(&self, name: &str) -> HostConfig {
        self.host_config_at(name, self.addr)
    }

    fn host_config_at(&self, name: &str, realm: SocketAddr) -> HostConfig {
        let start = StartLine {
            realm: realm.to_string(),
            realm_cert: parse_hex32(&self.cert).expect("realm cert"),
            token: HostToken::new(TOKEN),
            version: self.build.clone(),
            port: 0,
        };
        let mut cfg = HostConfig::new(mp_bin("mp-test-stub-node"), start);
        cfg.args.extend([
            format!("hello_game={}", self.game),
            format!("hello_protocol={}", self.protocol),
            format!("hello_tick={}", self.tick),
        ]);
        cfg.dir = Some(work(name));
        cfg
    }
}

impl Drop for Realm {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Waits for an event that `want` picks, failing with everything seen.
fn wait_for(host: &HostProcess, what: &str, want: impl Fn(&HostEvent) -> bool) -> HostEvent {
    let end = Instant::now() + WAIT;
    let mut seen = Vec::new();
    while Instant::now() < end {
        if let Some(e) = host.next_event(Duration::from_millis(100)) {
            if want(&e) {
                return e;
            }
            seen.push(e);
        }
    }
    panic!(
        "no {what} within {WAIT:?}; seen {seen:?}; status {:?}",
        host.status()
    );
}

fn ready(host: &HostProcess) -> d2_host_control::Ready {
    match wait_for(host, "ready", |e| matches!(e, HostEvent::Ready(_))) {
        HostEvent::Ready(r) => r,
        _ => unreachable!(),
    }
}

/// Kills a process by id when dropped (it may be gone already).
struct KillOnDrop(u32);

impl Drop for KillOnDrop {
    fn drop(&mut self) {
        let pid = self.0.to_string();
        let mut c = if cfg!(windows) {
            let mut c = Command::new("taskkill");
            c.args(["/F", "/PID", &pid]);
            c
        } else {
            let mut c = Command::new("kill");
            c.args(["-9", &pid]);
            c
        };
        let _ = c.stdout(Stdio::null()).stderr(Stdio::null()).status();
    }
}

/// True once nothing holds `addr` any more: the game server has exited.
fn port_freed(addr: SocketAddr, within: Duration) -> bool {
    let end = Instant::now() + within;
    while Instant::now() < end {
        if UdpSocket::bind(addr).is_ok() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    false
}

#[test]
fn the_host_links_and_reports_ready() {
    let realm = Realm::start("ready");
    let host = HostProcess::spawn(realm.host_config("ready")).expect("spawn");
    let first = host.next_event(WAIT).expect("an event");
    assert!(
        matches!(first, HostEvent::Started { attempt: 1, .. }),
        "{first:?}"
    );
    let r = ready(&host);
    assert!(r.node.starts_with("h-") && r.node.len() == 14, "{}", r.node);
    assert!(
        r.listen.ip().is_loopback() && r.listen.port() != 0,
        "{}",
        r.listen
    );
    assert_eq!(r.connect_addr(), r.listen);
    let st = host.status();
    assert_eq!(st.phase, Phase::Ready);
    assert_eq!(st.reach_view().text(), "Opening port...");
    assert!(st.pid.is_some());
    assert!(!st.support_details().contains(&token_hex()));
}

#[test]
fn check_again_gives_a_reach_answer() {
    let realm = Realm::start("reach");
    let host = HostProcess::spawn(realm.host_config("reach")).expect("spawn");
    ready(&host);
    host.recheck();
    wait_for(&host, "reach", |e| matches!(e, HostEvent::Reach(_)));
    let view = host.status().reach_view();
    assert_eq!(view, ReachView::Reachable(ReachHow::Direct));
    assert_eq!(view.text(), "Reachable");
}

#[test]
fn sleep_closes_the_game_and_stop_exits_cleanly() {
    let realm = Realm::start("sleep");
    let host = HostProcess::spawn(realm.host_config("sleep")).expect("spawn");
    ready(&host);
    // The Windows sleep notice, as the game client's window gets it.
    assert!(host.power_message(0x0218, 0x0004));
    wait_for(
        &host,
        "closed (sleep)",
        |e| matches!(e, HostEvent::Closed { reason } if reason == "sleep"),
    );
    assert!(host.is_running());
    // The host leaves its own game: the game ends (open question 9's default).
    assert!(host.host_left_game());
    wait_for(
        &host,
        "closed (stop)",
        |e| matches!(e, HostEvent::Closed { reason } if reason == "stop"),
    );
    let e = wait_for(&host, "exit", |e| matches!(e, HostEvent::Exited { .. }));
    assert_eq!(
        e,
        HostEvent::Exited {
            how: ExitKind::Clean,
            restart: false
        }
    );
    let st = host.status();
    assert_eq!((st.phase, st.exit), (Phase::Exited, Some(ExitKind::Clean)));
    assert_eq!(st.reach_view(), ReachView::Stopped);
}

#[test]
fn quit_waits_for_a_clean_exit() {
    let realm = Realm::start("quit");
    let host = HostProcess::spawn(realm.host_config("quit")).expect("spawn");
    let listen = ready(&host).listen;
    let t = Instant::now();
    assert_eq!(host.quit(), Some(ExitKind::Clean));
    assert!(t.elapsed() < Duration::from_secs(5), "{:?}", t.elapsed());
    assert!(port_freed(listen, Duration::from_secs(2)));
}

/// Dropping the handle closes the child's stdin: the game server saves,
/// closes and exits by itself.
#[test]
fn dropping_the_handle_ends_the_game_server() {
    let realm = Realm::start("drop");
    let host = HostProcess::spawn(realm.host_config("drop")).expect("spawn");
    let listen = ready(&host).listen;
    assert!(
        UdpSocket::bind(listen).is_err(),
        "the game server holds its port"
    );
    drop(host);
    assert!(
        port_freed(listen, Duration::from_secs(10)),
        "it is still running"
    );
}

/// The child half of the test below: a game client that hosts and then
/// crashes. Does nothing unless started by that test.
#[test]
fn crashing_client_child() {
    let Ok(realm) = std::env::var("D2HC_CRASH_REALM") else {
        return;
    };
    let mut w = realm.split(' ');
    let mut next = || w.next().expect("realm field").to_string();
    let realm = RealmInfo {
        addr: next().parse().unwrap(),
        cert: next(),
        game: next(),
        build: next(),
        protocol: next(),
        tick: next(),
    };
    let host = HostProcess::spawn(realm.host_config("crash-client")).expect("spawn");
    let r = ready(&host);
    println!("LISTEN {}", r.listen);
    use std::io::Write;
    let _ = std::io::stdout().flush();
    std::process::abort();
}

/// The game client crashes while hosting: its pipes close and the game
/// server it started exits by itself (it saves everyone and closes first).
#[test]
fn a_crashed_client_leaves_no_game_server() {
    let realm = Realm::start("crash");
    let spec = [
        realm.addr.to_string(),
        realm.cert.clone(),
        realm.game.clone(),
        realm.build.clone(),
        realm.protocol.clone(),
        realm.tick.clone(),
    ]
    .join(" ");
    let errors = work("crash").join("client-stderr.txt");
    let mut client = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "crashing_client_child",
            "--nocapture",
            "--test-threads",
            "1",
        ])
        .env("D2HC_CRASH_REALM", spec)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(std::fs::File::create(&errors).unwrap())
        .spawn()
        .expect("starting the crashing client");
    let out = client.stdout.take().unwrap();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        for l in BufReader::new(out).lines().map_while(Result::ok) {
            let _ = tx.send(l);
        }
    });
    let end = Instant::now() + Duration::from_secs(60);
    let mut said = Vec::new();
    let listen: SocketAddr = loop {
        match rx.recv_timeout(end.saturating_duration_since(Instant::now())) {
            // libtest's "test NAME ... " may come first on the line.
            Ok(l) => match l.split_once("LISTEN ") {
                Some((_, a)) => break a.trim().parse().unwrap(),
                None => said.push(l),
            },
            Err(e) => {
                let _ = client.wait();
                let text = std::fs::read_to_string(&errors).unwrap_or_default();
                panic!("the client's game server didn't get ready ({e}): {said:?} {text}")
            }
        }
    };
    let status = client.wait().unwrap();
    assert!(!status.success(), "the client crashed");
    assert!(
        port_freed(listen, Duration::from_secs(10)),
        "the crashed client's game server is still running"
    );
}

/// A game server that doesn't exit after `stop` is killed after the grace
/// time. The stand-in's official mode ignores `stop` and its stdin.
#[test]
fn a_game_server_that_ignores_stop_is_killed() {
    let realm = Realm::start("kill");
    let mut cfg = realm.host_config("kill");
    let dir = cfg.dir.clone().unwrap();
    let file = dir.join("d2rust-node.cfg");
    std::fs::write(
        &file,
        format!(
            "node_id = n1\nrealm_addr = {}\nrealm_cert_sha256 = {}\nlisten = 127.0.0.1:0\n\
             hello_game = {}\nhello_build = {}\nhello_protocol = {}\nhello_tick = {}\n",
            realm.addr, realm.cert, realm.game, realm.build, realm.protocol, realm.tick
        ),
    )
    .unwrap();
    cfg.args = vec!["--config".into(), file.to_string_lossy().into_owned()];
    cfg.stop_grace = Duration::from_secs(1);
    let host = HostProcess::spawn(cfg).expect("spawn");
    // It ignores its stdin: if this test fails, nothing else ends it.
    let _guard = KillOnDrop(host.status().pid.expect("pid"));
    ready(&host);
    let t = Instant::now();
    host.stop();
    let e = wait_for(&host, "exit", |e| matches!(e, HostEvent::Exited { .. }));
    assert_eq!(
        e,
        HostEvent::Exited {
            how: ExitKind::Killed,
            restart: false
        }
    );
    assert!(t.elapsed() < Duration::from_secs(5), "{:?}", t.elapsed());
}

/// A game server that crashes before it linked is started once more; a
/// second crash ends hosting (`DEFAULT_RESTART`).
#[cfg(unix)]
#[test]
fn a_crash_before_ready_is_restarted_once() {
    let realm = Realm::start("restart");
    // A main-server address where nothing answers: it never gets ready.
    let silent = UdpSocket::bind("127.0.0.1:0").unwrap();
    let host = HostProcess::spawn(realm.host_config_at("restart", silent.local_addr().unwrap()))
        .expect("spawn");
    let kill = |pid: u32| {
        let ok = Command::new("kill")
            .args(["-9", &pid.to_string()])
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        assert!(ok, "kill -9 {pid}");
    };
    let HostEvent::Started { pid, attempt: 1 } = host.next_event(WAIT).unwrap() else {
        panic!()
    };
    kill(pid);
    let e = wait_for(&host, "exit", |e| matches!(e, HostEvent::Exited { .. }));
    assert_eq!(
        e,
        HostEvent::Exited {
            how: ExitKind::Crashed(None),
            restart: true
        }
    );
    let HostEvent::Started {
        pid: pid2,
        attempt: 2,
    } = wait_for(&host, "restart", |e| matches!(e, HostEvent::Started { .. }))
    else {
        panic!()
    };
    assert_ne!(pid, pid2);
    assert_eq!(host.status().starts, 2);
    kill(pid2);
    let e = wait_for(&host, "exit", |e| matches!(e, HostEvent::Exited { .. }));
    assert_eq!(
        e,
        HostEvent::Exited {
            how: ExitKind::Crashed(None),
            restart: false
        }
    );
    assert_eq!(host.status().phase, Phase::Exited);
    assert!(!host.is_running());
}

/// The token is on the child's stdin only: not in its command line or
/// its environment as the system shows them.
#[cfg(target_os = "linux")]
#[test]
fn the_token_is_not_in_the_process_list() {
    let realm = Realm::start("proc");
    let host = HostProcess::spawn(realm.host_config("proc")).expect("spawn");
    ready(&host);
    let pid = host.status().pid.unwrap();
    let token = token_hex();
    for f in ["cmdline", "environ"] {
        let bytes = std::fs::read(format!("/proc/{pid}/{f}")).unwrap();
        let text = String::from_utf8_lossy(&bytes);
        assert!(!text.contains(&token), "{f}: {text}");
        if f == "cmdline" {
            assert!(text.contains("--host"), "{text}");
        }
    }
}
