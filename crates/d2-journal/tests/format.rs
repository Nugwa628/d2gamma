//! The journal format, its chunks and the intent digests, without a game.

use d2_journal::chunk::{compress, decode_chunk, read_journal, Journal, JournalError};
use d2_journal::digest::{check_intents, client_of_lease, span_digest, IntentDigester};
use d2_journal::format::{
    decode_body, encode_body, kind, leave, Entry, FormatError, GameSpec, Goods, Record, Start,
    VERSION,
};

fn start() -> Start {
    Start {
        version: "0.1.0+test".into(),
        data_hash: [7; 32],
        seed: 0x1234_5678,
        spec: GameSpec {
            id: 42,
            name: "duel-1".into(),
            difficulty: 1,
            hardcore: true,
            expansion: true,
            max_players: 8,
        },
        tick_rate: 25,
        player_hosted: true,
        host_client: Some(3),
    }
}

/// One record of every kind, in an order a journal could have.
fn every_kind() -> Vec<Entry> {
    let e = |tick, record| Entry { tick, record };
    vec![
        e(0, Record::Start(start())),
        e(0, Record::Frame { host_ms: 1000 }),
        e(
            0,
            Record::Join {
                client: 3,
                lease: 0xDEAD_BEEF_0001,
                record: [1; 32],
            },
        ),
        e(1, Record::Clock { host_ms: 1040 }),
        e(
            1,
            Record::Intent {
                client: 3,
                system: false,
                msg: vec![0x03, 0x10, 0x00, 0x20, 0x00],
            },
        ),
        e(
            1,
            Record::Intent {
                client: 3,
                system: true,
                msg: vec![0x6D, 1, 2, 3, 4],
            },
        ),
        e(
            2,
            Record::ItemIds {
                first: 1000,
                count: 256,
            },
        ),
        e(
            3,
            Record::ItemOut {
                client: 3,
                req: 9,
                unit: 77,
                item_id: Some(1001),
                stack: None,
                gold: 0,
            },
        ),
        e(
            3,
            Record::ItemOut {
                client: 3,
                req: 10,
                unit: 0,
                item_id: None,
                stack: Some(("gld".into(), 5)),
                gold: 500,
            },
        ),
        e(
            3,
            Record::ItemCredits {
                client: 3,
                req: 9,
                credits: vec![("r01".into(), 7), ("r02".into(), 2)],
            },
        ),
        e(
            4,
            Record::ItemIn {
                client: 3,
                req: 11,
                goods: Goods::Item(vec![0x4A, 0x4D, 1, 2]),
                gold: 0,
                cursor: true,
                result: 0,
            },
        ),
        e(
            4,
            Record::ItemIn {
                client: 3,
                req: 12,
                goods: Goods::Stack {
                    code: "key".into(),
                    count: 3,
                },
                gold: 10,
                cursor: false,
                result: 2,
            },
        ),
        e(5, Record::StateHash([9; 32])),
        e(
            5,
            Record::Save {
                lease: 0xDEAD_BEEF_0001,
                seq: 1,
                record: [2; 32],
            },
        ),
        e(
            6,
            Record::Leave {
                client: 3,
                how: leave::KICKED,
            },
        ),
        e(6, Record::End { reason: 4 }),
    ]
}

#[test]
fn every_record_kind_round_trips() {
    let entries = every_kind();
    let kinds: Vec<u8> = entries.iter().map(|e| e.record.kind()).collect();
    for k in 1..=13 {
        assert!(kinds.contains(&k), "kind {k} is covered");
    }
    let body = encode_body(42, 0, &entries);
    assert_eq!(body[0], VERSION);
    let back = decode_body(&body).unwrap();
    assert_eq!((back.game, back.seq), (42, 0));
    assert_eq!(back.entries, entries);
}

#[test]
fn a_cut_record_or_a_bad_field_does_not_decode() {
    let entries = every_kind();
    let body = encode_body(42, 0, &entries);
    // A cut between records decodes to the records before it; every cut
    // inside a record is caught.
    let ends: Vec<usize> = (0..=entries.len())
        .map(|n| encode_body(42, 0, &entries[..n]).len())
        .collect();
    for n in 9..body.len() {
        match ends.iter().position(|&e| e == n) {
            Some(k) => assert_eq!(decode_body(&body[..n]).unwrap().entries, entries[..k]),
            None => assert!(decode_body(&body[..n]).is_err(), "cut at {n} decoded"),
        }
    }
    let mut v = body.clone();
    v[0] = 2;
    assert_eq!(decode_body(&v), Err(FormatError::Version(2)));

    let one = |record| encode_body(1, 0, &[Entry { tick: 0, record }]);
    // Kind byte at offset 9 + 4.
    let mut k = one(Record::End { reason: 0 });
    k[13] = 14;
    assert_eq!(decode_body(&k), Err(FormatError::Kind(14)));
    let mut l = one(Record::Leave { client: 1, how: 0 });
    *l.last_mut().unwrap() = 9;
    assert_eq!(decode_body(&l), Err(FormatError::Field("leave how")));
    let mut b = one(Record::Intent {
        client: 1,
        system: true,
        msg: vec![1],
    });
    // client u16, then the bool.
    b[16] = 2;
    assert!(matches!(decode_body(&b), Err(FormatError::Field(_))));
    // Credits: client u16, req u64, then their count.
    let mut c = one(Record::ItemCredits {
        client: 1,
        req: 2,
        credits: vec![("r01".into(), 1)],
    });
    c[14 + 2 + 8] = 33;
    assert_eq!(decode_body(&c), Err(FormatError::Field("credits")));
}

#[test]
fn credits_past_the_limit_are_cut_and_the_kind_is_new() {
    let many: Vec<(String, u32)> = (0..40).map(|i| (format!("c{i}"), 1)).collect();
    let body = encode_body(
        1,
        0,
        &[Entry {
            tick: 0,
            record: Record::ItemCredits {
                client: 1,
                req: 2,
                credits: many.clone(),
            },
        }],
    );
    let Record::ItemCredits { credits, .. } = &decode_body(&body).unwrap().entries[0].record else {
        panic!("not credits");
    };
    assert_eq!(credits[..], many[..32]);
    // The kinds D2Gamma's own games write keep their numbers.
    assert_eq!((kind::ITEM_OUT, kind::ITEM_IN, kind::FRAME), (10, 11, 12));
    assert_eq!(kind::ITEM_CREDITS, 13);
}

fn chunks_of(entries: &[Entry], cuts: &[usize]) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    let mut from = 0;
    for (seq, &to) in cuts.iter().chain([&entries.len()]).enumerate() {
        out.push(compress(&encode_body(42, seq as u32, &entries[from..to])));
        from = to;
    }
    out
}

#[test]
fn a_journal_file_is_its_chunks_one_after_another() {
    let entries = every_kind();
    let chunks = chunks_of(&entries, &[3, 7, 12]);
    for c in &chunks {
        decode_chunk(c).unwrap();
    }
    let file: Vec<u8> = chunks.concat();
    let j = read_journal(&file).unwrap();
    assert_eq!(j.game, 42);
    assert_eq!(j.chunks, 4);
    assert!(j.ended);
    assert_eq!(j.entries, entries);
    assert_eq!(j.start(), &start());

    // Any gzip reader reads the file as one stream: the bodies back to back.
    let mut all = Vec::new();
    std::io::Read::read_to_end(&mut flate2::read::MultiGzDecoder::new(&file[..]), &mut all)
        .unwrap();
    let bodies: Vec<u8> = chunks
        .iter()
        .flat_map(|c| {
            let mut b = Vec::new();
            std::io::Read::read_to_end(&mut flate2::read::GzDecoder::new(&c[..]), &mut b).unwrap();
            b
        })
        .collect();
    assert_eq!(all, bodies);
}

#[test]
fn a_journal_cut_off_reads_without_its_end() {
    let entries = every_kind();
    let chunks = chunks_of(&entries, &[3, 7, 12]);
    let j = read_journal(&chunks[..3].concat()).unwrap();
    assert!(!j.ended);
    assert_eq!(j.entries, entries[..12]);
    // A member cut inside is an error, naming the chunk.
    let mut file = chunks[..2].concat();
    file.extend_from_slice(&chunks[2][..chunks[2].len() - 3]);
    assert!(matches!(
        read_journal(&file),
        Err(JournalError::Gzip { chunk: 2, .. })
    ));
}

#[test]
fn chunks_out_of_order_or_of_another_game_are_refused() {
    let entries = every_kind();
    let chunks = chunks_of(&entries, &[3, 7, 12]);
    let swapped = [&chunks[0], &chunks[2], &chunks[1], &chunks[3]]
        .iter()
        .flat_map(|c| c.iter().copied())
        .collect::<Vec<u8>>();
    assert_eq!(
        read_journal(&swapped),
        Err(JournalError::Order {
            chunk: 1,
            want: 1,
            got: 2
        })
    );
    let mut other = chunks[..1].concat();
    other.extend(compress(&encode_body(43, 1, &entries[3..7])));
    assert_eq!(
        read_journal(&other),
        Err(JournalError::OtherGame {
            chunk: 1,
            want: 42,
            got: 43
        })
    );
    assert_eq!(read_journal(&[]), Err(JournalError::Empty));
}

#[test]
fn record_order_is_checked() {
    let entries = every_kind();
    let check =
        |e: Vec<Entry>| Journal::from_bodies(vec![decode_body(&encode_body(42, 0, &e)).unwrap()]);
    assert_eq!(check(entries[1..].to_vec()), Err(JournalError::NoStart));
    let mut two = entries.clone();
    two.insert(3, entries[0].clone());
    assert_eq!(check(two), Err(JournalError::SecondStart { tick: 0 }));
    let mut back = entries.clone();
    back[6].tick = 0;
    assert!(matches!(
        check(back),
        Err(JournalError::TimeBack { index: 6, .. })
    ));
    let mut after = entries.clone();
    after.push(Entry {
        tick: 7,
        record: Record::Frame { host_ms: 2000 },
    });
    assert_eq!(check(after), Err(JournalError::AfterEnd { tick: 7 }));
    assert_eq!(kind::FRAME, 12);
}

fn intent(tick: u32, client: u16, msg: &[u8]) -> Entry {
    Entry {
        tick,
        record: Record::Intent {
            client,
            system: false,
            msg: msg.to_vec(),
        },
    }
}

fn journal_with(entries: Vec<Entry>) -> Journal {
    let mut all = vec![
        Entry {
            tick: 0,
            record: Record::Start(start()),
        },
        Entry {
            tick: 0,
            record: Record::Join {
                client: 1,
                lease: 500,
                record: [0; 32],
            },
        },
    ];
    all.extend(entries);
    Journal::from_bodies(vec![decode_body(&encode_body(42, 0, &all)).unwrap()]).unwrap()
}

#[test]
fn intent_digests_check_a_clients_messages_span_by_span() {
    let msgs: [&[u8]; 5] = [
        &[0x03, 1, 0, 2, 0],
        &[0x06, 1],
        &[0x03, 3, 0, 4, 0],
        &[0x13],
        &[0x5F],
    ];
    // The client: two spans, then one message it sent no digest for yet.
    let mut d = IntentDigester::new();
    assert_eq!(d.take(), None);
    d.sent(10, msgs[0]);
    d.sent(12, msgs[1]);
    let a = d.take().unwrap();
    assert_eq!((a.first_tick, a.last_tick, a.count), (10, 12, 2));
    assert_eq!(a.digest, span_digest([msgs[0], msgs[1]]));
    d.sent(30, msgs[2]);
    d.sent(31, msgs[3]);
    let b = d.take().unwrap();

    // The journal: the client's game messages, with another client's and a
    // system message between them.
    let mut e = vec![
        intent(5, 1, msgs[0]),
        intent(5, 2, &[0x99]),
        intent(7, 1, msgs[1]),
        Entry {
            tick: 7,
            record: Record::Intent {
                client: 1,
                system: true,
                msg: vec![0x6D],
            },
        },
        intent(20, 1, msgs[2]),
        intent(21, 1, msgs[3]),
        intent(22, 1, msgs[4]),
    ];
    let j = journal_with(e.clone());
    assert_eq!(client_of_lease(&j, 500), Some(1));
    assert_eq!(client_of_lease(&j, 501), None);
    let ok = check_intents(&j, 1, &[a, b]).unwrap();
    assert_eq!((ok.spans, ok.messages, ok.uncovered), (2, 4, 1));

    // A message the client never sent, put in by the game server.
    e.insert(4, intent(19, 1, &[0x0C, 1, 0, 1, 0]));
    let err = check_intents(&journal_with(e.clone()), 1, &[a, b]).unwrap_err();
    assert_eq!((err.span, err.journal_tick), (1, Some(19)));

    // A message dropped from the journal.
    e.remove(4);
    e.remove(0);
    let err = check_intents(&journal_with(e), 1, &[a, b]).unwrap_err();
    assert_eq!((err.span, err.journal_tick), (0, Some(7)));

    // More digested messages than the journal has.
    let j = journal_with(vec![intent(5, 1, msgs[0])]);
    let err = check_intents(&j, 1, &[a]).unwrap_err();
    assert_eq!(err.span, 0);
}
