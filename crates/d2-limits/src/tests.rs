use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::time::{Duration, Instant};

use super::*;

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

fn v4(last: u8, port: u16) -> SocketAddr {
    SocketAddr::new(IpAddr::V4(Ipv4Addr::new(203, 0, 113, last)), port)
}

/// How many of `tries` evenly spread over `span` pass `rate` for one key.
fn passed(rate: Rate, tries: u32, span: Duration) -> u32 {
    let t0 = Instant::now();
    let mut l = Limiter::new(rate);
    (0..tries)
        .filter(|&i| l.check(&1u64, t0 + span * i / tries).is_ok())
        .count() as u32
}

#[test]
fn a_burst_passes_up_to_the_bucket_then_waits_one_token() {
    let t0 = Instant::now();
    let mut l = Limiter::new(rates::LOGIN);
    for i in 0..10 {
        assert!(l.check(&"a", t0).is_ok(), "attempt {i} of the burst");
    }
    let refused = l.check(&"a", t0).unwrap_err();
    assert_eq!(
        refused.wait,
        Duration::from_secs(6),
        "one token is 60 s / 10"
    );
    assert_eq!(refused.wait_s(), 6);
    assert_eq!(l.wait(&"a", t0), Duration::from_secs(6));
    // Refused attempts take nothing: hammering doesn't push the wait out.
    for _ in 0..1000 {
        assert!(l.check(&"a", t0).is_err());
    }
    assert!(l.check(&"a", t0 + Duration::from_secs(6)).is_ok());
    assert!(l.check(&"a", t0 + Duration::from_secs(6)).is_err());
}

#[test]
fn a_slow_drip_at_the_rate_always_passes() {
    // One login every 6 s forever: exactly the refill rate.
    let t0 = Instant::now();
    let mut l = Limiter::new(rates::LOGIN);
    for i in 0..1000u32 {
        assert!(
            l.check(&"a", t0 + Duration::from_secs(6) * i).is_ok(),
            "drip {i}"
        );
    }
    // Slower than the rate: the bucket is full again between, so the key
    // is forgotten.
    let mut l = Limiter::new(rates::CHAT);
    for i in 0..100u32 {
        assert!(l.check(&7u64, t0 + Duration::from_secs(11) * i).is_ok());
        l.prune(t0 + Duration::from_secs(11) * i + Duration::from_secs(10));
        assert!(l.is_empty(), "a full bucket holds no memory");
    }
}

#[test]
fn a_drip_just_faster_than_the_rate_is_cut_to_the_rate() {
    // 10 chat lines a second for 100 s: the burst, then one every 2 s.
    let n = passed(rates::CHAT, 1000, Duration::from_secs(100));
    assert!((54..=56).contains(&n), "{n} lines passed");
}

#[test]
fn keys_are_independent() {
    let t0 = Instant::now();
    let mut l = Limiter::new(rates::CREATE_GAME);
    for _ in 0..5 {
        l.check(&1u64, t0).unwrap();
    }
    assert!(l.check(&1u64, t0).is_err());
    assert!(l.check(&2u64, t0).is_ok(), "another account");
}

#[test]
fn byte_costs_and_costs_above_the_burst() {
    let t0 = Instant::now();
    let mut l = Limiter::new(rates::JOURNAL_BYTES);
    // A 4 MiB burst, then about 64 KiB a second.
    l.check_n(&9u32, 4 << 20, t0).unwrap();
    assert!(l.check_n(&9u32, 1024, t0).is_err());
    assert!(l
        .check_n(&9u32, 60 << 10, t0 + Duration::from_secs(1))
        .is_ok());
    assert!(l
        .check_n(&9u32, 60 << 10, t0 + Duration::from_secs(1))
        .is_err());
    // More than the bucket never passes, even from empty, and takes nothing.
    let mut l = Limiter::new(rates::JOURNAL_BYTES);
    let r = l.check_n(&9u32, (4 << 20) + 1, t0).unwrap_err();
    assert_eq!(r.wait, rates::JOURNAL_BYTES.per);
    assert!(l.is_empty());
    assert!(l.check_n(&9u32, u32::MAX, t0).is_err());
}

#[test]
fn a_flood_of_new_keys_keeps_the_table_bounded() {
    let t0 = Instant::now();
    let mut l = Limiter::with_keys(rates::LOGIN, 1000);
    for k in 0..200_000u64 {
        l.check(&k, t0).unwrap();
        assert!(l.len() <= 1000);
    }
    assert_eq!(l.len(), 1000);
    // The key a flooding client keeps hitting stays limited: it is never
    // the one closest to full.
    let mut l = Limiter::with_keys(rates::LOGIN, 100);
    for _ in 0..10 {
        l.check(&u64::MAX, t0).unwrap();
    }
    // 10,000 new keys in its first 6 s (its next token's time).
    for k in 0..10_000u64 {
        let at = t0 + Duration::from_micros(k * 599);
        let _ = l.check(&k, at);
        assert!(l.check(&u64::MAX, at).is_err(), "at {k}");
    }
    assert!(l.check(&u64::MAX, t0 + Duration::from_secs(6)).is_ok());
}

#[test]
fn the_test_mode_divides_the_window_by_60() {
    let r = rates::REGISTER.test();
    assert_eq!(r, Rate::per_secs(3, 60));
    let t0 = Instant::now();
    let mut l = Limiter::new(r);
    for _ in 0..3 {
        l.check(&1u8, t0).unwrap();
    }
    assert_eq!(l.check(&1u8, t0).unwrap_err().wait, Duration::from_secs(20));
}

#[test]
fn wait_s_rounds_up_and_is_at_least_one() {
    assert_eq!(Refused { wait: ms(1) }.wait_s(), 1);
    assert_eq!(Refused { wait: ms(1001) }.wait_s(), 2);
    assert_eq!(
        Refused {
            wait: Duration::ZERO
        }
        .wait_s(),
        1
    );
}

#[test]
fn addresses_count_by_ipv4_or_ipv6_slash_64_never_the_port() {
    let a = AddrKey::from(v4(1, 1000));
    assert_eq!(a, AddrKey::from(v4(1, 2000)));
    assert_ne!(a, AddrKey::from(v4(2, 1000)));
    let mapped: IpAddr = "::ffff:203.0.113.1".parse().unwrap();
    assert_eq!(AddrKey::of(mapped), a);
    let x: IpAddr = "2001:db8:1:2:aaaa::1".parse().unwrap();
    let y: IpAddr = "2001:db8:1:2:ffff:1:2:3".parse().unwrap();
    let z: IpAddr = "2001:db8:1:3::1".parse().unwrap();
    assert_eq!(AddrKey::of(x), AddrKey::of(y));
    assert_ne!(AddrKey::of(x), AddrKey::of(z));
    assert_eq!(
        AddrKey::of(x).ip(),
        IpAddr::V6(Ipv6Addr::new(0x2001, 0xdb8, 1, 2, 0, 0, 0, 0))
    );
}

#[test]
fn a_ping_flood_from_one_address_is_cut_before_the_handshake() {
    // 1000 pings in 10 s from one address, each from a new port: the burst
    // of 10, then one a second.
    let gate = AddrGate::new(rates::PING);
    let t0 = Instant::now();
    let n = (0..1000u32)
        .filter(|&i| {
            gate.admit_at(v4(1, 1024 + i as u16), t0 + ms(10) * i)
                .is_ok()
        })
        .count();
    assert_eq!(n, 19, "10 at once plus one per second for 9.99 s");
    // Another address is untouched.
    assert!(gate.admit_at(v4(2, 1), t0).is_ok());
    // A player pinging 3 times every 60 s (the client's median of 3) is
    // never refused.
    let gate = AddrGate::new(rates::PING);
    for round in 0..100u32 {
        for p in 0..3u32 {
            let at = t0 + Duration::from_secs(60) * round + ms(200) * p;
            assert!(gate.admit_at(v4(3, 5000), at).is_ok());
        }
    }
}

#[test]
fn the_gate_stays_bounded_under_many_addresses() {
    let gate = AddrGate::with_keys(rates::PING, 4096);
    let t0 = Instant::now();
    for i in 0..100_000u32 {
        let [_, b, c, d] = i.to_be_bytes();
        let from = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(10, b, c, d)), 4000);
        gate.admit_at(from, t0).unwrap();
    }
    assert_eq!(gate.len(), 4096);
    gate.admit_at(v4(9, 1), t0 + Duration::from_secs(11))
        .unwrap();
    assert_eq!(gate.len(), 1, "every full bucket is forgotten");
}

#[test]
fn every_default_lets_its_burst_through() {
    for r in [
        rates::LOGIN,
        rates::REGISTER,
        rates::CREATE_GAME,
        rates::JOIN_WRONG_PASSWORD,
        rates::JOIN,
        rates::TICKET,
        rates::CHAT,
        rates::MARKET_SEARCH,
        rates::MARKET_POST,
        rates::PING,
        rates::HANDSHAKE,
        rates::LINK_MESSAGES,
        rates::SAVES,
    ] {
        let t0 = Instant::now();
        let mut l = Limiter::new(r);
        for _ in 0..r.burst {
            l.check(&0u8, t0).unwrap();
        }
        assert!(l.check(&0u8, t0).is_err(), "{r:?}");
    }
}
