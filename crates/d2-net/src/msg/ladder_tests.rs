//! The ladder messages' round trips and their decode rules.

use super::*;

fn row(rank: u32, flags: u8) -> LadderRow {
    LadderRow {
        rank,
        char: "Kaern".into(),
        class: 1,
        level: 92,
        experience: 2_000_000_000,
        tag: Some("NUG".into()),
        flags,
    }
}

#[test]
fn ladder_messages_round_trip() {
    for ask in [
        Ask::Seasons,
        Ask::Query(LadderQuery::default()),
        Ask::Query(LadderQuery {
            season: 1,
            split: split::HARDCORE | split::EXPANSION,
            class: 6,
            mode: mode::FIND,
            from_rank: 0,
            find: "kaern".into(),
            rows: 20,
        }),
        Ask::Query(LadderQuery {
            mode: mode::AROUND_ME,
            ..LadderQuery::default()
        }),
    ] {
        let m = ToRealm { req: 9, ask };
        let body = m.body();
        assert_eq!(body[0], m.kind());
        assert!(super::super::kind::LADDER.contains(&body[0]));
        assert!(super::super::kind::LOBBY.contains(&body[0]));
        assert_eq!(ToRealm::parse(&body), Ok(m));
    }
    for m in [
        FromRealm::SeasonRow {
            req: 9,
            row: SeasonRow {
                season: 1,
                name: "Test season".into(),
                started_ms: 1,
                ended_ms: 0,
                test: true,
                current: true,
            },
        },
        FromRealm::Head(LadderHead {
            req: 9,
            season: 1,
            split: split::EXPANSION,
            class: ALL_CLASSES,
            total: 60,
            first_rank: 1,
            rows: 50,
            my_rank: Some(57),
            updated_ms: 2,
            now_ms: 3,
        }),
        FromRealm::Row {
            req: 9,
            row: row(1, row_flags::DEAD_HARDCORE | row_flags::ONLINE),
        },
        FromRealm::Row {
            req: 9,
            row: LadderRow {
                tag: None,
                ..row(57, row_flags::PINNED | row_flags::MY_CHARACTER)
            },
        },
        FromRealm::End(LadderEnd::ok(9)),
        FromRealm::End(LadderEnd {
            req: 9,
            code: ErrorCode::RATE_LIMITED,
            wait_s: 20,
        }),
    ] {
        let body = m.body();
        assert_eq!(body[0], m.kind());
        assert!(super::super::kind::LADDER.contains(&body[0]));
        assert_eq!(m.req(), 9);
        assert_eq!(FromRealm::parse(&body), Ok(m));
    }
}

#[test]
fn pinned_row_follows_a_page_without_it() {
    let head = |first_rank, rows, my_rank| LadderHead {
        first_rank,
        rows,
        my_rank,
        ..LadderHead::default()
    };
    assert!(head(1, 50, Some(57)).pinned());
    assert!(!head(1, 50, Some(50)).pinned());
    assert!(head(1, 50, Some(51)).pinned());
    assert!(head(51, 10, Some(3)).pinned());
    assert!(!head(1, 50, None).pinned());
}

#[test]
fn ladder_malformed_refused() {
    let query = |q: LadderQuery| {
        ToRealm::parse(
            &ToRealm {
                req: 1,
                ask: Ask::Query(q),
            }
            .body(),
        )
    };
    for q in [
        LadderQuery {
            split: 4,
            ..LadderQuery::default()
        },
        LadderQuery {
            class: 7,
            ..LadderQuery::default()
        },
        LadderQuery {
            mode: 3,
            ..LadderQuery::default()
        },
        LadderQuery {
            find: "f".repeat(super::super::lobby::MAX_FROM + 1),
            ..LadderQuery::default()
        },
    ] {
        assert_eq!(query(q), bad());
    }
    // A page of more rows than a page has, a row's unknown class or flag,
    // an over-long tag or season name.
    let head = FromRealm::Head(LadderHead {
        rows: MAX_ROWS + 1,
        ..LadderHead::default()
    });
    assert_eq!(FromRealm::parse(&head.body()), bad());
    for r in [
        LadderRow {
            class: NO_CLASS,
            ..row(1, 0)
        },
        LadderRow {
            flags: 0x20,
            ..row(1, 0)
        },
        LadderRow {
            tag: Some("T".repeat(MAX_TAG + 1)),
            ..row(1, 0)
        },
    ] {
        let m = FromRealm::Row { req: 1, row: r };
        assert_eq!(FromRealm::parse(&m.body()), bad());
    }
    let season = FromRealm::SeasonRow {
        req: 1,
        row: SeasonRow {
            name: "s".repeat(MAX_SEASON_NAME + 1),
            ..SeasonRow::default()
        },
    };
    assert_eq!(FromRealm::parse(&season.body()), bad());
    // Trailing bytes, short bodies, kinds of the lobby.
    let mut body = ToRealm {
        req: 1,
        ask: Ask::Seasons,
    }
    .body();
    body.push(0);
    assert_eq!(ToRealm::parse(&body), bad());
    assert_eq!(FromRealm::parse(&[kind::END, 1, 0, 0, 0, 0]), bad());
    assert_eq!(FromRealm::parse(&[0x42]), bad());
}

fn bad<T>() -> Result<T, NetError> {
    Err(NetError::Malformed)
}
