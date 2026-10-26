//! The online store against a fake main server: what each call sends and
//! how the answers read.

use std::collections::VecDeque;

use super::*;
use crate::online::{goods, tab};

/// A fake main-server connection: keeps what was sent, answers from a
/// queue.
#[derive(Default)]
struct FakeRealm {
    sent: Vec<ToRealm>,
    answers: VecDeque<FromRealm>,
}

impl RealmStash for FakeRealm {
    fn call(&mut self, msg: ToRealm) -> Result<FromRealm, StoreError> {
        self.sent.push(msg);
        self.answers
            .pop_front()
            .ok_or_else(|| StoreError::Link("connection closed".into()))
    }
}

fn done(req: u64, count: u64) -> FromRealm {
    FromRealm::Done {
        req,
        code: 0,
        count,
        left: Vec::new(),
    }
}

fn stash(answers: Vec<FromRealm>) -> OnlineStash<FakeRealm> {
    OnlineStash::new(
        FakeRealm {
            sent: Vec::new(),
            answers: answers.into(),
        },
        catalog(),
        5000,
    )
}

#[test]
fn moves_are_sent_with_rising_request_numbers() {
    let mut s = stash(vec![
        done(5000, 1),
        done(5001, 0),
        done(5002, 1),
        done(5003, 3),
        done(5004, 250),
        done(5005, 0),
        done(5006, 0),
    ]);
    assert!(s.online());
    let cell = Cell::new(2, 1, 1);
    s.put(Put::Item {
        unit: 77,
        at: Some(cell),
    })
    .unwrap();
    assert_eq!(s.put(Put::Gold(400)).unwrap(), PutDone::default());
    assert_eq!(
        s.take(Take::Item {
            id: ItemId(9),
            to: TakeTo::Cursor
        }),
        Ok(1)
    );
    assert_eq!(
        s.take(Take::Stack {
            code: "r22".into(),
            count: 3
        }),
        Ok(3)
    );
    assert_eq!(s.take(Take::Gold(250)), Ok(250));
    s.move_item(ItemId(9), Cell::new(0, 0, 0)).unwrap();
    s.arrange(4).unwrap();
    assert_eq!(
        s.link().sent,
        vec![
            ToRealm::Put {
                req: 5000,
                unit: 77,
                to: goods::ITEM,
                gold: 0,
                cell: Some(cell)
            },
            ToRealm::Put {
                req: 5001,
                unit: 0,
                to: goods::GOLD,
                gold: 400,
                cell: None
            },
            ToRealm::Take {
                req: 5002,
                from: goods::ITEM,
                item: Some(ItemId(9)),
                stack: None,
                gold: 0,
                place: 1
            },
            ToRealm::Take {
                req: 5003,
                from: goods::STACK,
                item: None,
                stack: Some(("r22".into(), 3)),
                gold: 0,
                place: 0
            },
            ToRealm::Take {
                req: 5004,
                from: goods::GOLD,
                item: None,
                stack: None,
                gold: 250,
                place: 0
            },
            ToRealm::Move {
                req: 5005,
                item: ItemId(9),
                cell: Cell::new(0, 0, 0)
            },
            ToRealm::Arrange { req: 5006, page: 4 },
        ]
    );
}

#[test]
fn refusals_read_as_the_economy_s() {
    let mut s = stash(vec![
        FromRealm::Done {
            req: 5000,
            code: Refusal::StashFull.code(),
            count: 0,
            left: Vec::new(),
        },
        FromRealm::Done {
            req: 5001,
            code: 9999,
            count: 0,
            left: Vec::new(),
        },
        FromRealm::Found { hits: Vec::new() },
    ]);
    assert_eq!(
        s.put(Put::Item { unit: 1, at: None }),
        Err(StoreError::Refused(Refusal::StashFull))
    );
    assert!(matches!(s.take(Take::Gold(5)), Err(StoreError::Link(_))));
    assert!(
        matches!(s.arrange(0), Err(StoreError::Link(_))),
        "a wrong answer is the link's fault"
    );
    assert!(matches!(s.gold(), Err(StoreError::Link(_))), "no answer");
}

/// The main server's own codes: no room on the character, no game, not
/// kept online yet, a failed move. They are no economy refusal (8 and 99
/// would read as `MarketOff` and `BadRequest`).
#[test]
fn the_main_server_s_own_codes_read_as_theirs() {
    let refused = |code| FromRealm::Done {
        req: 0,
        code,
        count: 0,
        left: Vec::new(),
    };
    let mut s = stash(vec![refused(93), refused(81), refused(8), refused(99)]);
    let take = || Take::Item {
        id: ItemId(9),
        to: TakeTo::Inventory,
    };
    assert_eq!(s.take(take()), Err(StoreError::NoRoom));
    assert!(matches!(s.take(take()), Err(StoreError::Game(_))));
    assert!(matches!(
        s.take(Take::Stack {
            code: "r01".into(),
            count: 1
        }),
        Err(StoreError::Link(_))
    ));
    assert!(matches!(s.take(take()), Err(StoreError::Link(_))));
}

#[test]
fn send_stackables_reads_what_stayed() {
    let mut s = stash(vec![FromRealm::Done {
        req: 5000,
        code: 0,
        count: 3,
        left: vec![
            (12, Refusal::StashFull.code()),
            (14, Refusal::BadRequest.code()),
        ],
    }]);
    let out = s
        .put(Put::Stacks {
            units: vec![10, 11, 12, 13, 14],
        })
        .unwrap();
    assert_eq!(out.moved, vec![10, 11, 13]);
    assert_eq!(
        out.left,
        vec![(12, Refusal::StashFull), (14, Refusal::BadRequest)]
    );
    assert_eq!(
        s.link().sent,
        vec![ToRealm::PutStacks {
            req: 5000,
            units: vec![10, 11, 12, 13, 14]
        }]
    );
}

#[test]
fn pages_slots_gold_and_search_read_from_the_answers() {
    let item = RealmItem {
        id: ItemId(3),
        cell: Cell::new(6, 2, 5),
        size: (2, 3),
        record: vec![1, 2, 3],
    };
    let slots = FromRealm::Page {
        tab: tab::STACKS,
        page: 0,
        items: Vec::new(),
        stacks: vec![("r01".into(), 4), ("gpw".into(), 0)],
        gold: 1234,
    };
    let mut s = stash(vec![
        FromRealm::Page {
            tab: tab::ITEMS,
            page: 6,
            items: vec![item.clone()],
            stacks: Vec::new(),
            gold: 0,
        },
        slots.clone(),
        slots,
        FromRealm::Found {
            hits: vec![(ItemId(3), item.cell)],
        },
    ]);
    let page = s.page(6).unwrap();
    assert_eq!(
        (
            page[0].id,
            page[0].cell,
            page[0].size,
            page[0].record.clone()
        ),
        (item.id, item.cell, item.size, item.record.clone())
    );
    let stacks = s.stacks().unwrap();
    assert_eq!(
        stacks.into_iter().collect::<Vec<_>>(),
        vec![("r01".into(), 4)]
    );
    assert_eq!(s.gold(), Ok(1234));
    assert_eq!(s.search("shako"), Ok(vec![(ItemId(3), item.cell)]));
    assert_eq!(
        s.link().sent,
        vec![
            ToRealm::Open {
                tab: tab::ITEMS,
                page: 6
            },
            ToRealm::Open {
                tab: tab::STACKS,
                page: 0
            },
            ToRealm::Open {
                tab: tab::STACKS,
                page: 0
            },
            ToRealm::Search {
                text: "shako".into()
            },
        ]
    );
}

/// A mod realm's pages: the layout answer sets the shape the panel reads;
/// renames go out only for its renamable pages. Until the layout comes,
/// the shape is D2Gamma's.
#[test]
fn a_mod_realms_layout_and_renames() {
    let mod_shape = StashShape {
        pages: 10,
        cols: 14,
        rows: 14,
        renamable: true,
    };
    let names: Vec<String> = (0..10).map(StashShape::default_name).collect();
    let mut s = stash(vec![
        FromRealm::Shape {
            shape: mod_shape,
            cap: 9_999_999,
            names: names.clone(),
        },
        done(5000, 0),
    ]);
    assert_eq!(s.shape(), StashShape::DEFAULT);
    // D2Gamma's pages are never renamed: nothing is sent.
    assert!(s.rename_page(0, "Gear").is_err());
    assert!(s.link().sent.is_empty());
    assert_eq!(s.page_names().unwrap(), names);
    assert_eq!(s.shape(), mod_shape);
    s.rename_page(3, "  Charms ").unwrap();
    assert!(s.rename_page(10, "Past").is_err());
    assert!(s.rename_page(1, "Thirteen long").is_err());
    assert_eq!(
        s.link().sent,
        vec![
            ToRealm::Layout,
            ToRealm::Rename {
                req: 5000,
                page: 3,
                name: "Charms".into()
            }
        ]
    );
}
