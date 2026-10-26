//! The link messages' round trips and their decode rules.

use super::*;

fn hello() -> NodeHello {
    NodeHello {
        node: "eu-1".into(),
        host_token: None,
        version: "0.9.0".into(),
        protocol: 5,
        tick_rate: 25,
        game: "d2rust".into(),
        data_hash: [3; 32],
        listen_port: 6113,
        max_games: 16,
        max_players_total: 128,
    }
}

fn summary() -> CharSummary {
    CharSummary {
        name: "Kashya".into(),
        class: 0,
        level: 12,
        status: 0x20,
        difficulties: 1,
        look: vec![1; 16],
        last_played: 99,
        season: 0,
    }
}

fn node_messages() -> Vec<NodeToRealm> {
    vec![
        NodeToRealm::Hello(hello()),
        NodeToRealm::Hello(NodeHello {
            node: String::new(),
            host_token: Some([9; 16]),
            ..hello()
        }),
        NodeToRealm::Load(Load {
            games: 2,
            players: 7,
            cpu_permille: 310,
            tick_overruns: 4,
            draining: true,
        }),
        NodeToRealm::GameStarted { game: 3 },
        NodeToRealm::StartRefused {
            game: 3,
            code: ErrorCode::NODE_FULL,
        },
        NodeToRealm::SeatTaken { game: 3, lease: 77 },
        NodeToRealm::SeatRefused {
            game: 3,
            lease: 77,
            code: ErrorCode::CHAR_MISMATCH,
        },
        NodeToRealm::Save(SaveInfo {
            game: 3,
            lease: 77,
            seq: 12,
            reason: 1,
            journal_tick: 4000,
            summary: summary(),
        }),
        NodeToRealm::Players {
            game: 3,
            players: 4,
        },
        NodeToRealm::Left { game: 3, lease: 77 },
        NodeToRealm::GameClosed {
            game: 3,
            reason: ErrorCode::GAME_CLOSED,
        },
        NodeToRealm::DevAnswer {
            ok: true,
            text: "gold 100".into(),
        },
        NodeToRealm::CrashReport {
            game: 3,
            text: "a game fault".into(),
        },
        NodeToRealm::JournalChunk {
            game: 3,
            seq: 1,
            first_tick: 250,
            ticks: 250,
            last: false,
        },
        NodeToRealm::ItemIdsWanted { game: 3 },
        NodeToRealm::ReachInfo {
            method: 2,
            mapped_addr: Some("203.0.113.5:6113".into()),
        },
        NodeToRealm::PunchDone {
            game: 3,
            peer_addr: "198.51.100.7:50000".into(),
            connected: true,
        },
        NodeToRealm::UpdateState {
            version: "0.1.3.0".into(),
            state: update_state::READY,
            detail: String::new(),
        },
    ]
}

fn realm_messages() -> Vec<RealmToNode> {
    vec![
        RealmToNode::Welcome(NodeWelcome {
            realm: "D2RUST".into(),
            node: "eu-1".into(),
            kind: NodeKind::Official,
            seen_addr: "127.0.0.1:6113".into(),
            heartbeat_ms: 2000,
            orphan_s: 30,
            journal_chunk_s: 10,
            save_interval_s: 60,
            ticket_keys: vec![TicketKey {
                key_id: 1,
                public: [5; 32],
            }],
        }),
        RealmToNode::Refused {
            code: ErrorCode::NODE_UNKNOWN,
            text: "unknown".into(),
        },
        RealmToNode::StartGame {
            spec: NodeGameSpec {
                game: 3,
                name: "cows".into(),
                difficulty: 2,
                hardcore: false,
                expansion: true,
                max_players: 8,
            },
            seed: 0xdead_beef,
            item_ids: ItemIdBlock {
                first: 1000,
                count: 500,
            },
        },
        RealmToNode::SeatOffer {
            game: 3,
            lease: 77,
            player: 5,
            character: "Kashya".into(),
            new: None,
            has_record: true,
            ticket: None,
        },
        RealmToNode::SeatOffer {
            game: 3,
            lease: 78,
            player: 6,
            character: "Akara".into(),
            new: Some(NewCharInfo {
                name: "Akara".into(),
                class: 3,
                hardcore: true,
                expansion: true,
                progression: 0,
            }),
            has_record: false,
            ticket: Some([4; 16]),
        },
        RealmToNode::SaveAck {
            lease: 77,
            seq: 12,
            code: ErrorCode::LEASE_REVOKED,
        },
        RealmToNode::CloseGame { game: 3, how: 1 },
        RealmToNode::Kick { game: 3, lease: 77 },
        RealmToNode::Drain { on: true },
        RealmToNode::DevCommand {
            character: "Kashya".into(),
            command: "gold 100".into(),
        },
        RealmToNode::ItemIds {
            game: 3,
            ids: ItemIdBlock {
                first: 1500,
                count: 500,
            },
        },
        RealmToNode::ReachResult {
            reachable: false,
            checked_addr: "203.0.113.5:6113".into(),
            nat: 3,
        },
        RealmToNode::Punch {
            game: 3,
            peer_addr: "198.51.100.7:50000".into(),
            at_ms: 123_456,
            tries: 8,
        },
        RealmToNode::Update {
            phase: update_phase::COUNTDOWN,
            version: "0.1.3.0".into(),
            seconds: 600,
            text: "Restarting for an update in 10 minutes".into(),
        },
    ]
}

#[test]
fn every_message_round_trips_in_the_node_range() {
    let mut kinds = std::collections::BTreeSet::new();
    for m in node_messages() {
        let body = m.body();
        assert_eq!(body[0], m.kind());
        assert!(kind::NODE.contains(&body[0]));
        assert_eq!(NodeToRealm::parse(&body).unwrap(), m, "{m:?}");
        kinds.insert(body[0]);
    }
    for m in realm_messages() {
        let body = m.body();
        assert_eq!(body[0], m.kind());
        assert!(kind::NODE.contains(&body[0]));
        assert_eq!(RealmToNode::parse(&body).unwrap(), m, "{m:?}");
        kinds.insert(body[0]);
    }
    // 0x80-0x9D: each kind once, one direction each.
    assert_eq!(kinds, (0x80..=0x9D).collect());
    assert!(NodeToRealm::parse(&realm_messages()[0].body()).is_err());
    assert!(RealmToNode::parse(&NodeToRealm::Hello(hello()).body()).is_err());
}

#[test]
fn hello_layout_is_the_contracts() {
    let body = NodeToRealm::Hello(hello()).body();
    let mut want = vec![0x80, 4];
    want.extend_from_slice(b"eu-1");
    want.push(0); // no host token
    want.push(5);
    want.extend_from_slice(b"0.9.0");
    want.extend_from_slice(&5u32.to_le_bytes());
    want.extend_from_slice(&25u32.to_le_bytes());
    want.push(6);
    want.extend_from_slice(b"d2rust");
    want.extend_from_slice(&[3; 32]);
    want.extend_from_slice(&6113u16.to_le_bytes());
    want.extend_from_slice(&16u16.to_le_bytes());
    want.extend_from_slice(&128u16.to_le_bytes());
    want.push(0); // no running games
    assert_eq!(body, want);
}

#[test]
fn decode_refuses_bad_fields() {
    let bad = Err(NetError::Malformed);
    // An official id must be a valid id, a host has a token and no id.
    for h in [
        NodeHello {
            node: "EU 1".into(),
            ..hello()
        },
        NodeHello {
            host_token: Some([1; 16]),
            ..hello()
        },
        NodeHello {
            node: String::new(),
            ..hello()
        },
    ] {
        assert_eq!(NodeToRealm::parse(&NodeToRealm::Hello(h).body()), bad);
    }
    // Running games are not sent yet.
    let mut body = NodeToRealm::Hello(hello()).body();
    *body.last_mut().unwrap() = 1;
    assert_eq!(NodeToRealm::parse(&body), bad);
    // Trailing bytes.
    let mut body = NodeToRealm::GameStarted { game: 1 }.body();
    body.push(0);
    assert_eq!(NodeToRealm::parse(&body), bad);
    // A save reason past transfer.
    let mut body = NodeToRealm::Save(SaveInfo {
        game: 1,
        lease: 2,
        seq: 3,
        reason: 0,
        journal_tick: 0,
        summary: summary(),
    })
    .body();
    body[1 + 4 + 8 + 4] = SAVE_REASON_MAX + 1;
    assert_eq!(NodeToRealm::parse(&body), bad);
    // A seat is either a stored character or a new one.
    let both = RealmToNode::SeatOffer {
        game: 1,
        lease: 1,
        player: 1,
        character: "Akara".into(),
        new: Some(NewCharInfo {
            name: "Akara".into(),
            class: 1,
            hardcore: false,
            expansion: true,
            progression: 0,
        }),
        has_record: true,
        ticket: None,
    };
    assert_eq!(RealmToNode::parse(&both.body()), Err(NetError::Malformed));
    // A game spec past Hell.
    let RealmToNode::StartGame {
        spec,
        seed,
        item_ids,
    } = realm_messages()[2].clone()
    else {
        unreachable!()
    };
    let hell_plus = RealmToNode::StartGame {
        spec: NodeGameSpec {
            difficulty: 3,
            ..spec
        },
        seed,
        item_ids,
    };
    assert_eq!(
        RealmToNode::parse(&hell_plus.body()),
        Err(NetError::Malformed)
    );
    // A welcome without a heartbeat.
    let RealmToNode::Welcome(w) = realm_messages()[0].clone() else {
        unreachable!()
    };
    let still = RealmToNode::Welcome(NodeWelcome {
        heartbeat_ms: 0,
        ..w
    });
    assert_eq!(RealmToNode::parse(&still.body()), Err(NetError::Malformed));
}

#[test]
fn node_ids_are_short_lowercase_names() {
    for ok in ["eu-1", "a", "node-0123456789"] {
        assert!(valid_node_id(ok), "{ok}");
    }
    for no in ["", "EU-1", "eu 1", "eu_1", "node-01234567890", "ü"] {
        assert!(!valid_node_id(no), "{no}");
    }
}

#[test]
fn debug_hides_tokens_and_tickets() {
    let h = NodeToRealm::Hello(NodeHello {
        node: String::new(),
        host_token: Some([0xAB; 16]),
        ..hello()
    });
    let text = format!("{h:?}");
    assert!(text.contains("<hidden>"), "{text}");
    assert!(!text.contains("171"), "{text}");
    let offer = format!("{:?}", realm_messages()[4]);
    assert!(offer.contains("<hidden>"), "{offer}");
    assert!(!offer.contains("[4, 4"), "{offer}");
}

fn update(phase: u8, version: &str, text: &str) -> RealmToNode {
    RealmToNode::Update {
        phase,
        version: version.into(),
        seconds: 30,
        text: text.into(),
    }
}

fn update_state(version: &str, state: u8, detail: &str) -> NodeToRealm {
    NodeToRealm::UpdateState {
        version: version.into(),
        state,
        detail: detail.into(),
    }
}

/// Every phase and state round-trips; a phase or state past the last,
/// an empty or non-numeric version and a long text are malformed.
#[test]
fn update_fields_are_checked() {
    for phase in [
        update_phase::COUNTDOWN,
        update_phase::RESTART,
        update_phase::CANCEL,
        update_phase::CATCH_UP,
    ] {
        let m = update(phase, "0.1.3.0", "");
        assert_eq!(RealmToNode::parse(&m.body()).unwrap(), m);
    }
    for state in 0..=update_state::RESTARTING {
        let m = update_state("0.1.3.0", state, "checked");
        assert_eq!(NodeToRealm::parse(&m.body()).unwrap(), m);
    }
    let bad = Err(NetError::Malformed);
    let bad_n = Err(NetError::Malformed);
    assert_eq!(RealmToNode::parse(&update(4, "0.1.3.0", "").body()), bad);
    assert_eq!(
        NodeToRealm::parse(&update_state("0.1.3.0", 5, "").body()),
        bad_n
    );
    for v in ["", "0.1.3-beta", "../0.1"] {
        assert_eq!(RealmToNode::parse(&update(0, v, "").body()), bad, "{v}");
        assert_eq!(NodeToRealm::parse(&update_state(v, 1, "").body()), bad_n);
    }
    let long = "x".repeat(MAX_UPDATE_TEXT + 1);
    assert_eq!(RealmToNode::parse(&update(0, "0.1", &long).body()), bad);
    assert_eq!(
        NodeToRealm::parse(&update_state("0.1", 3, &long).body()),
        bad_n
    );
    assert!(valid_update_version("0.1.3.1200"));
    assert!(!valid_update_version(&"1".repeat(MAX_BUILD + 1)));
}

/// The update's layout: phase, version, seconds, text.
#[test]
fn update_layout_is_the_plans() {
    let body = update(update_phase::RESTART, "0.2", "now").body();
    let mut want = vec![0x9C, 1, 3];
    want.extend_from_slice(b"0.2");
    want.extend_from_slice(&30u32.to_le_bytes());
    want.push(3);
    want.extend_from_slice(b"now");
    assert_eq!(body, want);
    let body = update_state("0.2", update_state::READY, "").body();
    assert_eq!(body, vec![0x9D, 3, b'0', b'.', b'2', 2, 0]);
}
