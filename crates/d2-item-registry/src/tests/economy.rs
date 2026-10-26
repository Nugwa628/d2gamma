use super::{at, realm, save, NOW};
use crate::{Error, Place, Refusal, Split};

#[test]
fn a_stash_put_and_take_follow_the_id() {
    let (conn, reg) = realm();
    let b = reg.grant(&conn, 1, NOW).unwrap();
    let x = b.first;
    assert!(save(&reg, &conn, 1, "amy", &[x]).accepted());
    // Amy puts it into her account's shared stash.
    reg.arriving(&conn, Split::DEFAULT, x, "amy", 1, NOW)
        .unwrap();
    reg.moved(&conn, x, Place::Stash, "account 7", None, "stash", NOW)
        .unwrap();
    assert_eq!(at(&reg, &conn, x), Some((Place::Stash, "account 7".into())));
    // Her transfer save is the record without it: nothing left her.
    assert!(save(&reg, &conn, 1, "amy", &[]).left.is_empty());
    // A copy in Bob's save of the same game is a dupe of the stash item.
    let v = save(&reg, &conn, 1, "bob", &[x]);
    assert!(
        matches!(&v.refused[..], [(id, Refusal::HeldElsewhere(w))] if *id == x && w.place == Place::Stash),
        "{v:?}"
    );
    // Bob, another character of the account, takes it in game 2.
    reg.grant(&conn, 2, NOW).unwrap();
    reg.moved(&conn, x, Place::InFlight, "bob", Some(2), "take", NOW)
        .unwrap();
    let v = save(&reg, &conn, 2, "bob", &[x]);
    assert!(v.accepted(), "{v:?}");
    assert_eq!(at(&reg, &conn, x), Some((Place::Character, "bob".into())));
    let whys: Vec<_> = reg
        .history(&conn, x)
        .unwrap()
        .into_iter()
        .map(|s| s.why)
        .collect();
    assert_eq!(whys, ["new", "stash", "take", "arrived"]);
}

#[test]
fn a_new_item_put_before_any_save_is_registered() {
    let (conn, reg) = realm();
    let b = reg.grant(&conn, 1, NOW).unwrap();
    reg.arriving(&conn, Split::DEFAULT, b.first, "amy", 1, NOW)
        .unwrap();
    assert_eq!(
        at(&reg, &conn, b.first),
        Some((Place::Character, "amy".into()))
    );
}

#[test]
fn a_put_of_a_foreign_or_held_item_is_refused() {
    let (conn, reg) = realm();
    let b = reg.grant(&conn, 1, NOW).unwrap();
    reg.grant(&conn, 2, NOW).unwrap();
    assert!(save(&reg, &conn, 1, "amy", &[b.first]).accepted());
    // Bob, in game 2, puts a copy of Amy's item.
    let e = reg
        .arriving(&conn, Split::DEFAULT, b.first, "bob", 2, NOW)
        .unwrap_err();
    assert!(matches!(e, Error::Refused { id, why: Refusal::HeldElsewhere(_) } if id == b.first));
    // An invented ID.
    let e = reg
        .arriving(&conn, Split::DEFAULT, 0x7777_0000, "bob", 2, NOW)
        .unwrap_err();
    let Error::Refused { why, .. } = e else {
        panic!("{e:?}")
    };
    assert_eq!(
        (why.clone(), why.transfer_code()),
        (Refusal::NotGranted, 109)
    );
}

#[test]
fn moving_an_unregistered_id_fails() {
    let (conn, reg) = realm();
    assert_eq!(
        reg.moved(&conn, 5, Place::Stash, "account 1", None, "stash", NOW),
        Err(Error::Unregistered(5))
    );
}
