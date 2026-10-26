//! The item transfers' round trips and their decode rules.

use super::super::account::CharSummary;
use super::super::node::{kind, NodeToRealm, RealmToNode, SaveInfo};
use super::super::ErrorCode;
use super::*;
use crate::wire::MAX_FRAME;

fn save(seq: u32) -> SaveInfo {
    SaveInfo {
        game: 4,
        lease: 77,
        seq,
        reason: SAVE_REASON_TRANSFER,
        journal_tick: 1234,
        summary: CharSummary {
            name: "Kashya".into(),
            class: 0,
            level: 12,
            status: 0x20,
            difficulties: 1,
            look: vec![1; 16],
            last_played: 99,
            season: 0,
        },
    }
}

fn sketch() -> ItemSketch {
    ItemSketch {
        code: "cap".into(),
        item_type: "helm".into(),
        quality: 7,
        level_req: 25,
        ilvl: 40,
        sockets: 1,
        ethereal: true,
        class: Some(3),
        size: (2, 2),
    }
}

fn out() -> ItemOut {
    ItemOut {
        save: save(9),
        req: 5,
        to: goods::ITEM,
        unit: 31,
        item_id: Some(0x0100_0042),
        stack: None,
        gold: 0,
        sketch: Some(sketch()),
        item: vec![0x10; 40],
        credits: Vec::new(),
    }
}

fn item_in() -> ItemIn {
    ItemIn {
        game: 4,
        lease: 77,
        req: 6,
        from: goods::ITEM,
        item_id: Some(0x0100_0042),
        stack: None,
        gold: 0,
        ids: None,
        place: 1,
        item: vec![0x20; 40],
    }
}

#[test]
fn transfers_round_trip_in_their_range() {
    let wanted = ItemWanted {
        game: 4,
        lease: 77,
        req: 5,
        unit: 31,
        to: goods::ITEM,
        count: 0,
        gold: 0,
    };
    let gold_out = ItemOut {
        to: goods::GOLD,
        unit: 0,
        item_id: None,
        gold: 5000,
        sketch: None,
        item: Vec::new(),
        ..out()
    };
    let stack_in = ItemIn {
        from: goods::STACK,
        item_id: None,
        stack: Some(("r01".into(), 3)),
        ids: Some(ItemIdBlock {
            first: 100,
            count: 3,
        }),
        item: Vec::new(),
        ..item_in()
    };
    let mut kinds = std::collections::BTreeSet::new();
    for m in [
        NodeToRealm::ItemOut(out()),
        NodeToRealm::ItemOut(gold_out),
        NodeToRealm::ItemInDone(ItemInDone {
            lease: 77,
            req: 6,
            result: placed::PLACED,
            save: Some(save(10)),
        }),
        NodeToRealm::ItemInDone(ItemInDone {
            lease: 77,
            req: 7,
            result: placed::NO_ROOM,
            save: None,
        }),
        NodeToRealm::ItemOutRefused {
            lease: 77,
            req: 8,
            code: ErrorCode(82),
        },
    ] {
        let body = m.body();
        assert_eq!(body[0], m.kind());
        assert!(kind::TRANSFER.contains(&body[0]));
        assert_eq!(NodeToRealm::parse(&body).unwrap(), m);
        assert!(RealmToNode::parse(&body).is_err());
        kinds.insert(body[0]);
    }
    for m in [
        RealmToNode::ItemWanted(wanted),
        RealmToNode::ItemOutAck {
            lease: 77,
            req: 5,
            code: ErrorCode(80),
        },
        RealmToNode::ItemIn(item_in()),
        RealmToNode::ItemIn(stack_in),
    ] {
        let body = m.body();
        assert_eq!(body[0], m.kind());
        assert!(kind::TRANSFER.contains(&body[0]));
        assert_eq!(RealmToNode::parse(&body).unwrap(), m);
        assert!(NodeToRealm::parse(&body).is_err());
        kinds.insert(body[0]);
    }
    assert_eq!(kinds, (0xD0..=0xD5).collect());
}

/// The largest item, with every optional field, fits one link frame.
#[test]
fn largest_transfers_fit_a_frame() {
    let big = ItemOut {
        item: vec![0xAB; MAX_ITEM],
        ..out()
    };
    let big_in = ItemIn {
        item: vec![0xAB; MAX_ITEM],
        ..item_in()
    };
    assert!(NodeToRealm::ItemOut(big.clone()).body().len() <= MAX_FRAME);
    let credited = ItemOut {
        credits: (0..MAX_CREDITS)
            .map(|i| (format!("c{i:07}"), u32::MAX))
            .collect(),
        ..big
    };
    assert!(NodeToRealm::ItemOut(credited).body().len() <= MAX_FRAME);
    assert!(RealmToNode::ItemIn(big_in).body().len() <= MAX_FRAME);
}

#[test]
fn decode_refuses_bad_fields() {
    let bad = |m: ItemOut| NodeToRealm::parse(&NodeToRealm::ItemOut(m).body()).is_err();
    // Not a transfer save, unknown goods, a bad size, an item too long.
    let mut s = save(9);
    s.reason = 0;
    assert!(bad(ItemOut { save: s, ..out() }));
    assert!(bad(ItemOut { to: 4, ..out() }));
    assert!(bad(ItemOut {
        sketch: Some(ItemSketch {
            size: (5, 1),
            ..sketch()
        }),
        ..out()
    }));
    assert!(bad(ItemOut {
        sketch: Some(ItemSketch {
            code: String::new(),
            ..sketch()
        }),
        ..out()
    }));
    assert!(bad(ItemOut {
        item: vec![0; MAX_ITEM + 1],
        ..out()
    }));
    // Placed needs its save; not placed has none; the save is the seat's.
    let done = |result, save| {
        NodeToRealm::parse(
            &NodeToRealm::ItemInDone(ItemInDone {
                lease: 77,
                req: 1,
                result,
                save,
            })
            .body(),
        )
    };
    assert!(done(placed::PLACED, None).is_err());
    assert!(done(placed::NO_ROOM, Some(save(3))).is_err());
    assert!(done(3, None).is_err());
    let mut other = save(3);
    other.lease = 78;
    assert!(done(placed::PLACED, Some(other)).is_err());
    assert!(done(placed::PLACED, Some(save(3))).is_ok());
    // An unknown source.
    let src = RealmToNode::ItemIn(ItemIn {
        from: 0,
        ..item_in()
    });
    assert!(RealmToNode::parse(&src.body()).is_err());
}

/// A mod game's credits ride after the item; without them the bytes are
/// D2Gamma's as before (an old `ItemOut` decodes with none).
#[test]
fn item_out_credits_are_a_trailing_field() {
    let plain = NodeToRealm::ItemOut(out()).body();
    let with = ItemOut {
        credits: vec![("r01".into(), 3), ("oil".into(), 1)],
        ..out()
    };
    let body = NodeToRealm::ItemOut(with.clone()).body();
    assert_eq!(&body[..plain.len()], &plain[..]);
    assert_eq!(body.len(), plain.len() + 1 + (1 + 3 + 4) * 2);
    assert_eq!(
        NodeToRealm::parse(&body).unwrap(),
        NodeToRealm::ItemOut(with)
    );
    assert_eq!(
        NodeToRealm::parse(&plain).unwrap(),
        NodeToRealm::ItemOut(out())
    );
    // An empty list, an empty code or a count of 0 is malformed.
    let mut empty = plain.clone();
    empty.push(0);
    assert!(NodeToRealm::parse(&empty).is_err());
    let bad = |credits: Vec<(String, u32)>| {
        NodeToRealm::parse(&NodeToRealm::ItemOut(ItemOut { credits, ..out() }).body()).is_err()
    };
    assert!(bad(vec![(String::new(), 1)]));
    assert!(bad(vec![("r01".into(), 0)]));
}
