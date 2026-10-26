//! Putting the ladder's answers together.

use super::*;
use d2_net::msg::ladder::{row_flags, LadderQuery};
use d2_net::msg::lobby::LobbyEvent;
use d2_net::msg::ErrorCode;

fn row(rank: u32, flags: u8) -> FromRealm {
    FromRealm::Row {
        req: 0,
        row: LadderRow {
            rank,
            char: format!("c{rank}"),
            class: 2,
            level: 80,
            experience: 1_000 - u64::from(rank),
            tag: None,
            flags,
        },
    }
}

/// Sets the `req` of a message built with 0.
fn with_req(m: FromRealm, req: u32) -> Vec<u8> {
    let mut body = m.body();
    body[1..5].copy_from_slice(&req.to_le_bytes());
    body
}

fn head(req: u32, first_rank: u32, rows: u8, my_rank: Option<u32>) -> Vec<u8> {
    FromRealm::Head(LadderHead {
        req,
        season: 1,
        split: ladder::split::EXPANSION,
        class: ladder::ALL_CLASSES,
        total: 60,
        first_rank,
        rows,
        my_rank,
        updated_ms: 1,
        now_ms: 2,
    })
    .body()
}

fn end(req: u32, code: ErrorCode) -> Vec<u8> {
    FromRealm::End(LadderEnd {
        req,
        code,
        wait_s: 0,
    })
    .body()
}

/// A page with the pinned own row, the seasons and a refused find, their
/// bodies interleaved with each other and with lobby bodies: each comes
/// out whole under its `req`; the lobby bodies go back to the caller.
#[test]
fn ladder_client_assembles_pages() {
    let mut c = LadderClient::new();
    let (page, body) = c.prepare(Ask::Query(LadderQuery {
        rows: 2,
        ..LadderQuery::default()
    }));
    assert_eq!(body[0], ladder::kind::QUERY);
    let (seasons, _) = c.prepare(Ask::Seasons);
    let (find, _) = c.prepare(Ask::Query(LadderQuery {
        mode: ladder::mode::FIND,
        find: "nobody".into(),
        ..LadderQuery::default()
    }));
    let lobby = LobbyEvent::GameListEnd.body();
    let season = |season: u16| {
        FromRealm::SeasonRow {
            req: seasons,
            row: SeasonRow {
                season,
                ..SeasonRow::default()
            },
        }
        .body()
    };
    let bodies = [
        head(page, 1, 2, Some(37)),
        season(1),
        with_req(row(1, row_flags::ONLINE), page),
        lobby.clone(),
        end(find, ErrorCode::UNKNOWN_PLAYER),
        with_req(row(2, row_flags::DEAD_HARDCORE), page),
        season(0),
        with_req(row(37, row_flags::PINNED | row_flags::MY_CHARACTER), page),
        end(seasons, ErrorCode(0)),
        end(page, ErrorCode(0)),
    ];
    let mut others = 0;
    for b in &bodies {
        if !c.feed(b).unwrap() {
            others += 1;
            assert!(crate::lobby::lobby_event(b).is_some());
        } else {
            // Ladder bodies are never lobby events, though inside its range.
            assert!(crate::lobby::lobby_event(b).is_none(), "{:#x}", b[0]);
        }
    }
    assert_eq!(others, 1);
    let got = c.take();
    assert_eq!(got.len(), 3, "{got:#?}");
    match &got[0] {
        LadderAnswer::Page {
            req,
            head,
            rows,
            pinned,
            end,
        } => {
            assert_eq!(*req, find);
            assert!(head.is_none() && rows.is_empty() && pinned.is_none());
            assert_eq!(end.code, ErrorCode::UNKNOWN_PLAYER);
        }
        other => panic!("{other:?}"),
    }
    match &got[1] {
        LadderAnswer::Seasons { req, rows, end } => {
            assert_eq!(*req, seasons);
            assert_eq!(rows.iter().map(|r| r.season).collect::<Vec<_>>(), [1, 0]);
            assert!(end.is_ok());
        }
        other => panic!("{other:?}"),
    }
    match &got[2] {
        LadderAnswer::Page {
            req,
            head,
            rows,
            pinned,
            end,
        } => {
            assert_eq!(*req, page);
            assert_eq!(head.as_ref().unwrap().my_rank, Some(37));
            assert_eq!(rows.iter().map(|r| r.rank).collect::<Vec<_>>(), [1, 2]);
            assert_eq!(pinned.as_ref().unwrap().rank, 37);
            assert!(end.is_ok());
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(c.in_flight(), 0);
}

#[test]
fn broken_pages_are_malformed() {
    let run = |bodies: &[Vec<u8>]| {
        let mut c = LadderClient::new();
        let (req, _) = c.prepare(Ask::Query(LadderQuery::default()));
        assert_eq!(req, 1);
        let mut out = Ok(true);
        for b in bodies {
            out = c.feed(b);
            if out.is_err() {
                break;
            }
        }
        out
    };
    let malformed = Err(OnlineError::Malformed);
    // A row before the head; a row past the page that isn't a pinned own
    // row; an end with rows missing; a head after rows; a season row for a
    // page; a done page without a head.
    assert_eq!(run(&[with_req(row(1, 0), 1)]), malformed);
    assert_eq!(
        run(&[
            head(1, 1, 1, None),
            with_req(row(1, 0), 1),
            with_req(row(2, 0), 1)
        ]),
        malformed
    );
    assert_eq!(
        run(&[
            head(1, 1, 1, Some(1)),
            with_req(row(1, 0), 1),
            with_req(row(5, row_flags::PINNED), 1)
        ]),
        malformed
    );
    assert_eq!(
        run(&[
            head(1, 1, 2, None),
            with_req(row(1, 0), 1),
            end(1, ErrorCode(0))
        ]),
        malformed
    );
    assert_eq!(run(&[head(1, 1, 0, None), head(1, 1, 0, None)]), malformed);
    let season = FromRealm::SeasonRow {
        req: 1,
        row: SeasonRow::default(),
    };
    assert_eq!(run(&[season.body()]), malformed);
    assert_eq!(run(&[end(1, ErrorCode(0))]), malformed);
    // Answers to a request never sent are dropped, not malformed.
    assert_eq!(run(&[end(9, ErrorCode(0))]), Ok(true));
}

#[test]
fn errors_answer_the_oldest_request_of_their_kind() {
    let mut c = LadderClient::new();
    let (q, _) = c.prepare(Ask::Query(LadderQuery::default()));
    let (s, _) = c.prepare(Ask::Seasons);
    let e = ErrorMsg::new(ladder::kind::SEASONS, ErrorCode::NOT_SUPPORTED);
    assert!(c.feed(&e.body()).unwrap());
    let lobby_error = ErrorMsg::new(0x45, ErrorCode::GAME_FULL);
    assert!(!c.feed(&lobby_error.body()).unwrap());
    let got = c.take();
    assert!(matches!(&got[..], [LadderAnswer::Error { req: Some(r), .. }] if *r == s));
    assert_eq!(c.in_flight(), 1);
    c.reset();
    assert_eq!(c.in_flight(), 0);
    assert!(c.prepare(Ask::Seasons).0 > q);
}
