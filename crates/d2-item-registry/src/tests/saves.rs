use super::{adopt, adopting, at, realm, save, NOW};
use crate::{DupeHow, Place, Refusal, SaveItems, Split};

#[test]
fn issued_ids_are_accepted_and_kept() {
    let (conn, reg) = realm();
    let b = reg.grant(&conn, 1, NOW).unwrap();
    let v = save(&reg, &conn, 1, "amy", &[b.first, b.first + 1]);
    assert!(v.accepted(), "{v:?}");
    assert_eq!(v.new, vec![b.first, b.first + 1]);
    assert_eq!(
        at(&reg, &conn, b.first),
        Some((Place::Character, "amy".into()))
    );
    // The next save keeps them: nothing new.
    let v = save(&reg, &conn, 1, "amy", &[b.first, b.first + 1]);
    assert!(
        v.accepted() && v.new.is_empty() && v.left.is_empty(),
        "{v:?}"
    );
}

#[test]
fn an_id_never_granted_is_refused() {
    let (conn, reg) = realm();
    let b = reg.grant(&conn, 1, NOW).unwrap();
    let ok = b.first;
    let foreign = b.first + b.count + 100;
    let v = save(&reg, &conn, 1, "amy", &[ok, foreign]);
    assert_eq!(v.refused, vec![(foreign, Refusal::NotGranted)]);
    // Nothing of a refused save is written.
    assert_eq!(at(&reg, &conn, ok), None);
}

#[test]
fn another_games_block_is_not_granted() {
    let (conn, reg) = realm();
    reg.grant(&conn, 1, NOW).unwrap();
    let other = reg.grant(&conn, 2, NOW).unwrap();
    let v = save(&reg, &conn, 1, "amy", &[other.first]);
    assert_eq!(v.refused, vec![(other.first, Refusal::NotGranted)]);
}

#[test]
fn a_dupe_across_two_characters_is_refused() {
    let (conn, reg) = realm();
    let b1 = reg.grant(&conn, 1, NOW).unwrap();
    reg.grant(&conn, 2, NOW).unwrap();
    let x = b1.first;
    assert!(save(&reg, &conn, 1, "amy", &[x]).accepted());
    // Bob, in another game, comes back with a copy of Amy's item.
    let v = save(&reg, &conn, 2, "bob", &[x]);
    assert_eq!(v.refused.len(), 1);
    match &v.refused[0] {
        (id, Refusal::HeldElsewhere(w)) => {
            assert_eq!(*id, x);
            assert_eq!(
                (w.place, w.place_id.as_str(), w.game),
                (Place::Character, "amy", Some(1))
            );
        }
        r => panic!("{r:?}"),
    }
    // The save checks' hook names the holder too.
    let holder = reg
        .other_holder(&conn, 2, "bob", Split::DEFAULT, x)
        .unwrap()
        .expect("held");
    assert_eq!(holder.place_id, "amy");
    assert_eq!(
        reg.other_holder(&conn, 1, "amy", Split::DEFAULT, x)
            .unwrap(),
        None
    );
    assert_eq!(at(&reg, &conn, x), Some((Place::Character, "amy".into())));
}

#[test]
fn a_dupe_of_an_offline_characters_item_is_refused() {
    let (conn, reg) = realm();
    let b = reg.grant(&conn, 1, NOW).unwrap();
    let x = b.first;
    assert!(save(&reg, &conn, 1, "amy", &[x]).accepted());
    reg.left(&conn, 1, "amy").unwrap();
    // Amy left game 1 with the item: Bob, still in game 1, can't take it on.
    let v = save(&reg, &conn, 1, "bob", &[x]);
    assert!(
        matches!(v.refused[..], [(id, Refusal::HeldElsewhere(_))] if id == x),
        "{v:?}"
    );
}

#[test]
fn a_trade_in_game_moves_the_id() {
    let (conn, reg) = realm();
    let b = reg.grant(&conn, 1, NOW).unwrap();
    let x = b.first;
    assert!(save(&reg, &conn, 1, "amy", &[x]).accepted());
    // Amy gives the item to Bob; Bob's save comes in before Amy's.
    let v = save(&reg, &conn, 1, "bob", &[x]);
    assert!(v.accepted(), "{v:?}");
    assert_eq!(v.moved, vec![(x, "amy".to_owned())]);
    assert_eq!(v.contested, vec![(x, "amy".to_owned())]);
    let v = save(&reg, &conn, 1, "amy", &[]);
    assert!(v.accepted() && v.left.is_empty(), "{v:?}");
    assert_eq!(at(&reg, &conn, x), Some((Place::Character, "bob".into())));
    assert!(reg.close_game(&conn, 1, NOW).unwrap().is_empty());
}

#[test]
fn a_dropped_item_picked_up_in_the_game_moves() {
    let (conn, reg) = realm();
    let b = reg.grant(&conn, 1, NOW).unwrap();
    let x = b.first;
    assert!(save(&reg, &conn, 1, "amy", &[x]).accepted());
    // Amy drops it and saves; Bob picks it up later.
    let v = save(&reg, &conn, 1, "amy", &[]);
    assert_eq!(v.left, vec![x]);
    assert_eq!(at(&reg, &conn, x), Some((Place::Gone, "amy".into())));
    let v = save(&reg, &conn, 1, "bob", &[x]);
    assert_eq!(v.moved, vec![(x, "amy".to_owned())]);
    assert!(v.contested.is_empty());
    // After the game closes, a dropped item is gone for good.
    let y = b.first + 1;
    assert!(save(&reg, &conn, 1, "bob", &[x, y]).accepted());
    assert_eq!(save(&reg, &conn, 1, "bob", &[x]).left, vec![y]);
    reg.close_game(&conn, 1, NOW).unwrap();
    reg.grant(&conn, 2, NOW).unwrap();
    let v = save(&reg, &conn, 2, "cid", &[y]);
    assert!(
        matches!(v.refused[..], [(id, Refusal::HeldElsewhere(_))] if id == y),
        "{v:?}"
    );
}

#[test]
fn a_dupe_inside_one_game_is_found_when_it_closes() {
    let (conn, reg) = realm();
    let b = reg.grant(&conn, 1, NOW).unwrap();
    let x = b.first;
    assert!(save(&reg, &conn, 1, "amy", &[x]).accepted());
    // A duplicated item: both keep it in their last saves.
    assert!(save(&reg, &conn, 1, "bob", &[x]).accepted());
    assert!(save(&reg, &conn, 1, "amy", &[x]).accepted());
    let dupes = reg.close_game(&conn, 1, NOW).unwrap();
    assert_eq!(dupes.len(), 1);
    let d = &dupes[0];
    assert_eq!((d.id, d.game, d.how), (x, Some(1), DupeHow::InGame));
    // The registry keeps it where the last save put it (Amy), Bob is flagged.
    assert_eq!(
        (d.kept.as_str(), d.also.clone()),
        ("amy", vec!["bob".to_owned()])
    );
    let stored = reg.dupes(&conn).unwrap();
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].also, vec!["bob".to_owned()]);
}

#[test]
fn an_id_twice_in_one_save_is_refused() {
    let (conn, reg) = realm();
    let b = reg.grant(&conn, 1, NOW).unwrap();
    let v = save(&reg, &conn, 1, "amy", &[b.first, b.first]);
    assert_eq!(v.refused, vec![(b.first, Refusal::Twice)]);
}

#[test]
fn ids_of_another_split_are_refused() {
    let (conn, reg) = realm();
    let b = reg.grant(&conn, 1, NOW).unwrap();
    assert!(save(&reg, &conn, 1, "amy", &[b.first]).accepted());
    let hardcore = Split {
        hardcore: true,
        ..Split::DEFAULT
    };
    let v = reg
        .accept_save(
            &conn,
            &SaveItems {
                game: 1,
                character: "amy",
                split: hardcore,
                ids: &[b.first],
                now: NOW,
            },
        )
        .unwrap();
    assert_eq!(
        v.refused,
        vec![(
            b.first,
            Refusal::EconomyMismatch {
                split: "sc-l0-exp".into()
            }
        )]
    );
    assert_eq!(v.refused[0].1.transfer_code(), 89);
}

#[test]
fn first_sight_registers_and_flags_dupes() {
    let (conn, reg) = realm();
    // Two v1 characters; Bob's record carries a copy of Amy's item 77.
    let a = adopt(&reg, &conn, "amy", &[77, 78, 0]);
    assert_eq!(a.registered, vec![77, 78]);
    let b = adopt(&reg, &conn, "bob", &[77, 79]);
    assert_eq!(b.registered, vec![79]);
    assert_eq!(b.dupes.len(), 1);
    assert_eq!((b.dupes[0].0, b.dupes[0].1.place_id.as_str()), (77, "amy"));
    // Seen again, nothing changes.
    assert_eq!(adopt(&reg, &conn, "amy", &[77, 78]).known, vec![77, 78]);
    // Kept on the first character; the duplicate is listed.
    assert_eq!(at(&reg, &conn, 77), Some((Place::Character, "amy".into())));
    let d = reg.dupes(&conn).unwrap();
    assert_eq!(d.len(), 1);
    assert_eq!((d[0].kept.as_str(), d[0].how), ("amy", DupeHow::FirstSight));
    // Amy plays on with her old items; Bob's save with the copy is refused.
    reg.grant(&conn, 1, NOW).unwrap();
    assert!(save(&reg, &conn, 1, "amy", &[77, 78]).accepted());
    reg.grant(&conn, 2, NOW).unwrap();
    assert!(!save(&reg, &conn, 2, "bob", &[77, 79]).accepted());
}

#[test]
fn unknown_ids_are_adopted_and_flagged_when_configured() {
    let (conn, reg) = adopting();
    reg.grant(&conn, 1, NOW).unwrap();
    let v = save(&reg, &conn, 1, "amy", &[0xDEAD_BEEF]);
    assert!(v.accepted() && v.flagged(), "{v:?}");
    assert_eq!(v.adopted, vec![0xDEAD_BEEF]);
    // Adopted is still unique: a copy in another game is refused.
    reg.grant(&conn, 2, NOW).unwrap();
    assert!(!save(&reg, &conn, 2, "bob", &[0xDEAD_BEEF]).accepted());
}

#[test]
fn history_follows_an_id() {
    let (conn, reg) = realm();
    let b = reg.grant(&conn, 1, NOW).unwrap();
    let x = b.first;
    save(&reg, &conn, 1, "amy", &[x]);
    save(&reg, &conn, 1, "bob", &[x]);
    save(&reg, &conn, 1, "bob", &[]);
    let h = reg.history(&conn, x).unwrap();
    let steps: Vec<_> = h
        .iter()
        .map(|s| (s.place, s.place_id.as_str(), s.game, s.why.as_str()))
        .collect();
    assert_eq!(
        steps,
        vec![
            (Place::Character, "amy", Some(1), "new"),
            (Place::Character, "bob", Some(1), "trade"),
            (Place::Gone, "bob", Some(1), "left"),
        ]
    );
}
