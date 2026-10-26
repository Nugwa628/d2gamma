//! Automatic port opening: asks the home router to forward the game
//! server's UDP port, the one its QUIC endpoint listens on.
//!
//! Three protocols are tried at once and the first mapping wins:
//! UPnP-IGD (found by SSDP multicast, `AddPortMapping`), and PCP with a
//! fall back to NAT-PMP (both sent to the default gateway, port 5351). A
//! second mapping that also succeeds is removed again. The mapping is
//! renewed at half its lease while hosting and removed when the game
//! server stops ([`PortMapper::stop`], or dropping the mapper).
//!
//! The protocol clients are the `igd-next` crate (UPnP-IGD, its tokio
//! client) and the `crab_nat` crate (PCP and NAT-PMP). The mapper runs
//! them on its own thread with a small tokio runtime, so the game server
//! only polls [`MapEvent`]s.
//!
//! A mapping's external address is what the router reports; behind a
//! second NAT it is private and the port is still closed. The outside
//! check (`check`) finds that out; this module only reports what the
//! router said.

mod pmp;
mod upnp;

use std::fmt;
use std::net::{IpAddr, SocketAddr};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

use tokio::sync::oneshot;
use tokio::task::{JoinHandle as Task, JoinSet};

use crate::{ReachInfo, ReachMethod};

/// The lease asked for, in seconds (20 minutes); renewed at half. A router
/// that only grants permanent mappings gets lease 0 (UPnP), which is then
/// removed on exit like any other.
pub const DEFAULT_LEASE_S: u32 = 1200;

/// The mapping's description in the router's table.
pub const DESCRIPTION: &str = "D2RUST game server";

/// The SSDP multicast address UPnP devices answer on.
pub const SSDP_ADDR: &str = "239.255.255.250:1900";

/// How long a UPnP search waits for a router.
pub const UPNP_SEARCH_MS: u64 = 3_000;

/// The first NAT-PMP/PCP request's wait, doubled on each retry
/// (RFC 6886 starts at 250 ms).
pub const PMP_INITIAL_MS: u64 = 250;

/// NAT-PMP/PCP retries after the first request (250 + 500 + 1000 ms).
pub const PMP_RETRIES: usize = 2;

/// How long stopping waits for the router to confirm the removal.
pub const STOP_WAIT_MS: u64 = 2_000;

/// How UPnP-IGD is searched for.
#[derive(Clone, Debug)]
pub struct UpnpConfig {
    /// Where the SSDP search goes ([`SSDP_ADDR`]).
    pub search_addr: SocketAddr,
    /// The local address the search is sent from.
    pub bind_addr: SocketAddr,
    /// How long the search waits for a router.
    pub timeout: Duration,
    /// Router addresses trusted beyond the one that answered the search.
    /// A loopback router (the tests' fake) is only trusted when listed.
    pub trusted: Vec<IpAddr>,
}

impl Default for UpnpConfig {
    fn default() -> Self {
        Self {
            search_addr: SSDP_ADDR.parse().expect("an address"),
            bind_addr: "0.0.0.0:0".parse().expect("an address"),
            timeout: Duration::from_millis(UPNP_SEARCH_MS),
            trusted: Vec::new(),
        }
    }
}

/// How NAT-PMP and PCP are tried.
#[derive(Clone, Debug)]
pub struct PmpConfig {
    /// The routers asked; `None` asks the default gateways
    /// ([`crate::gateway::default_gateways`]).
    pub gateways: Option<Vec<IpAddr>>,
    /// The first request's wait, doubled on each retry.
    pub initial_timeout: Duration,
    /// Retries after the first request.
    pub retries: usize,
}

impl Default for PmpConfig {
    fn default() -> Self {
        Self {
            gateways: None,
            initial_timeout: Duration::from_millis(PMP_INITIAL_MS),
            retries: PMP_RETRIES,
        }
    }
}

/// What to map and how.
#[derive(Clone, Debug)]
pub struct PortMapConfig {
    /// The game server's UDP port (its QUIC endpoint's).
    pub internal_port: u16,
    /// The game server's address on the home network; `None` takes the
    /// local address facing each router.
    pub local_ip: Option<IpAddr>,
    /// The lease asked for, in seconds ([`DEFAULT_LEASE_S`]).
    pub lease_s: u32,
    /// The description in the router's table ([`DESCRIPTION`]).
    pub description: String,
    /// UPnP-IGD, if tried.
    pub upnp: Option<UpnpConfig>,
    /// NAT-PMP and PCP, if tried.
    pub pmp: Option<PmpConfig>,
}

impl PortMapConfig {
    /// Every protocol, the default lease and description, for `port`.
    pub fn new(internal_port: u16) -> Self {
        Self {
            internal_port,
            local_ip: None,
            lease_s: DEFAULT_LEASE_S,
            description: DESCRIPTION.to_string(),
            upnp: Some(UpnpConfig::default()),
            pmp: Some(PmpConfig::default()),
        }
    }
}

/// A port forwarded by the router.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Mapping {
    /// The protocol that made it.
    pub method: ReachMethod,
    /// The public address players reach, as the router reports it.
    pub external: SocketAddr,
    /// The game server's address it forwards to.
    pub internal: SocketAddr,
    /// The lease granted, in seconds; 0 is permanent.
    pub lease_s: u32,
    /// The router.
    pub gateway: IpAddr,
}

impl Mapping {
    /// What the game server reports to the main server (`ReachInfo`).
    pub fn reach_info(&self) -> ReachInfo {
        ReachInfo {
            method: self.method,
            mapped_addr: Some(self.external),
        }
    }
}

/// One protocol's try that did not end in a mapping, for the support
/// details (`reach ... method=none`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Attempt {
    /// The protocol tried.
    pub method: ReachMethod,
    /// The router asked, when one was found.
    pub gateway: Option<IpAddr>,
    /// What happened.
    pub outcome: String,
}

impl Attempt {
    fn new(method: ReachMethod, gateway: Option<IpAddr>, outcome: impl fmt::Display) -> Self {
        Self {
            method,
            gateway,
            outcome: outcome.to_string(),
        }
    }
}

impl fmt::Display for Attempt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.method.word())?;
        if let Some(gw) = self.gateway {
            write!(f, " {gw}")?;
        }
        write!(f, ": {}", self.outcome)
    }
}

/// What the mapper did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MapEvent {
    /// The port is forwarded.
    Mapped(Mapping),
    /// No protocol made a mapping; the mapper has ended.
    Failed(Vec<Attempt>),
    /// The lease was renewed (or the port mapped again after a renewal was
    /// refused); the address may have changed.
    Renewed(Mapping),
    /// The mapping could not be renewed nor made again; the mapper has
    /// ended.
    Lost(Vec<Attempt>),
    /// The mapping was removed on stop.
    Removed(Mapping),
}

/// Opens the port and keeps it open: started with the game server, stopped
/// (or dropped) when it exits.
#[derive(Debug)]
pub struct PortMapper {
    events: mpsc::Receiver<MapEvent>,
    stop: Option<oneshot::Sender<()>>,
    thread: Option<JoinHandle<()>>,
}

impl PortMapper {
    /// Starts mapping `config.internal_port` on a thread of its own.
    pub fn start(config: PortMapConfig) -> std::io::Result<Self> {
        let (tx, events) = mpsc::channel();
        let (stop, stopped) = oneshot::channel();
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        let thread = std::thread::Builder::new()
            .name("d2-reach-portmap".into())
            .spawn(move || rt.block_on(run(Arc::new(config), tx, stopped)))?;
        Ok(Self {
            events,
            stop: Some(stop),
            thread: Some(thread),
        })
    }

    /// The next event, if one happened.
    pub fn try_event(&self) -> Option<MapEvent> {
        self.events.try_recv().ok()
    }

    /// The next event, waiting up to `timeout`.
    pub fn next_event(&self, timeout: Duration) -> Option<MapEvent> {
        self.events.recv_timeout(timeout).ok()
    }

    /// Removes the mapping (waiting up to [`STOP_WAIT_MS`] for the router)
    /// and ends the mapper. Returns the events not read yet, the last of
    /// which is [`MapEvent::Removed`] when a mapping was removed.
    pub fn stop(mut self) -> Vec<MapEvent> {
        self.shut_down();
        self.events.try_iter().collect()
    }

    fn shut_down(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for PortMapper {
    fn drop(&mut self) {
        self.shut_down();
    }
}

/// A mapping held, with what renews and removes it.
enum Held {
    Upnp(upnp::Held),
    Pmp(pmp::Held),
}

impl Held {
    fn mapping(&self) -> Mapping {
        match self {
            Held::Upnp(h) => h.mapping,
            Held::Pmp(h) => h.mapping,
        }
    }

    async fn renew(&mut self, config: &PortMapConfig) -> Result<(), Attempt> {
        match self {
            Held::Upnp(h) => h.renew(config).await,
            Held::Pmp(h) => h.renew().await,
        }
    }

    async fn remove(self) -> bool {
        match self {
            Held::Upnp(h) => h.remove().await,
            Held::Pmp(h) => h.remove().await,
        }
    }
}

async fn run(
    config: Arc<PortMapConfig>,
    events: mpsc::Sender<MapEvent>,
    mut stop: oneshot::Receiver<()>,
) {
    let mut losers = Vec::new();
    let first = tokio::select! {
        got = acquire(&config, &mut losers) => got,
        _ = &mut stop => {
            finish(None, losers).await;
            return;
        }
    };
    let mut held = match first {
        Ok(held) => held,
        Err(attempts) => {
            let _ = events.send(MapEvent::Failed(attempts));
            finish(None, losers).await;
            return;
        }
    };
    let _ = events.send(MapEvent::Mapped(held.mapping()));
    loop {
        let lease = held.mapping().lease_s;
        let renew_in = if lease == 0 {
            Duration::MAX
        } else {
            Duration::from_millis(u64::from(lease) * 500)
        };
        tokio::select! {
            _ = tokio::time::sleep(renew_in) => {}
            _ = &mut stop => break,
        }
        let renewed = tokio::select! {
            r = held.renew(&config) => r,
            _ = &mut stop => break,
        };
        if let Err(failed) = renewed {
            // The router forgot it (rebooted) or refused: map again, any
            // protocol.
            let again = tokio::select! {
                got = acquire(&config, &mut losers) => got,
                _ = &mut stop => break,
            };
            match again {
                Ok(new) => {
                    // The old one is gone or refused; removing it is best effort.
                    losers.retain(|t| !t.is_finished());
                    losers.push(tokio::spawn(async move {
                        held.remove().await;
                    }));
                    held = new;
                }
                Err(mut attempts) => {
                    attempts.insert(0, failed);
                    let _ = events.send(MapEvent::Lost(attempts));
                    finish(None, losers).await;
                    return;
                }
            }
        }
        let _ = events.send(MapEvent::Renewed(held.mapping()));
    }
    let mapping = held.mapping();
    let removed = finish(Some(held), losers).await;
    if removed {
        let _ = events.send(MapEvent::Removed(mapping));
    }
}

/// Removes `held`, then lets the removals of second mappings finish, all
/// within [`STOP_WAIT_MS`]; what is still running then is dropped with the
/// runtime. True when the router confirmed the removal of `held`.
async fn finish(held: Option<Held>, losers: Vec<Task<()>>) -> bool {
    let deadline = tokio::time::Instant::now() + Duration::from_millis(STOP_WAIT_MS);
    let removed = match held {
        Some(h) => tokio::time::timeout_at(deadline, h.remove())
            .await
            .unwrap_or(false),
        None => false,
    };
    for t in losers {
        let _ = tokio::time::timeout_at(deadline, t).await;
    }
    removed
}

/// Tries UPnP-IGD and NAT-PMP/PCP at once; the first mapping wins and a
/// later one is removed (its task joins `losers`). Every attempt's outcome
/// when none maps.
async fn acquire(
    config: &Arc<PortMapConfig>,
    losers: &mut Vec<Task<()>>,
) -> Result<Held, Vec<Attempt>> {
    let mut tries: JoinSet<Result<Held, Vec<Attempt>>> = JoinSet::new();
    if let Some(u) = config.upnp.clone() {
        let c = config.clone();
        tries.spawn(async move { upnp::map(&c, &u).await.map(Held::Upnp) });
    }
    if let Some(p) = config.pmp.clone() {
        let c = config.clone();
        tries.spawn(async move { pmp::map(&c, &p).await.map(Held::Pmp) });
    }
    let mut attempts = Vec::new();
    while let Some(done) = tries.join_next().await {
        match done {
            Ok(Ok(held)) => {
                if !tries.is_empty() {
                    losers.push(tokio::spawn(async move {
                        while let Some(late) = tries.join_next().await {
                            if let Ok(Ok(late)) = late {
                                late.remove().await;
                            }
                        }
                    }));
                }
                return Ok(held);
            }
            Ok(Err(a)) => attempts.extend(a),
            Err(e) => attempts.push(Attempt::new(ReachMethod::None, None, e)),
        }
    }
    if attempts.is_empty() {
        attempts.push(Attempt::new(ReachMethod::None, None, "no protocol enabled"));
    }
    Err(attempts)
}

#[cfg(test)]
mod tests;
