//! The stash messages' round trips and their decode rules.

use super::*;
use crate::wire::MAX_FRAME;

fn cell() -> StashCell {
    StashCell {
        page: 2,
        x: 9,
        y: 7,
    }
}

#[test]
fn requests_round_trip() {
    for m in [
        StashRequest::Open {
            tab: tab::ITEMS,
            page: 3,
        },
        StashRequest::Put {
            req: 1 << 40,
            unit: 77,
            to: goods::ITEM,
            gold: 0,
            cell: Some(cell()),
        },
        StashRequest::Put {
            req: 2,
            unit: 0,
            to: goods::GOLD,
            gold: 12_345,
            cell: None,
        },
        StashRequest::PutStacks {
            req: 3,
            units: vec![4, 5, 6],
        },
        StashRequest::Take {
            req: 4,
            from: goods::ITEM,
            item: Some(0xDEAD_BEEF),
            stack: None,
            gold: 0,
            place: place::CURSOR,
        },
        StashRequest::Take {
            req: 5,
            from: goods::STACK,
            item: None,
            stack: Some(("r01".into(), 3)),
            gold: 0,
            place: place::INVENTORY,
        },
        StashRequest::Move {
            req: 6,
            item: 9,
            cell: cell(),
        },
        StashRequest::Arrange { req: 7, page: 1 },
        StashRequest::Search {
            text: "shako".into(),
        },
    ] {
        let body = m.body();
        assert_eq!(body[0], m.kind());
        assert!(KINDS.contains(&body[0]));
        assert_eq!(StashRequest::parse(&body).unwrap(), m);
    }
}

#[test]
fn events_round_trip() {
    for m in [
        StashEvent::Page {
            tab: tab::STACKS,
            page: 0,
            gold: 2_500_000,
            stacks: vec![("r01".into(), 4), ("gpb".into(), 1)],
            items: 0,
        },
        StashEvent::Item(StashItem {
            id: 0x0100_0007,
            cell: cell(),
            size: (2, 3),
            record: vec![0x10, 0x20, 0x30],
        }),
        StashEvent::Done {
            req: 8,
            code: 0,
            count: 1,
            left: vec![(12, 99)],
        },
        StashEvent::refused(9, 80),
        StashEvent::Found {
            hits: vec![(1, cell()), (2, StashCell::default())],
        },
    ] {
        let body = m.body();
        assert_eq!(body[0], m.kind());
        assert!(KINDS.contains(&body[0]));
        assert_eq!(StashEvent::parse(&body).unwrap(), m);
    }
}

/// The largest item and the longest lists still fit one control frame.
#[test]
fn largest_messages_fit_a_frame() {
    let item = StashEvent::Item(StashItem {
        id: 1,
        cell: cell(),
        size: (2, 4),
        record: vec![0xAB; MAX_ITEM],
    });
    let found = StashEvent::Found {
        hits: vec![(1, cell()); MAX_HITS],
    };
    let page = StashEvent::Page {
        tab: tab::STACKS,
        page: 0,
        gold: u64::MAX,
        stacks: vec![("abcdefgh".into(), u32::MAX); MAX_STACKS],
        items: 0,
    };
    let done = StashEvent::Done {
        req: 1,
        code: 0,
        count: 0,
        left: vec![(1, 2); MAX_UNITS],
    };
    let stacks = StashRequest::PutStacks {
        req: 1,
        units: vec![7; MAX_UNITS],
    };
    for body in [
        item.body(),
        found.body(),
        page.body(),
        done.body(),
        stacks.body(),
    ] {
        assert!(body.len() <= MAX_FRAME, "{} bytes", body.len());
    }
}

#[test]
fn decode_refuses_bad_fields() {
    // An item past its limit.
    let mut big = StashEvent::Item(StashItem {
        id: 1,
        cell: cell(),
        size: (1, 1),
        record: vec![0; MAX_ITEM + 1],
    })
    .body();
    assert!(StashEvent::parse(&big).is_err());
    big.truncate(10);
    assert!(StashEvent::parse(&big).is_err());
    // Too many units.
    let mut units = vec![kind::STASH_PUT_STACKS];
    units.extend_from_slice(&1u64.to_le_bytes());
    units.extend_from_slice(&((MAX_UNITS + 1) as u16).to_le_bytes());
    units.extend(std::iter::repeat_n(0, (MAX_UNITS + 1) * 4));
    assert!(StashRequest::parse(&units).is_err());
    // A bad option flag, trailing bytes, unknown kinds.
    let mut put = StashRequest::Put {
        req: 1,
        unit: 2,
        to: goods::ITEM,
        gold: 0,
        cell: None,
    }
    .body();
    *put.last_mut().unwrap() = 2;
    assert!(StashRequest::parse(&put).is_err());
    let mut open = StashRequest::Open { tab: 0, page: 0 }.body();
    open.push(0);
    assert!(StashRequest::parse(&open).is_err());
    assert!(StashRequest::parse(&[0xAF]).is_err());
    assert!(StashEvent::parse(&[kind::STASH_OPEN, 0, 0, 0]).is_err());
    // A search text past its limit.
    let long = StashRequest::Search {
        text: "a".repeat(MAX_SEARCH + 1),
    };
    assert!(StashRequest::parse(&long.body()).is_err());
}

#[test]
fn changing_requests_name_their_number() {
    assert_eq!(StashRequest::Arrange { req: 5, page: 0 }.req(), Some(5));
    assert_eq!(StashRequest::Open { tab: 0, page: 0 }.req(), None);
    assert_eq!(StashRequest::Search { text: "x".into() }.req(), None);
}

/// A mod realm's stash: the rename and layout requests and the shape
/// answer round trip; the kinds are new, so D2Gamma's messages keep their
/// bytes.
#[test]
fn layout_and_rename_round_trip() {
    let rename = StashRequest::Rename {
        req: 11,
        page: 9,
        name: "Runewords".into(),
    };
    let layout = StashRequest::Layout;
    for m in [rename.clone(), layout.clone()] {
        let body = m.body();
        assert!(KINDS.contains(&body[0]));
        assert_eq!(StashRequest::parse(&body).unwrap(), m);
    }
    assert_eq!(rename.req(), Some(11));
    assert_eq!(layout.req(), None);
    assert_eq!(layout.body(), vec![kind::STASH_LAYOUT]);
    let shape = StashEvent::Shape(StashShapeMsg {
        pages: 10,
        cols: 14,
        rows: 14,
        renamable: true,
        cap: 9_999_999,
        names: vec!["I".into(), "Charms".into(), "Ünïcode ✓".into()],
    });
    let body = shape.body();
    assert_eq!(body[0], kind::STASH_SHAPE);
    assert!(KINDS.contains(&body[0]));
    assert_eq!(StashEvent::parse(&body).unwrap(), shape);
    // The longest answer fits a frame.
    let big = StashEvent::Shape(StashShapeMsg {
        pages: MAX_PAGE_NAMES as u8,
        cols: 255,
        rows: 255,
        renamable: true,
        cap: u32::MAX,
        names: vec!["é".repeat(MAX_PAGE_NAME / 2); MAX_PAGE_NAMES],
    });
    assert!(big.body().len() <= MAX_FRAME);
    assert_eq!(StashEvent::parse(&big.body()).unwrap(), big);
    // A name past its limit, a bad flag.
    let long = StashRequest::Rename {
        req: 1,
        page: 0,
        name: "a".repeat(MAX_PAGE_NAME + 1),
    };
    assert!(StashRequest::parse(&long.body()).is_err());
    let mut flag = shape.body();
    flag[4] = 2;
    assert!(StashEvent::parse(&flag).is_err());
}

/// D2Gamma's own messages are byte for byte what they were.
#[test]
fn d2gamma_stash_bytes_are_unchanged() {
    assert_eq!(
        StashRequest::Open {
            tab: tab::ITEMS,
            page: 3
        }
        .body(),
        vec![0xA0, 0, 3, 0]
    );
    assert_eq!(
        StashRequest::Arrange { req: 7, page: 1 }.body(),
        vec![0xA7, 7, 0, 0, 0, 0, 0, 0, 0, 1]
    );
    assert_eq!(
        StashEvent::refused(9, 80).body(),
        vec![0xA4, 9, 0, 0, 0, 0, 0, 0, 0, 80, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]
    );
}
