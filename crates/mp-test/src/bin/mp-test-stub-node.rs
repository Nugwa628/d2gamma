//! `mp-test-stub-node`: a stand-in game server for the MP test tool's v2
//! setups until `d2rust-node` exists (plan N0b). It runs no games. It
//! links to the main server with a v1 session handshake (so a v1 program
//! can stand in for the main server), prints the ready lines of contracts
//! 11.10 and 11.12, notices a lost link and links again, and as a host
//! (`--host`) follows the stdin lines of contracts 11.12.
//!
//! ```text
//! mp-test-stub-node --config d2rust-node.cfg   (keys: node_id, realm_addr, realm_cert_sha256,
//!                                              listen, hello_game, hello_build, hello_protocol,
//!                                              hello_tick; others are ignored)
//! mp-test-stub-node --host [hello_game=G] [hello_protocol=N] [hello_tick=N]
//!                                              (stdin: start realm=ADDR realm_cert=HEX64
//!                                              token=HEX32 version=V port=N)
//! ```

use std::io::BufRead;
use std::net::{SocketAddr, UdpSocket};
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use d2_net::wire::{parse_session_answer, ClientHello};
use d2_quic::{fingerprint, fingerprint_hex, parse_fingerprint, Identity, QuicClient, ServerTrust};
use mp_test::bot::BOT_TIMING;

/// How long it waits before linking again.
const RELINK: Duration = Duration::from_secs(1);

struct Plan {
    host: bool,
    id: String,
    realm: SocketAddr,
    pin: [u8; 32],
    listen: SocketAddr,
    hello: ClientHello,
}

fn hello(game: &str, build: &str, protocol: Option<u32>, tick: Option<u32>) -> ClientHello {
    ClientHello {
        protocol: protocol.unwrap_or(d2_proto::PROTOCOL_VERSION),
        game: game.into(),
        build: build.into(),
        tick_rate: tick.unwrap_or_else(|| d2_sim::tick::TickRate::BASE.hz()),
        resume: None,
    }
}

/// An official stand-in's plan, from its config file.
fn from_config(path: &str) -> Result<Plan, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
    let get = |key: &str| {
        text.lines()
            .filter_map(|l| l.split_once('='))
            .find(|(k, _)| k.trim() == key)
            .map(|(_, v)| v.trim().to_string())
    };
    let need = |key: &str| get(key).ok_or_else(|| format!("{path}: no {key}"));
    Ok(Plan {
        host: false,
        id: need("node_id")?,
        realm: need("realm_addr")?
            .parse()
            .map_err(|_| "realm_addr: not an address".to_string())?,
        pin: parse_fingerprint(&need("realm_cert_sha256")?)
            .ok_or("realm_cert_sha256: not 64 hex digits")?,
        listen: get("listen")
            .unwrap_or_else(|| "127.0.0.1:0".into())
            .parse()
            .map_err(|_| "listen: not an address".to_string())?,
        hello: hello(
            &get("hello_game").unwrap_or_else(|| "d2rust".into()),
            &get("hello_build").unwrap_or_else(|| "0.0.0".into()),
            get("hello_protocol").and_then(|v| v.parse().ok()),
            get("hello_tick").and_then(|v| v.parse().ok()),
        ),
    })
}

/// A host's plan, from its start line and the `KEY=VALUE` words after
/// `--host` (`hello_game`, `hello_protocol`, `hello_tick`).
fn from_start_line(line: &str, extra: &[&str]) -> Result<Plan, String> {
    let arg = |key: &str| {
        extra
            .iter()
            .filter_map(|w| w.split_once('='))
            .find(|(k, _)| *k == key)
            .map(|(_, v)| v.to_string())
    };
    let rest = line
        .trim()
        .strip_prefix("start ")
        .ok_or("the first line is not a start line")?;
    let get = |key: &str| {
        rest.split_whitespace()
            .filter_map(|w| w.split_once('='))
            .find(|(k, _)| *k == key)
            .map(|(_, v)| v.to_string())
    };
    let token = get("token").ok_or("no token")?;
    if token.len() != 32 || !token.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("the token is not 32 hex digits".into());
    }
    let port: u16 = get("port")
        .unwrap_or_else(|| "6113".into())
        .parse()
        .map_err(|_| "port: not a number".to_string())?;
    let seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos() as u64)
        ^ u64::from(std::process::id());
    Ok(Plan {
        host: true,
        id: format!("h-{:012x}", seed & 0xffff_ffff_ffff),
        realm: get("realm")
            .ok_or("no realm")?
            .parse()
            .map_err(|_| "realm: not an address".to_string())?,
        pin: parse_fingerprint(&get("realm_cert").ok_or("no realm_cert")?)
            .ok_or("realm_cert: not 64 hex digits")?,
        listen: SocketAddr::from(([127, 0, 0, 1], port)),
        hello: hello(
            &arg("hello_game").unwrap_or_else(|| "d2rust".into()),
            &get("version").unwrap_or_else(|| "0.0.0".into()),
            arg("hello_protocol").and_then(|v| v.parse().ok()),
            arg("hello_tick").and_then(|v| v.parse().ok()),
        ),
    })
}

/// How a link ended.
enum End {
    /// It never came up.
    NotLinked(String),
    /// It was up and dropped.
    Lost(String),
}

/// One link: a session handshake with the main server. Holds until the
/// link drops or `stop` is set.
fn link_once(p: &Plan, ready: &str, stop: &AtomicBool) -> Result<(), End> {
    let (client, mut control) =
        QuicClient::connect(p.realm, ServerTrust::Pinned(p.pin), BOT_TIMING)
            .map_err(|e| End::NotLinked(format!("connect: {e}")))?;
    let answer = control
        .send(p.hello.body())
        .and_then(|()| control.recv_timeout(BOT_TIMING.timeout))
        .map_err(|e| End::NotLinked(format!("handshake: {e}")))?
        .ok_or_else(|| End::NotLinked("the main server did not answer in time".into()))?;
    parse_session_answer(&answer).map_err(|r| End::NotLinked(format!("refused: {}", r.text)))?;
    println!("{ready}");
    while !stop.load(Ordering::Relaxed) {
        if let Err(e) = control.recv() {
            drop(client);
            return Err(End::Lost(e.to_string()));
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    Ok(())
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let stdin = std::io::stdin();
    let mut lines = stdin.lock().lines();
    let plan = match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        ["--config", path] => from_config(path),
        ["--host", extra @ ..] => match lines.next() {
            Some(Ok(l)) => from_start_line(&l, extra),
            _ => Err("stdin closed before the start line".into()),
        },
        _ => Err("usage: mp-test-stub-node --config FILE | --host [KEY=VALUE..]".into()),
    };
    let plan = match plan {
        Ok(p) => p,
        Err(e) => {
            eprintln!("mp-test-stub-node: {e}");
            if args.first().is_some_and(|a| a == "--host") {
                println!("error code=start text={}", e.replace(' ', "_"));
            }
            return ExitCode::from(2);
        }
    };
    // The players' socket: the stand-in serves nobody on it, but it holds
    // the port and reports it as a game server would.
    let socket = match UdpSocket::bind(plan.listen) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("mp-test-stub-node: binding {}: {e}", plan.listen);
            return ExitCode::FAILURE;
        }
    };
    let listen = socket.local_addr().unwrap_or(plan.listen);
    let cert = match Identity::self_signed() {
        Ok(i) => fingerprint_hex(&fingerprint(&i.cert)),
        Err(e) => {
            eprintln!("mp-test-stub-node: identity: {e}");
            return ExitCode::FAILURE;
        }
    };
    let ready = if plan.host {
        format!("ready node={} listen={listen} cert_sha256={cert}", plan.id)
    } else {
        format!(
            "ready program=node node={} listen={listen} cert_sha256={cert} realm={} linked=1 caps=nodes",
            plan.id, plan.realm
        )
    };
    let stop = Arc::new(AtomicBool::new(false));
    let host = plan.host;
    {
        let stop = stop.clone();
        std::thread::spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                match link_once(&plan, &ready, &stop) {
                    Ok(()) => break,
                    Err(End::Lost(why)) => {
                        let why = why.replace(' ', "_");
                        if plan.host {
                            println!("error code=link text={why}");
                        } else {
                            println!("link lost reason={why}");
                        }
                    }
                    Err(End::NotLinked(why)) => eprintln!("mp-test-stub-node: link: {why}"),
                }
                std::thread::sleep(RELINK);
            }
        });
    }
    // Contracts 11.12 for a host. An official stand-in, like an official
    // game server, runs on at the end of its input and stops on SIGTERM
    // (or a `quit` line).
    let reason = loop {
        match lines.next() {
            Some(Ok(l)) => match l.trim() {
                "quit" if !host => break "quit",
                _ if !host => {}
                "stop" if host => break "stop",
                "sleep" if host => println!("closed reason=sleep"),
                "recheck" if host => {
                    println!("reach state=open method=none addr={listen} nat=unknown")
                }
                _ => {}
            },
            _ if !host => loop {
                std::thread::park();
            },
            _ => break "client-gone",
        }
    };
    stop.store(true, Ordering::Relaxed);
    if host {
        println!("closed reason={reason}");
    }
    ExitCode::SUCCESS
}
