//! Abuse tests (plan N26): replay storms against the game server's check
//! and its bounded replay cache, and forged-ticket floods.

use std::time::{Duration, SystemTime};

use crate::body::NONCE_LEN;
use crate::*;

const REALM: &str = "d2rust";
const NODE: &str = "fi-1";

fn at(ms: u64) -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_millis(1_800_000_000_000 + ms)
}

fn grant(lease: u64) -> SeatGrant {
    SeatGrant {
        realm: REALM.into(),
        game: 7,
        node: NodeId(NODE.into()),
        lease: Lease(lease),
        player: PlayerId(900 + lease),
        character: "Amazonia".into(),
        difficulty: 1,
        build: "0.1.1.1162".into(),
    }
}

fn offer(c: &mut SignedChecker, g: &SeatGrant, now: SystemTime) {
    c.offer(
        OfferedSeat {
            game: g.game,
            lease: g.lease,
            player: g.player,
            character: g.character.clone(),
        },
        now,
    );
}

fn new_issuer() -> SignedIssuer {
    SignedIssuer::new(TicketKey::from_pkcs8(1, &TicketKey::generate_pkcs8().unwrap()).unwrap())
}

#[test]
fn a_replay_storm_of_one_ticket_is_refused_every_time() {
    let issuer = new_issuer();
    let mut c = SignedChecker::new(REALM, NodeId(NODE.into()), issuer.public_keys());
    offer(&mut c, &grant(1), at(0));
    let t = issuer.try_issue_at(&grant(1), at(0)).unwrap();
    c.verify(&t, at(1)).unwrap();
    for i in 0..2_000u64 {
        // The main server offering the lease again doesn't revive it.
        if i % 100 == 0 {
            offer(&mut c, &grant(1), at(i));
        }
        assert_eq!(c.verify(&t, at(i)), Err(TicketError::Reused), "replay {i}");
    }
}

#[test]
fn a_flood_of_used_nonces_keeps_the_replay_cache_bounded() {
    // 200,000 used tickets, all still within their lifetime: the cache
    // never holds more than its capacity.
    let mut cache = ReplayCache::new(ReplayCache::DEFAULT_CAPACITY);
    let now = 1_000;
    for i in 0..200_000u64 {
        let mut nonce = [0; NONCE_LEN];
        nonce[..8].copy_from_slice(&i.to_le_bytes());
        cache.insert(nonce, now + 60_000 + i % 997, now);
        assert!(cache.len() <= ReplayCache::DEFAULT_CAPACITY);
    }
    assert_eq!(cache.len(), ReplayCache::DEFAULT_CAPACITY);
    // Once they are all past their time, one more insert empties it.
    cache.insert([0xFF; NONCE_LEN], now + 200_000, now + 100_000);
    assert_eq!(cache.len(), 1);
}

#[test]
fn a_ticket_the_full_cache_forgot_still_cannot_pass_again() {
    // A cache of 8 and 64 joins: most nonces are forgotten, but each
    // forgotten ticket's offer was taken by its first use.
    let issuer = new_issuer();
    let mut c = SignedChecker::with_cache(
        REALM,
        NodeId(NODE.into()),
        issuer.public_keys(),
        ReplayCache::new(8),
    );
    let tickets: Vec<_> = (0..64)
        .map(|l| {
            offer(&mut c, &grant(l), at(0));
            issuer.try_issue_at(&grant(l), at(l)).unwrap()
        })
        .collect();
    for t in &tickets {
        c.verify(t, at(100)).unwrap();
    }
    let mut reused = 0;
    for t in &tickets {
        match c.verify(t, at(200)) {
            Err(TicketError::Reused) => reused += 1,
            Err(TicketError::NoOffer) => {}
            other => panic!("a replayed ticket gave {other:?}"),
        }
    }
    assert_eq!(reused, 8, "the 8 newest are still remembered");
}

#[test]
fn a_flood_of_forged_tickets_changes_nothing() {
    let issuer = new_issuer();
    let mut c = SignedChecker::new(REALM, NodeId(NODE.into()), issuer.public_keys());
    offer(&mut c, &grant(3), at(0));
    let good = issuer.try_issue_at(&grant(3), at(0)).unwrap();
    // Every one-bit change, then truncations and extensions.
    for bit in 0..good.len() * 8 {
        let mut t = good.clone();
        t[bit / 8] ^= 1 << (bit % 8);
        assert!(c.verify(&t, at(1)).is_err(), "bit {bit}");
    }
    for n in 0..good.len() {
        assert!(c.verify(&good[..n], at(1)).is_err());
    }
    let mut long = good.clone();
    long.push(0);
    assert!(c.verify(&long, at(1)).is_err());
    // Another key's tickets for this very seat.
    let other = new_issuer();
    for _ in 0..200 {
        let t = other.try_issue_at(&grant(3), at(0)).unwrap();
        assert_eq!(c.verify(&t, at(1)), Err(TicketError::BadSignature));
    }
    assert_eq!(c.offers_waiting(), 1, "the offer is untouched");
    c.verify(&good, at(2))
        .expect("the real ticket still passes");
}
