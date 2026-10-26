#![no_main]
//! Join tickets (d2-tickets): the body decoder and the game server's
//! check. Random bytes never carry a good signature, so the input is also
//! signed as a body with the checker's own key: the check then runs past
//! the signature into the body, realm, node, lifetime, clock and offer
//! checks. A ticket that passes is refused when shown again.
use std::sync::OnceLock;
use std::time::{Duration, SystemTime};

use d2_tickets::body::TicketBody;
use d2_tickets::{OfferedSeat, SignedChecker, TicketError, TicketKey, TicketKeys};
use libfuzzer_sys::fuzz_target;
use ring::signature::Ed25519KeyPair;

/// The process's ticket key (id 1): its pair for signing fuzz bodies and
/// the public keys a checker takes.
fn key() -> &'static (Ed25519KeyPair, TicketKeys) {
    static KEY: OnceLock<(Ed25519KeyPair, TicketKeys)> = OnceLock::new();
    KEY.get_or_init(|| {
        let pkcs8 = TicketKey::generate_pkcs8().expect("random");
        let public = TicketKey::from_pkcs8(1, &pkcs8).expect("key").public();
        let pair = Ed25519KeyPair::from_pkcs8(&pkcs8).expect("key");
        (pair, TicketKeys::new(vec![public]).expect("one key"))
    })
}

fn seat(b: &TicketBody) -> OfferedSeat {
    OfferedSeat {
        game: b.game,
        lease: b.lease,
        player: b.player,
        character: b.character.clone(),
    }
}

fuzz_target!(|data: &[u8]| {
    d2_fuzz::arm("ticket", data);
    let (pair, keys) = key();

    // The body decoder: whatever decodes encodes back to the same bytes
    // (one layout, no slack) and fits the string limits.
    let decoded = TicketBody::decode(data).ok();
    if let Some(b) = &decoded {
        assert!(b.fits());
        assert_eq!(b.encode(), data);
    }

    // The raw bytes as a ticket never pass: no good signature.
    let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_800_000_000);
    let mut checker = SignedChecker::new("d2rust", d2_tickets::NodeId("fi-1".into()), keys.clone());
    assert!(checker.verify(data, now).is_err());

    // The bytes as a body signed with the checker's key, the checker set
    // to the realm and node the body names, holding the offer it asks for,
    // at its issue time: only the fields' own checks remain.
    let mut signed = data.to_vec();
    signed.extend_from_slice(pair.sign(data).as_ref());
    let mut checker = match &decoded {
        Some(b) => {
            let mut c = SignedChecker::new(b.realm.clone(), b.node.clone(), keys.clone());
            c.offer(seat(b), now);
            c
        }
        None => SignedChecker::new("d2rust", d2_tickets::NodeId("fi-1".into()), keys.clone()),
    };
    let at = decoded.as_ref().map_or(now, |b| {
        SystemTime::UNIX_EPOCH + Duration::from_millis(b.issued_ms.min(1 << 50))
    });
    match checker.verify(&signed, at) {
        Ok(claims) => {
            let b = decoded.expect("a passing ticket decodes");
            assert_eq!((claims.game, claims.lease), (b.game, b.lease));
            assert_eq!(claims.character, b.character);
            assert_eq!(checker.offers_waiting(), 0, "a good ticket takes its offer");
            // Shown again it is refused, even with a new offer for its lease.
            assert_eq!(checker.verify(&signed, at), Err(TicketError::Reused));
            checker.offer(seat(&b), at);
            assert_eq!(checker.verify(&signed, at), Err(TicketError::Reused));
        }
        Err(e) => {
            if decoded.is_none() {
                assert!(
                    matches!(e, TicketError::Malformed | TicketError::UnknownKey),
                    "{e:?} for a body that doesn't decode"
                );
            }
        }
    }
});
