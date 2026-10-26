//! The port mapper against the fake router on loopback.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::{Duration, Instant};

use super::*;
use crate::testkit::fake_gateway::{FakePmp, FakeUpnp, PmpMode, UpnpBehaviour};

const PORT: u16 = 6112;
const WAIT: Duration = Duration::from_secs(8);
const PUBLIC: Ipv4Addr = Ipv4Addr::new(203, 0, 113, 7);

fn upnp_only(fake: &FakeUpnp, lease_s: u32) -> PortMapConfig {
    PortMapConfig {
        lease_s,
        upnp: Some(UpnpConfig {
            search_addr: fake.search_addr(),
            bind_addr: "127.0.0.1:0".parse().unwrap(),
            timeout: Duration::from_secs(3),
            trusted: vec![IpAddr::V4(Ipv4Addr::LOCALHOST)],
        }),
        pmp: None,
        ..PortMapConfig::new(PORT)
    }
}

fn pmp_only(gateway: IpAddr, lease_s: u32) -> PortMapConfig {
    PortMapConfig {
        lease_s,
        upnp: None,
        pmp: Some(PmpConfig {
            gateways: Some(vec![gateway]),
            initial_timeout: Duration::from_millis(150),
            retries: 1,
        }),
        ..PortMapConfig::new(PORT)
    }
}

fn mapped(mapper: &PortMapper) -> Mapping {
    match mapper.next_event(WAIT) {
        Some(MapEvent::Mapped(m)) => m,
        other => panic!("expected a mapping, got {other:?}"),
    }
}

fn failed(mapper: &PortMapper) -> Vec<Attempt> {
    match mapper.next_event(WAIT) {
        Some(MapEvent::Failed(a)) => a,
        other => panic!("expected a failure, got {other:?}"),
    }
}

fn removed(events: Vec<MapEvent>) -> Mapping {
    match events.last() {
        Some(MapEvent::Removed(m)) => *m,
        other => panic!("expected the removal last, got {other:?}"),
    }
}

/// Polls `f` until it holds or `WAIT` passes.
fn eventually(mut f: impl FnMut() -> bool) -> bool {
    let end = Instant::now() + WAIT;
    while Instant::now() < end {
        if f() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    false
}

#[test]
fn upnp_maps_renews_and_removes() {
    let fake = FakeUpnp::start(UpnpBehaviour::default()).unwrap();
    let mapper = PortMapper::start(upnp_only(&fake, 2)).unwrap();
    let m = mapped(&mapper);
    assert_eq!(m.method, ReachMethod::UpnpIgd);
    assert_eq!(m.external, SocketAddr::new(PUBLIC.into(), PORT));
    assert_eq!(m.internal.port(), PORT);
    assert!(m.internal.ip().is_loopback());
    assert_eq!(m.lease_s, 2);
    assert_eq!(m.gateway, IpAddr::V4(Ipv4Addr::LOCALHOST));
    assert_eq!(
        m.reach_info(),
        crate::ReachInfo {
            method: ReachMethod::UpnpIgd,
            mapped_addr: Some(m.external)
        }
    );
    let held = fake.mappings();
    assert_eq!(held.len(), 1);
    assert_eq!(held[0].internal, m.internal);
    let add = &fake.requests()[0];
    assert_eq!(add.args["NewProtocol"], "UDP");
    assert_eq!(add.args["NewPortMappingDescription"], DESCRIPTION);

    // Renewed at half the lease: the same mapping added again.
    assert_eq!(mapper.next_event(WAIT), Some(MapEvent::Renewed(m)));
    let adds = fake
        .actions()
        .iter()
        .filter(|a| *a == "AddPortMapping")
        .count();
    assert_eq!(adds, 2);

    assert_eq!(removed(mapper.stop()), m);
    assert!(fake.mappings().is_empty());
    assert_eq!(fake.actions().last().unwrap(), "DeletePortMapping");
}

#[test]
fn upnp_permanent_only_gets_lease_zero() {
    let fake = FakeUpnp::start(UpnpBehaviour {
        permanent_only: true,
        ..UpnpBehaviour::default()
    })
    .unwrap();
    let mapper = PortMapper::start(upnp_only(&fake, DEFAULT_LEASE_S)).unwrap();
    let m = mapped(&mapper);
    assert_eq!(m.lease_s, 0);
    let leases: Vec<String> = fake
        .requests()
        .iter()
        .filter(|r| r.action == "AddPortMapping")
        .map(|r| r.args["NewLeaseDuration"].clone())
        .collect();
    assert_eq!(leases, ["1200", "0"]);
    // Permanent: removed on exit all the same.
    assert_eq!(removed(mapper.stop()), m);
    assert!(fake.mappings().is_empty());
}

#[test]
fn upnp_port_taken_lets_the_router_pick() {
    let fake = FakeUpnp::start(UpnpBehaviour {
        taken: vec![PORT],
        ..UpnpBehaviour::default()
    })
    .unwrap();
    let mapper = PortMapper::start(upnp_only(&fake, DEFAULT_LEASE_S)).unwrap();
    let m = mapped(&mapper);
    assert_eq!(m.external, SocketAddr::new(PUBLIC.into(), 40000));
    assert_eq!(m.internal.port(), PORT);
    assert!(fake.actions().contains(&"AddAnyPortMapping".to_string()));
    assert_eq!(removed(mapper.stop()), m);
}

#[test]
fn upnp_refusal_is_reported() {
    let fake = FakeUpnp::start(UpnpBehaviour {
        refuse: true,
        ..UpnpBehaviour::default()
    })
    .unwrap();
    let mapper = PortMapper::start(upnp_only(&fake, DEFAULT_LEASE_S)).unwrap();
    let attempts = failed(&mapper);
    assert_eq!(attempts.len(), 1);
    assert_eq!(attempts[0].method, ReachMethod::UpnpIgd);
    assert_eq!(attempts[0].gateway, Some(IpAddr::V4(Ipv4Addr::LOCALHOST)));
    let text = attempts[0].to_string();
    assert!(text.starts_with("upnp 127.0.0.1: refused"), "{text}");
    assert!(mapper.stop().is_empty());
}

#[test]
fn nat_pmp_maps_renews_and_removes() {
    let fake = FakePmp::start(PmpMode::NatPmpOnly, PUBLIC).unwrap();
    let mapper = PortMapper::start(pmp_only(fake.gateway(), 2)).unwrap();
    let m = mapped(&mapper);
    assert_eq!(m.method, ReachMethod::NatPmp);
    assert_eq!(m.external, SocketAddr::new(PUBLIC.into(), PORT));
    assert_eq!(m.gateway, fake.gateway());
    assert_eq!(m.lease_s, 2);
    // PCP first, answered with NAT-PMP's unsupported version, then NAT-PMP.
    let versions: Vec<u8> = fake.requests().iter().map(|r| r.version).collect();
    assert_eq!(versions, [2, 0]);
    assert_eq!(fake.mappings().len(), 1);

    assert_eq!(mapper.next_event(WAIT), Some(MapEvent::Renewed(m)));
    assert_eq!(removed(mapper.stop()), m);
    assert!(fake.mappings().is_empty());
    assert_eq!(fake.requests().last().unwrap().lifetime, 0);
}

#[test]
fn pcp_maps_and_removes() {
    let fake = FakePmp::start(PmpMode::Pcp, PUBLIC).unwrap();
    let mapper = PortMapper::start(pmp_only(fake.gateway(), DEFAULT_LEASE_S)).unwrap();
    let m = mapped(&mapper);
    assert_eq!(m.method, ReachMethod::Pcp);
    assert_eq!(m.external, SocketAddr::new(PUBLIC.into(), PORT));
    assert_eq!(m.lease_s, DEFAULT_LEASE_S);
    assert!(fake.requests().iter().all(|r| r.version == 2));
    assert_eq!(removed(mapper.stop()), m);
    assert!(fake.mappings().is_empty());
}

#[test]
fn refusals_from_pcp_and_nat_pmp_are_reported() {
    let fake = FakePmp::start(PmpMode::Refuse, PUBLIC).unwrap();
    let mapper = PortMapper::start(pmp_only(fake.gateway(), DEFAULT_LEASE_S)).unwrap();
    let methods: Vec<ReachMethod> = failed(&mapper).iter().map(|a| a.method).collect();
    assert_eq!(methods, [ReachMethod::Pcp, ReachMethod::NatPmp]);
    assert!(fake.mappings().is_empty());
}

#[test]
fn nothing_answering_fails_in_bounded_time() {
    let quiet = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    let config = PortMapConfig {
        upnp: Some(UpnpConfig {
            search_addr: quiet.local_addr().unwrap(),
            bind_addr: "127.0.0.1:0".parse().unwrap(),
            timeout: Duration::from_millis(500),
            trusted: vec![IpAddr::V4(Ipv4Addr::LOCALHOST)],
        }),
        ..pmp_only(
            IpAddr::V4(Ipv4Addr::new(127, 254, 253, 252)),
            DEFAULT_LEASE_S,
        )
    };
    let started = Instant::now();
    let mapper = PortMapper::start(config).unwrap();
    let attempts = failed(&mapper);
    assert!(started.elapsed() < Duration::from_secs(5));
    assert!(attempts.iter().any(|a| a.method == ReachMethod::UpnpIgd));
    assert!(attempts
        .iter()
        .any(|a| matches!(a.method, ReachMethod::Pcp | ReachMethod::NatPmp)));
}

#[test]
fn first_mapping_wins_and_the_other_is_removed() {
    let upnp = FakeUpnp::start(UpnpBehaviour::default()).unwrap();
    let pmp = FakePmp::start(PmpMode::Pcp, PUBLIC).unwrap();
    let config = PortMapConfig {
        upnp: upnp_only(&upnp, DEFAULT_LEASE_S).upnp,
        ..pmp_only(pmp.gateway(), DEFAULT_LEASE_S)
    };
    let mapper = PortMapper::start(config).unwrap();
    let m = mapped(&mapper);
    assert!(
        eventually(|| upnp.mappings().len() + pmp.mappings().len() == 1),
        "only the winner's mapping stays"
    );
    match m.method {
        ReachMethod::UpnpIgd => assert_eq!(upnp.mappings().len(), 1),
        ReachMethod::Pcp => assert_eq!(pmp.mappings().len(), 1),
        other => panic!("unexpected method {other:?}"),
    }
    assert_eq!(removed(mapper.stop()), m);
    assert!(upnp.mappings().is_empty() && pmp.mappings().is_empty());
}

#[test]
fn dropping_the_mapper_removes_the_mapping() {
    let fake = FakePmp::start(PmpMode::Pcp, PUBLIC).unwrap();
    let mapper = PortMapper::start(pmp_only(fake.gateway(), DEFAULT_LEASE_S)).unwrap();
    mapped(&mapper);
    drop(mapper);
    assert!(fake.mappings().is_empty());
}
