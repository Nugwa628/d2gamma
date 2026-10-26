//! The NAT modes of the v2 setups' fabric, with plain sockets: what gets
//! through each router, and that hole punching works behind a cone NAT
//! and fails behind a symmetric one (plan N1, N10).

use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};
use std::time::{Duration, Instant};

use mp_test::net::nat::{Fabric, NatMode};
use mp_test::net::Conditions;

/// Long enough for a datagram that gets through to arrive.
const ARRIVES: Duration = Duration::from_millis(1500);
/// How long to wait before calling a datagram dropped.
const DROPPED: Duration = Duration::from_millis(300);

fn sock() -> UdpSocket {
    let s = UdpSocket::bind("127.0.0.1:0").expect("bind");
    s.set_read_timeout(Some(Duration::from_millis(20)))
        .expect("timeout");
    s
}

/// The next datagram within `wait`, ignoring stray errors.
fn recv(s: &UdpSocket, wait: Duration) -> Option<(Vec<u8>, SocketAddr)> {
    let until = Instant::now() + wait;
    let mut buf = [0u8; 2048];
    while Instant::now() < until {
        if let Ok((n, from)) = s.recv_from(&mut buf) {
            return Some((buf[..n].to_vec(), from));
        }
    }
    None
}

fn ip(n: u8) -> IpAddr {
    IpAddr::V4(Ipv4Addr::new(127, 0, 1, n))
}

/// A main server, a fabric and two parties that have each linked (sent
/// to the main server and had an answer back).
struct Net {
    realm: UdpSocket,
    fab: Fabric,
    p: (usize, UdpSocket),
    q: (usize, UdpSocket),
}

impl Net {
    fn new(p_mode: NatMode, q_mode: NatMode) -> Self {
        let realm = sock();
        let fab = Fabric::start(realm.local_addr().expect("addr"), 1).expect("fabric");
        let p = (fab.add_party("p", p_mode, ip(1)).expect("p"), sock());
        let q = (fab.add_party("q", q_mode, ip(2)).expect("q"), sock());
        let n = Self { realm, fab, p, q };
        n.link(&n.p);
        n.link(&n.q);
        n
    }

    /// A round trip to the main server and back, as a link's handshake.
    fn link(&self, (id, s): &(usize, UdpSocket)) {
        let ingress = self.fab.ingress(*id).expect("ingress");
        s.send_to(b"hello", ingress).expect("send");
        let (got, seen) = recv(&self.realm, ARRIVES).expect("the main server hears the party");
        assert_eq!(got, b"hello");
        assert_eq!(
            Some(seen),
            self.fab.public(*id),
            "the main server sees its outside address"
        );
        assert_eq!(
            seen.ip(),
            self.fab_ip(*id),
            "on the party's own loopback address"
        );
        self.realm.send_to(b"welcome", seen).expect("send");
        let (got, from) = recv(s, ARRIVES).expect("the answer comes back");
        assert_eq!(got, b"welcome");
        assert_eq!(from, ingress, "from the address it sent to");
    }

    fn fab_ip(&self, id: usize) -> IpAddr {
        if id == self.p.0 {
            ip(1)
        } else {
            ip(2)
        }
    }

    /// `from` sends to `to`'s outside address (as the main server would
    /// tell it); what `to` receives, and from where.
    fn send(&self, from: &(usize, UdpSocket), to: &(usize, UdpSocket)) -> Option<SocketAddr> {
        let target = self.fab.public(to.0).expect("public");
        from.1.send_to(b"punch", target).expect("send");
        recv(&to.1, ARRIVES).map(|(_, a)| a)
    }
}

#[test]
fn every_mode_links_out_to_the_main_server() {
    for mode in NatMode::ALL {
        let n = Net::new(mode, mode);
        assert_eq!(n.fab.mode(n.p.0), Some(mode));
    }
}

#[test]
fn open_takes_anyone() {
    let n = Net::new(NatMode::Open, NatMode::Cone);
    let from = n.send(&n.q, &n.p);
    assert_eq!(
        from,
        n.fab.public(n.q.0),
        "an unsolicited datagram reaches an open party"
    );
}

#[test]
fn cone_drops_strangers_until_it_punches() {
    let n = Net::new(NatMode::Cone, NatMode::Cone);
    let target = n.fab.public(n.p.0).expect("public");
    n.q.1.send_to(b"first", target).expect("send");
    assert!(
        recv(&n.p.1, DROPPED).is_none(),
        "a cone NAT drops a stranger"
    );
    // The punch: p sends to q's outside address; q's router saw q send
    // to p's, so it lets it in.
    let got = n.send(&n.p, &n.q);
    assert_eq!(
        got,
        n.fab.public(n.p.0),
        "behind a cone NAT one outside address serves everyone"
    );
    let got = n.send(&n.q, &n.p);
    assert_eq!(
        got,
        n.fab.public(n.q.0),
        "after the punch the peer gets through"
    );
    assert_eq!(n.fab.mappings(n.p.0), 1);
}

#[test]
fn symmetric_punching_fails() {
    let n = Net::new(NatMode::Symmetric, NatMode::Cone);
    let p_public = n.fab.public(n.p.0).expect("public");
    n.q.1.send_to(b"first", p_public).expect("send");
    assert!(
        recv(&n.p.1, DROPPED).is_none(),
        "a symmetric NAT drops a stranger"
    );
    let q_public = n.fab.public(n.q.0).expect("public");
    n.p.1.send_to(b"punch", q_public).expect("send");
    assert!(
        recv(&n.q.1, DROPPED).is_none(),
        "the punch leaves from a new address q never sent to"
    );
    assert_eq!(
        n.fab.mappings(n.p.0),
        2,
        "one outside address per destination"
    );
    n.q.1.send_to(b"again", p_public).expect("send");
    assert!(
        recv(&n.p.1, DROPPED).is_none(),
        "the address the main server saw stays shut"
    );
}

#[test]
fn blocked_only_has_the_main_server() {
    let n = Net::new(NatMode::Blocked, NatMode::Open);
    let p_public = n.fab.public(n.p.0).expect("public");
    n.q.1.send_to(b"in", p_public).expect("send");
    assert!(recv(&n.p.1, DROPPED).is_none(), "nothing comes in");
    let q_public = n.fab.public(n.q.0).expect("public");
    n.p.1.send_to(b"out", q_public).expect("send");
    assert!(
        recv(&n.q.1, DROPPED).is_none(),
        "nothing goes out but the link"
    );
    n.link(&n.p);
}

#[test]
fn cut_link_cuts_only_the_main_server_link() {
    let n = Net::new(NatMode::Cone, NatMode::Open);
    n.fab.cut_link(n.p.0, true);
    assert!(n.fab.link_cut(n.p.0));
    let ingress = n.fab.ingress(n.p.0).expect("ingress");
    n.p.1.send_to(b"up", ingress).expect("send");
    assert!(
        recv(&n.realm, DROPPED).is_none(),
        "the cut link carries nothing up"
    );
    n.realm
        .send_to(b"down", n.fab.public(n.p.0).expect("public"))
        .expect("send");
    assert!(recv(&n.p.1, DROPPED).is_none(), "nor down");
    let got = n.send(&n.p, &n.q);
    assert_eq!(
        got,
        n.fab.public(n.p.0),
        "players still reach the game server"
    );
    n.fab.cut_link(n.p.0, false);
    n.link(&n.p);
}

#[test]
fn conditions_delay_both_ways() {
    let n = Net::new(NatMode::Open, NatMode::Open);
    n.fab.set_conditions(
        n.p.0,
        Conditions {
            latency_ms: 150,
            ..Conditions::default()
        },
    );
    let t = Instant::now();
    n.link(&n.p);
    assert!(
        t.elapsed() >= Duration::from_millis(290),
        "{:?}",
        t.elapsed()
    );
    let mut c = n.fab.conditions(n.p.0);
    c.latency_ms = 0;
    c.cut = true;
    n.fab.set_conditions(n.p.0, c);
    let ingress = n.fab.ingress(n.p.0).expect("ingress");
    n.p.1.send_to(b"up", ingress).expect("send");
    assert!(
        recv(&n.realm, DROPPED).is_none(),
        "a cut party sends nothing"
    );
    let (_, dropped) = n.fab.counts();
    assert!(dropped >= 1);
}

#[test]
fn a_moved_main_server_is_found_again() {
    let n = Net::new(NatMode::Cone, NatMode::Cone);
    let moved = sock();
    n.fab.retarget_realm(moved.local_addr().expect("addr"));
    let ingress = n.fab.ingress(n.p.0).expect("ingress");
    n.p.1.send_to(b"again", ingress).expect("send");
    let (got, seen) = recv(&moved, ARRIVES).expect("the restarted server hears it");
    assert_eq!(got, b"again");
    assert_eq!(Some(seen), n.fab.public(n.p.0), "through the same mapping");
}

#[test]
fn modes_parse_by_name() {
    for m in NatMode::ALL {
        assert_eq!(m.name().parse::<NatMode>(), Ok(m));
    }
    assert!("full-cone".parse::<NatMode>().is_err());
}
