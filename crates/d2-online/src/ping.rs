//! Pings to the game servers the main server lists, measured in the
//! background while the player is in the lobby (contracts section 11.4).
//!
//! A server's ping is the median of [`PING_TRIES`] QUIC pings
//! (`d2_quic::ping`: a handshake on its own application protocol against
//! the server's pinned certificate, QUIC's own round-trip estimate). It is
//! kept [`PING_KEEP`] and then measured again while the server is still
//! listed. [`Pinger`] does this on a thread of its own, so the screens only
//! read [`Pinger::pings`]; [`Pings::auto`] is the automatic choice.

use std::collections::BTreeMap;
use std::net::{SocketAddr, ToSocketAddrs};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use d2_net::msg::nodelist::{pick_auto, NodeInfo};
use d2_quic::ping::PingClient;
use d2_quic::ServerTrust;

/// Pings per measurement; the median counts.
pub const PING_TRIES: usize = 3;
/// How long a measurement is kept before the server is pinged again.
pub const PING_KEEP: Duration = Duration::from_secs(60);
/// How long one ping waits for its answer.
pub const PING_WAIT: Duration = Duration::from_secs(2);
/// The pause between the pings of one measurement.
const PING_GAP: Duration = Duration::from_millis(50);

/// What to ping: a listed server's id, its ping address and pin.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PingTarget {
    pub node: String,
    /// `host:port`.
    pub addr: String,
    pub cert_sha256: [u8; 32],
}

impl PingTarget {
    pub fn of(n: &NodeInfo) -> Self {
        Self {
            node: n.node.clone(),
            addr: n.ping_addr.clone(),
            cert_sha256: n.cert_sha256,
        }
    }
}

/// One measurement.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ping {
    /// The median round trip in milliseconds; `None`: no ping answered.
    pub ms: Option<u32>,
    pub at: Instant,
}

/// The measurements by server id.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Pings(BTreeMap<String, Ping>);

impl Pings {
    /// The measurement of `node`: `None` while it is being measured.
    pub fn get(&self, node: &str) -> Option<Ping> {
        self.0.get(node).copied()
    }

    /// `node`'s round trip in milliseconds, if one answered.
    pub fn ms(&self, node: &str) -> Option<u32> {
        self.get(node).and_then(|p| p.ms)
    }

    pub fn insert(&mut self, node: &str, p: Ping) {
        self.0.insert(node.to_owned(), p);
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, Ping)> {
        self.0.iter().map(|(k, v)| (k.as_str(), *v))
    }

    /// The automatic choice among `nodes` with these pings
    /// ([`pick_auto`]).
    pub fn auto<'a>(&self, nodes: &'a [NodeInfo]) -> Option<&'a NodeInfo> {
        pick_auto(nodes, &|n| self.ms(n))
    }
}

/// The median of the answered samples (the lower middle for an even
/// count); `None` when none answered.
pub fn median(samples: &[Option<Duration>]) -> Option<u32> {
    let mut ms: Vec<u32> = samples
        .iter()
        .flatten()
        .map(|d| u32::try_from(d.as_millis()).unwrap_or(u32::MAX))
        .collect();
    ms.sort_unstable();
    ms.get(ms.len().checked_sub(1)? / 2).copied()
}

/// One ping to one server: the round trip, or `None` without an answer.
/// [`QuicProbe`] is the real one; tests use their own.
pub trait Probe: Send + 'static {
    fn ping(&mut self, t: &PingTarget) -> Option<Duration>;
}

/// Pings over QUIC with `d2_quic::ping`.
#[derive(Debug)]
pub struct QuicProbe(PingClient);

impl QuicProbe {
    pub fn new() -> std::io::Result<Self> {
        PingClient::new()
            .map(Self)
            .map_err(|e| std::io::Error::other(e.to_string()))
    }
}

fn resolve(addr: &str) -> Option<SocketAddr> {
    addr.to_socket_addrs().ok()?.next()
}

impl Probe for QuicProbe {
    fn ping(&mut self, t: &PingTarget) -> Option<Duration> {
        let addr = resolve(&t.addr)?;
        self.0
            .ping(addr, ServerTrust::Pinned(t.cert_sha256), PING_WAIT)
            .ok()
    }
}

#[derive(Debug, Default)]
struct State {
    targets: Vec<PingTarget>,
    pings: Pings,
    /// Rises with every new measurement.
    version: u64,
    stop: bool,
}

#[derive(Debug, Default)]
struct Shared {
    state: Mutex<State>,
    wake: Condvar,
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|p| p.into_inner())
    }
}

/// Measures the listed servers in the background. Dropping it stops the
/// thread without waiting for it: the ping in flight (up to [`PING_WAIT`])
/// finishes on its own, so a logout on the front end's tick never waits
/// for a server that doesn't answer.
#[derive(Debug)]
pub struct Pinger {
    shared: Arc<Shared>,
}

impl Pinger {
    /// Starts the thread with `probe`; it waits for targets.
    pub fn start(probe: impl Probe) -> std::io::Result<Self> {
        Self::start_with(probe, PING_KEEP)
    }

    /// [`Pinger::start`] keeping each measurement `keep` (tests).
    pub fn start_with(mut probe: impl Probe, keep: Duration) -> std::io::Result<Self> {
        let shared = Arc::new(Shared::default());
        let s = shared.clone();
        std::thread::Builder::new()
            .name("online-ping".into())
            .spawn(move || run(&s, &mut probe, keep))?;
        Ok(Self { shared })
    }

    /// The servers to measure (the main server's latest list). Servers no
    /// longer listed lose their measurement; new ones are measured first.
    pub fn set_targets(&self, targets: Vec<PingTarget>) {
        let mut st = self.shared.lock();
        if st.targets == targets {
            return;
        }
        let keep: Vec<String> = targets.iter().map(|t| t.node.clone()).collect();
        st.pings.0.retain(|k, _| keep.contains(k));
        st.targets = targets;
        st.version += 1;
        drop(st);
        self.shared.wake.notify_all();
    }

    /// The measurements so far.
    pub fn pings(&self) -> Pings {
        self.shared.lock().pings.clone()
    }

    /// Changes whenever [`Pinger::pings`] does.
    pub fn version(&self) -> u64 {
        self.shared.lock().version
    }
}

impl Drop for Pinger {
    fn drop(&mut self) {
        self.shared.lock().stop = true;
        self.shared.wake.notify_all();
    }
}

/// The thread: measure whatever is unmeasured or older than `keep`, then
/// sleep until the oldest measurement is due or the targets change.
fn run(shared: &Shared, probe: &mut dyn Probe, keep: Duration) {
    let mut st = shared.lock();
    loop {
        if st.stop {
            return;
        }
        let now = Instant::now();
        let due = st
            .targets
            .iter()
            .find(|t| {
                st.pings
                    .get(&t.node)
                    .is_none_or(|p| now.saturating_duration_since(p.at) >= keep)
            })
            .cloned();
        let Some(target) = due else {
            let next = st
                .pings
                .iter()
                .map(|(_, p)| (p.at + keep).saturating_duration_since(now))
                .min();
            st = match next {
                Some(wait) => {
                    shared
                        .wake
                        .wait_timeout(st, wait)
                        .unwrap_or_else(|p| p.into_inner())
                        .0
                }
                None => shared.wake.wait(st).unwrap_or_else(|p| p.into_inner()),
            };
            continue;
        };
        drop(st);
        let mut samples = Vec::with_capacity(PING_TRIES);
        for i in 0..PING_TRIES {
            if i > 0 {
                std::thread::sleep(PING_GAP);
            }
            if shared.lock().stop {
                return;
            }
            samples.push(probe.ping(&target));
        }
        let ping = Ping {
            ms: median(&samples),
            at: Instant::now(),
        };
        st = shared.lock();
        // The list may have changed meanwhile.
        if st.targets.contains(&target) {
            st.pings.insert(&target.node, ping);
            st.version += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use d2_net::msg::nodelist::NodeState;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn target(node: &str) -> PingTarget {
        PingTarget {
            node: node.into(),
            addr: format!("{node}.invalid:6113"),
            cert_sha256: [1; 32],
        }
    }

    /// Answers with a fixed round trip per server (none for unknown ones)
    /// and counts the pings.
    struct Table(Vec<(&'static str, u64)>, Arc<AtomicUsize>);

    impl Probe for Table {
        fn ping(&mut self, t: &PingTarget) -> Option<Duration> {
            self.1.fetch_add(1, Ordering::SeqCst);
            self.0
                .iter()
                .find(|(n, _)| *n == t.node)
                .map(|&(_, ms)| Duration::from_millis(ms))
        }
    }

    fn wait_for(p: &Pinger, ok: impl Fn(&Pings) -> bool) -> Pings {
        let end = Instant::now() + Duration::from_secs(5);
        loop {
            let got = p.pings();
            if ok(&got) {
                return got;
            }
            assert!(Instant::now() < end, "no measurement: {got:?}");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn the_median_of_three_and_none_without_answers() {
        let ms = |v: u64| Some(Duration::from_millis(v));
        assert_eq!(median(&[ms(30), ms(10), ms(90)]), Some(30));
        assert_eq!(median(&[ms(30), None, ms(10)]), Some(10));
        assert_eq!(median(&[None, ms(42), None]), Some(42));
        assert_eq!(median(&[None, None, None]), None);
        assert_eq!(median(&[]), None);
    }

    #[test]
    fn listed_servers_are_measured_in_the_background() {
        let count = Arc::new(AtomicUsize::new(0));
        let p = Pinger::start(Table(vec![("near", 20), ("far", 80)], count.clone())).unwrap();
        let v0 = p.version();
        p.set_targets(vec![target("near"), target("far"), target("gone")]);
        let got = wait_for(&p, |g| g.iter().count() == 3);
        assert_eq!(got.ms("near"), Some(20));
        assert_eq!(got.ms("far"), Some(80));
        assert_eq!(got.get("gone").map(|p| p.ms), Some(None));
        assert!(p.version() > v0);
        assert_eq!(count.load(Ordering::SeqCst), 3 * PING_TRIES);
        // Kept: no more pings within the keep time.
        std::thread::sleep(Duration::from_millis(100));
        assert_eq!(count.load(Ordering::SeqCst), 3 * PING_TRIES);
        // A server that leaves the list loses its measurement.
        p.set_targets(vec![target("near")]);
        assert_eq!(p.pings().iter().count(), 1);
    }

    /// Answers after `.0`, counting the pings that started.
    struct Slow(Duration, Arc<AtomicUsize>);

    impl Probe for Slow {
        fn ping(&mut self, _: &PingTarget) -> Option<Duration> {
            self.1.fetch_add(1, Ordering::SeqCst);
            std::thread::sleep(self.0);
            None
        }
    }

    /// Regression: dropping the pinger (a logout, on the front end's tick)
    /// waited for the ping in flight and the tries after it.
    #[test]
    fn dropping_it_does_not_wait_for_a_ping_in_flight() {
        let count = Arc::new(AtomicUsize::new(0));
        let p = Pinger::start(Slow(Duration::from_millis(400), count.clone())).unwrap();
        p.set_targets(vec![target("silent")]);
        let end = Instant::now() + Duration::from_secs(5);
        while count.load(Ordering::SeqCst) == 0 {
            assert!(Instant::now() < end, "no ping started");
            std::thread::sleep(Duration::from_millis(5));
        }
        let t = Instant::now();
        drop(p);
        assert!(
            t.elapsed() < Duration::from_millis(100),
            "{:?}",
            t.elapsed()
        );
        // The thread stops after the ping in flight: no further tries.
        std::thread::sleep(Duration::from_millis(600));
        assert_eq!(count.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn a_measurement_is_taken_again_when_it_is_old() {
        let count = Arc::new(AtomicUsize::new(0));
        let p = Pinger::start_with(
            Table(vec![("a", 5)], count.clone()),
            Duration::from_millis(150),
        )
        .unwrap();
        p.set_targets(vec![target("a")]);
        let first = wait_for(&p, |g| g.get("a").is_some()).get("a").unwrap();
        wait_for(&p, |g| g.get("a").is_some_and(|x| x.at > first.at));
        assert!(count.load(Ordering::SeqCst) >= 2 * PING_TRIES);
    }

    #[test]
    fn automatic_uses_the_measurements() {
        let node = |id: &str, load_pct| NodeInfo {
            node: id.into(),
            name: id.into(),
            location: String::new(),
            ping_addr: String::new(),
            cert_sha256: [0; 32],
            load_pct,
            state: NodeState::Open,
            suggested: id == "hint",
        };
        let nodes = [node("hint", 0), node("a", 0), node("b", 0)];
        let mut pings = Pings::default();
        assert_eq!(pings.auto(&nodes).unwrap().node, "hint");
        let at = Instant::now();
        pings.insert("a", Ping { ms: Some(80), at });
        pings.insert("b", Ping { ms: Some(20), at });
        pings.insert("hint", Ping { ms: None, at });
        assert_eq!(pings.auto(&nodes).unwrap().node, "b");
    }

    #[test]
    fn a_real_quic_ping_reaches_a_ping_server() {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .unwrap();
        let id = d2_quic::Identity::self_signed().unwrap();
        let srv = d2_quic::ping::PingServer::bind(
            "127.0.0.1:0".parse().unwrap(),
            &id,
            rt.handle().clone(),
        )
        .unwrap();
        let t = PingTarget {
            node: "local".into(),
            addr: srv.local_addr().unwrap().to_string(),
            cert_sha256: d2_quic::fingerprint(&id.cert),
        };
        let p = Pinger::start(QuicProbe::new().unwrap()).unwrap();
        p.set_targets(vec![t]);
        let got = wait_for(&p, |g| g.get("local").is_some());
        let ms = got.ms("local").expect("answered");
        assert!(ms < 500, "{ms} ms on loopback");
    }
}
