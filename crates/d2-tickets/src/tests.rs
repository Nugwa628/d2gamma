use std::time::{Duration, SystemTime};

use super::body::{TicketBody, MAX_BUILD, MAX_CHARACTER, MAX_NODE, MAX_REALM, MAX_TICKET};
use super::*;

const REALM: &str = "d2rust";
const NODE: &str = "fi-1";

fn at(s: u64) -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_secs(1_800_000_000 + s)
}

fn key(id: u8) -> TicketKey {
    TicketKey::from_pkcs8(id, &TicketKey::generate_pkcs8().unwrap()).unwrap()
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

fn seat(g: &SeatGrant) -> OfferedSeat {
    OfferedSeat {
        game: g.game,
        lease: g.lease,
        player: g.player,
        character: g.character.clone(),
    }
}

/// An issuer with key 1 and a checker of game server `NODE` taking it, with
/// an offer for `grant(lease)` made at `at(0)`.
fn setup(lease: u64) -> (SignedIssuer, SignedChecker) {
    let issuer = SignedIssuer::new(key(1));
    let mut checker = SignedChecker::new(REALM, NodeId(NODE.into()), issuer.public_keys());
    checker.offer(seat(&grant(lease)), at(0));
    (issuer, checker)
}

fn ticket(issuer: &SignedIssuer, g: &SeatGrant) -> Vec<u8> {
    issuer.try_issue_at(g, at(0)).unwrap()
}

/// A ticket for `b` signed with `k`, whatever its fields say.
fn signed(k: &TicketKey, b: &TicketBody) -> Vec<u8> {
    let mut t = b.encode();
    let sig = k.sign(&t);
    t.extend_from_slice(sig.as_ref());
    t
}

#[test]
fn round_trip() {
    let (issuer, mut checker) = setup(5);
    let t = ticket(&issuer, &grant(5));
    let claims = checker.verify(&t, at(1)).unwrap();
    assert_eq!(
        claims,
        TicketClaims {
            game: 7,
            lease: Lease(5),
            character: "Amazonia".into()
        }
    );
    assert_eq!(checker.offers_waiting(), 0, "a good ticket takes its offer");

    let (signed, _) = body::split(&t).unwrap();
    let b = TicketBody::decode(signed).unwrap();
    assert_eq!(
        (b.key_id, b.realm.as_str(), b.node.0.as_str()),
        (1, REALM, NODE)
    );
    assert_eq!(
        (b.player, b.difficulty, b.build.as_str()),
        (PlayerId(905), 1, "0.1.1.1162")
    );
    assert_eq!(b.expires_ms - b.issued_ms, 30_000);
}

#[test]
fn the_trait_answers_ticket_invalid() {
    let (issuer, mut checker) = setup(5);
    let t = issuer.issue(&grant(5));
    let check: &mut dyn TicketCheck = &mut checker;
    assert!(check.check(&t, SystemTime::now()).is_ok());
    assert_eq!(
        check.check(&t, SystemTime::now()),
        Err(ErrorCode::TICKET_INVALID)
    );
}

#[test]
fn every_tampered_byte_is_refused() {
    let (issuer, mut checker) = setup(5);
    let t = ticket(&issuer, &grant(5));
    for i in 0..t.len() {
        for flip in [0x01, 0x80] {
            let mut bad = t.clone();
            bad[i] ^= flip;
            let e = checker.verify(&bad, at(1)).unwrap_err();
            assert!(
                matches!(
                    e,
                    TicketError::Malformed | TicketError::UnknownKey | TicketError::BadSignature
                ),
                "byte {i} ^ {flip:#x}: {e:?}"
            );
        }
    }
    assert!(checker.verify(&t[..t.len() - 1], at(1)).is_err(), "cut");
    assert!(
        checker
            .verify(&[t.clone(), vec![0]].concat(), at(1))
            .is_err(),
        "extended"
    );
    assert_eq!(checker.verify(&[], at(1)), Err(TicketError::Malformed));
    assert!(
        checker.verify(&t, at(1)).is_ok(),
        "the refusals changed nothing"
    );
}

#[test]
fn another_game_servers_ticket_is_refused() {
    let (issuer, mut checker) = setup(5);
    let mut g = grant(5);
    g.node = NodeId("fi-2".into());
    assert_eq!(
        checker.verify(&ticket(&issuer, &g), at(1)),
        Err(TicketError::WrongNode)
    );

    let mut g = grant(5);
    g.realm = "pd2".into();
    assert_eq!(
        checker.verify(&ticket(&issuer, &g), at(1)),
        Err(TicketError::WrongRealm)
    );
    assert_eq!(checker.offers_waiting(), 1);
}

#[test]
fn an_expired_ticket_is_refused() {
    let (issuer, mut checker) = setup(5);
    let t = ticket(&issuer, &grant(5));
    // Good until its expiry (30 s) plus the clock slack (30 s).
    let last = at(60);
    assert_eq!(
        checker.verify(&t, last + Duration::from_millis(1)),
        Err(TicketError::Expired)
    );
    assert_eq!(checker.verify(&t, at(3600)), Err(TicketError::Expired));
    checker.offer(seat(&grant(5)), at(59));
    assert!(checker.verify(&t, last).is_ok());
}

#[test]
fn a_ticket_from_the_future_is_refused() {
    let (issuer, mut checker) = setup(5);
    let t = issuer.try_issue_at(&grant(5), at(31)).unwrap();
    assert_eq!(checker.verify(&t, at(0)), Err(TicketError::NotYetValid));
    assert!(checker.verify(&t, at(1)).is_ok(), "within the clock slack");
}

#[test]
fn a_ticket_with_another_lifetime_is_refused() {
    let k = key(1);
    let mut checker = SignedChecker::new(
        REALM,
        NodeId(NODE.into()),
        TicketKeys::new(vec![k.public()]).unwrap(),
    );
    checker.offer(seat(&grant(5)), at(0));
    let t = ticket(&SignedIssuer::new(key(9)), &grant(5));
    let (s, _) = body::split(&t).unwrap();
    let mut b = TicketBody::decode(s).unwrap();
    b.key_id = 1;
    b.expires_ms += 3_600_000;
    assert_eq!(
        checker.verify(&signed(&k, &b), at(1)),
        Err(TicketError::BadLifetime)
    );
    b.expires_ms = b.issued_ms - 1;
    assert_eq!(
        checker.verify(&signed(&k, &b), at(1)),
        Err(TicketError::BadLifetime)
    );
}

#[test]
fn a_used_ticket_is_refused() {
    let (issuer, mut checker) = setup(5);
    let t = ticket(&issuer, &grant(5));
    assert!(checker.verify(&t, at(1)).is_ok());
    assert_eq!(checker.verify(&t, at(2)), Err(TicketError::Reused));
    // Even with the seat offered again (a new lease would be a new ticket).
    checker.offer(seat(&grant(5)), at(2));
    assert_eq!(checker.verify(&t, at(3)), Err(TicketError::Reused));
}

#[test]
fn a_ticket_of_an_unknown_key_is_refused() {
    let (_, mut checker) = setup(5);
    let other = SignedIssuer::new(key(2));
    assert_eq!(
        checker.verify(&ticket(&other, &grant(5)), at(1)),
        Err(TicketError::UnknownKey)
    );
    // Same id, another key.
    let forger = SignedIssuer::new(key(1));
    assert_eq!(
        checker.verify(&ticket(&forger, &grant(5)), at(1)),
        Err(TicketError::BadSignature)
    );
}

#[test]
fn a_ticket_without_its_offer_waits_for_it() {
    let (issuer, mut checker) = setup(5);
    let t = ticket(&issuer, &grant(6));
    let e = checker.verify(&t, at(1)).unwrap_err();
    assert_eq!(e, TicketError::NoOffer);
    assert!(e.retry());
    // The offer arrives: the same ticket passes (the refusal didn't use it).
    checker.offer(seat(&grant(6)), at(2));
    assert!(checker.verify(&t, at(3)).is_ok());
    assert!(!TicketError::Reused.retry());
}

#[test]
fn an_offer_for_another_seat_is_refused() {
    let (issuer, mut checker) = setup(5);
    let mut g = grant(5);
    g.character = "Sorc".into();
    assert_eq!(
        checker.verify(&ticket(&issuer, &g), at(1)),
        Err(TicketError::OfferMismatch)
    );
    let mut g = grant(5);
    g.game = 8;
    assert_eq!(
        checker.verify(&ticket(&issuer, &g), at(1)),
        Err(TicketError::OfferMismatch)
    );
    let mut g = grant(5);
    g.player = PlayerId(1);
    assert_eq!(
        checker.verify(&ticket(&issuer, &g), at(1)),
        Err(TicketError::OfferMismatch)
    );
    assert!(checker.verify(&ticket(&issuer, &grant(5)), at(1)).is_ok());
}

#[test]
fn offers_expire_and_close_with_their_game() {
    let (issuer, mut checker) = setup(5);
    checker.offer(seat(&grant(6)), at(20));
    let t = issuer.try_issue_at(&grant(5), at(25)).unwrap();
    assert_eq!(checker.verify(&t, at(31)), Err(TicketError::OfferExpired));
    assert_eq!(checker.expire_offers(at(31)), vec![seat(&grant(5))]);
    assert_eq!(checker.offers_waiting(), 1);
    checker.game_closed(7);
    assert_eq!(checker.offers_waiting(), 0);
    checker.offer(seat(&grant(7)), at(40));
    assert_eq!(checker.withdraw(Lease(7)), Some(seat(&grant(7))));
}

#[test]
fn key_rotation() {
    let mut issuer = SignedIssuer::new(key(1));
    let next = key(2);
    // Publish the next key first: game servers take both.
    issuer = issuer.with_other(next.public()).unwrap();
    let both = issuer.public_keys();
    assert_eq!(both.iter().map(|k| k.id).collect::<Vec<_>>(), vec![1, 2]);
    let mut checker = SignedChecker::new(REALM, NodeId(NODE.into()), both);
    for l in 1..=3 {
        checker.offer(seat(&grant(l)), at(0));
    }
    let old = ticket(&issuer, &grant(1));
    issuer.rotate(next).unwrap();
    assert_eq!(issuer.key_id(), 2);
    let new = ticket(&issuer, &grant(2));
    assert!(checker.verify(&old, at(1)).is_ok());
    assert!(checker.verify(&new, at(1)).is_ok());

    // The old key dropped: its tickets are refused.
    issuer.drop_other();
    checker.set_keys(issuer.public_keys());
    let old = ticket(&SignedIssuer::new(key(1)), &grant(3));
    assert_eq!(checker.verify(&old, at(1)), Err(TicketError::UnknownKey));
    assert!(checker.verify(&ticket(&issuer, &grant(3)), at(1)).is_ok());

    assert!(issuer.rotate(key(2)).is_err(), "a new key needs a new id");
    let p = |id| key(id).public();
    assert!(TicketKeys::new(vec![p(1), p(2), p(3)]).is_err());
    assert!(TicketKeys::new(vec![p(1), p(1)]).is_err());
}

#[test]
fn tickets_fit_their_limits() {
    let issuer = SignedIssuer::new(key(1));
    let g = SeatGrant {
        realm: "r".repeat(MAX_REALM),
        node: NodeId("n".repeat(MAX_NODE)),
        character: "c".repeat(MAX_CHARACTER),
        build: "b".repeat(MAX_BUILD),
        ..grant(5)
    };
    assert_eq!(ticket(&issuer, &g).len(), MAX_TICKET);
    assert_eq!(MAX_TICKET, 190);

    let mut long = g.clone();
    long.character.push('c');
    assert_eq!(issuer.try_issue_at(&long, at(0)), Err(IssueError::TooLong));
    let empty = issuer.issue(&long);
    assert!(empty.is_empty());
    let (_, mut checker) = setup(5);
    assert_eq!(checker.verify(&empty, at(1)), Err(TicketError::Malformed));
}

#[test]
fn the_replay_cache_is_bounded() {
    let mut c = ReplayCache::new(2);
    c.insert([1; 16], 100, 0);
    c.insert([2; 16], 50, 0);
    c.insert([3; 16], 200, 0);
    assert_eq!(c.len(), 2);
    assert!(!c.seen(&[2; 16]), "the one closest to its time goes first");
    assert!(c.seen(&[1; 16]) && c.seen(&[3; 16]));
    c.prune(150);
    assert!(!c.seen(&[1; 16]) && c.seen(&[3; 16]));
    c.prune(201);
    assert!(c.is_empty());
}

#[test]
fn a_forgotten_ticket_still_cant_be_used_twice() {
    let issuer = SignedIssuer::new(key(1));
    let mut checker = SignedChecker::with_cache(
        REALM,
        NodeId(NODE.into()),
        issuer.public_keys(),
        ReplayCache::new(1),
    );
    checker.offer(seat(&grant(1)), at(0));
    checker.offer(seat(&grant(2)), at(0));
    let first = ticket(&issuer, &grant(1));
    assert!(checker.verify(&first, at(1)).is_ok());
    assert!(checker.verify(&ticket(&issuer, &grant(2)), at(1)).is_ok());
    // The cache of one forgot the first ticket; its offer is gone.
    assert_eq!(checker.verify(&first, at(2)), Err(TicketError::NoOffer));
}

#[test]
fn key_files() {
    let dir = std::env::temp_dir().join(format!("d2-tickets-keys-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let made = TicketKey::create(&dir, 3).unwrap();
    assert!(TicketKey::file(&dir, 3).ends_with("ticket-3.key"));
    let loaded = TicketKey::load(&dir, 3).unwrap();
    assert_eq!(made.public(), loaded.public());
    assert_eq!(loaded.id(), 3);
    assert!(
        TicketKey::create(&dir, 3).is_err(),
        "a key file is never replaced"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(TicketKey::file(&dir, 3))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }
    std::fs::write(TicketKey::file(&dir, 4), b"not a key").unwrap();
    assert!(matches!(TicketKey::load(&dir, 4), Err(KeyError::BadKey)));
    std::fs::remove_dir_all(&dir).unwrap();
}

/// Lowercase hex of `b`.
fn hex(b: &[u8]) -> String {
    b.iter().map(|b| format!("{b:02x}")).collect()
}

#[test]
fn no_key_bytes_in_debug_output() {
    let pkcs8 = TicketKey::generate_pkcs8().unwrap();
    let k = TicketKey::from_pkcs8(1, &pkcs8).unwrap();
    let public = k.public();
    let issuer = SignedIssuer::new(TicketKey::from_pkcs8(1, &pkcs8).unwrap());
    let checker = SignedChecker::new(REALM, NodeId(NODE.into()), issuer.public_keys());
    let shown = format!(
        "{k:?} {k:#?} {public:?} {:?} {issuer:?} {issuer:#?} {checker:?} {checker:#?}",
        issuer.public_keys()
    );
    // Any 8 bytes in a row of the key file or the public key, as hex or as
    // a byte list, would be a leak.
    for secret in [&pkcs8[..], &public.bytes[..]] {
        for w in secret.windows(8) {
            assert!(!shown.contains(&hex(w)), "hex bytes in {shown}");
            let list = w.iter().map(u8::to_string).collect::<Vec<_>>().join(", ");
            assert!(!shown.contains(&list), "byte list in {shown}");
        }
    }
}

#[test]
fn only_a_tickets_first_two_bytes_are_shown() {
    let (issuer, _) = setup(5);
    let t = ticket(&issuer, &grant(5));
    assert_eq!(ticket_tag(&t).to_string(), "ticket 01:01..");
    assert_eq!(format!("{:?}", ticket_tag(&t[..1])), "ticket 01:--..");
    assert_eq!(ticket_tag(&[]).to_string(), "ticket --:--..");
    let e = format!("{} {:?}", TicketError::Reused, TicketError::Reused);
    assert!(!e.contains(&hex(&t[2..10])));
}

/// The library logs nothing itself, so no key or ticket can reach a log
/// through it: no print or log macro in any source file but the tests.
#[test]
fn the_sources_log_nothing() {
    let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let banned = [
        "println!",
        "eprintln!",
        "print!(",
        "eprint!(",
        "dbg!",
        "log::",
        "tracing::",
    ];
    let mut seen = 0;
    for entry in std::fs::read_dir(&src).unwrap() {
        let path = entry.unwrap().path();
        if path.file_name().is_some_and(|n| n == "tests.rs") {
            continue;
        }
        let text = std::fs::read_to_string(&path).unwrap();
        for b in banned {
            assert!(!text.contains(b), "{} uses {b}", path.display());
        }
        seen += 1;
    }
    assert!(seen >= 7, "read {seen} source files");
}
