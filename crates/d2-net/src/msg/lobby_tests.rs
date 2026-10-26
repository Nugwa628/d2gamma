//! The lobby messages' round trips and their decode rules.

use super::*;

fn game() -> GameInfo {
    GameInfo {
        game_id: 7,
        name: "baal run".into(),
        difficulty: 2,
        players: 3,
        max_players: 8,
        locked: true,
        hardcore: false,
        expansion: true,
        joinable: ErrorCode::DIFFICULTY_LOCKED.0,
        ..GameInfo::default()
    }
}

/// A v6 entry with every tail field set, at its longest.
fn game_v6() -> GameInfo {
    GameInfo {
        node_id: "h-0123456789ab".into(),
        server: "S".repeat(MAX_SERVER_LABEL),
        player_hosted: true,
        host_rtt_ms: 64,
        description: "d".repeat(MAX_DESCRIPTION),
        level_min: 40,
        level_max: 70,
        created_ms: 1_800_000_000_000,
        ..game()
    }
}

#[test]
fn requests_round_trip() {
    for m in [
        LobbyRequest::ListGames,
        LobbyRequest::CreateGame {
            name: "cows".into(),
            password: String::new(),
            difficulty: 1,
            max_players: 0,
            place: Placement::Automatic,
            description: String::new(),
            level_min: 0,
            level_max: 0,
        },
        LobbyRequest::JoinGame {
            game_id: 9,
            password: "moo".into(),
        },
        LobbyRequest::LeaveLobby,
        LobbyRequest::Chat {
            text: "hello there".into(),
        },
    ] {
        let body = m.body();
        assert_eq!(body[0], m.kind());
        assert!(super::super::kind::LOBBY.contains(&body[0]));
        assert_eq!(LobbyRequest::parse(&body).unwrap(), m);
    }
}

#[test]
fn events_round_trip() {
    for m in [
        LobbyEvent::GameListBegin { count: 2 },
        LobbyEvent::GameEntry(game()),
        LobbyEvent::GameListEnd,
        LobbyEvent::GameGone { game_id: 7 },
        LobbyEvent::JoinOk {
            game_id: 7,
            ticket: [3; 16],
            difficulty: 2,
        },
        LobbyEvent::ChatLine {
            from: "Werwer".into(),
            text: "anyone for cows?".into(),
            at: 1_800_000_000,
        },
    ] {
        let body = m.body();
        assert_eq!(body[0], m.kind());
        assert_eq!(LobbyEvent::parse(&body).unwrap(), m);
    }
}

/// The exact layout of `CreateGame` (contracts 3.6, wire 6): name,
/// password, difficulty, max players, the placement, the description and
/// the level range.
#[test]
fn create_game_layout() {
    let body = LobbyRequest::CreateGame {
        name: "ab".into(),
        password: "x".into(),
        difficulty: 2,
        max_players: 4,
        place: Placement::Automatic,
        description: String::new(),
        level_min: 0,
        level_max: 0,
    }
    .body();
    assert_eq!(body, [0x45, 2, b'a', b'b', 1, b'x', 2, 4, 0, 0, 0, 0, 0]);
}

/// The list's entry never carries a password, only whether there is one.
#[test]
fn game_entry_layout_has_no_password() {
    let body = LobbyEvent::GameEntry(game()).body();
    let mut want = vec![0x42, 7, 0, 0, 0, 8];
    want.extend_from_slice(b"baal run");
    want.extend_from_slice(&[2, 3, 8, 1, 0, 1, 66, 0]);
    // The v6 tail, empty: node, server, hosted, host rtt, description,
    // level range, created.
    want.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, 0]);
    want.extend_from_slice(&[0; 8]);
    assert_eq!(body, want);
}

/// `GameInfo` v6 round trips with every tail field at its longest, and
/// the entry fits a frame.
#[test]
fn game_info_v6_round_trips_with_longest_description() {
    let m = LobbyEvent::GameEntry(game_v6());
    let body = m.body();
    assert!(body.len() <= crate::wire::MAX_FRAME, "{}", body.len());
    assert_eq!(LobbyEvent::parse(&body).unwrap(), m);
    // One byte more of description, server or node id is malformed.
    for long in [
        GameInfo {
            description: "d".repeat(MAX_DESCRIPTION + 1),
            ..game_v6()
        },
        GameInfo {
            server: "S".repeat(MAX_SERVER_LABEL + 1),
            ..game_v6()
        },
        GameInfo {
            node_id: "n".repeat(MAX_NODE_ID + 1),
            ..game_v6()
        },
        // A range whose lowest level is above its highest.
        GameInfo {
            level_min: 71,
            ..game_v6()
        },
    ] {
        assert!(LobbyEvent::parse(&LobbyEvent::GameEntry(long).body()).is_err());
    }
    // An open bound on either side is a range.
    let open = GameInfo {
        level_min: 90,
        level_max: 0,
        ..game_v6()
    };
    assert!(LobbyEvent::parse(&LobbyEvent::GameEntry(open).body()).is_ok());
}

/// `CreateGame`'s v6 fields round trip: every placement, the longest
/// description, a level range; a too-long description or a bad placement
/// is malformed. A range the rules refuse (min above max) still decodes,
/// so the realm can answer `LevelRange`.
#[test]
fn create_game_v6_fields_round_trip() {
    let create = |place: Placement, description: String, level_min: u8, level_max: u8| {
        LobbyRequest::CreateGame {
            name: "cows".into(),
            password: "moo".into(),
            difficulty: 1,
            max_players: 8,
            place,
            description,
            level_min,
            level_max,
        }
    };
    for place in [
        Placement::Automatic,
        Placement::AutomaticPick("hel-1".into()),
        Placement::Official("fra-1".into()),
        Placement::HostOnMyPc,
    ] {
        let m = create(place, "d".repeat(MAX_DESCRIPTION), 40, 70);
        assert_eq!(LobbyRequest::parse(&m.body()).unwrap(), m);
    }
    let backwards = create(Placement::Automatic, String::new(), 70, 40);
    assert_eq!(LobbyRequest::parse(&backwards.body()).unwrap(), backwards);
    let long = create(Placement::Automatic, "d".repeat(MAX_DESCRIPTION + 1), 0, 0);
    assert!(LobbyRequest::parse(&long.body()).is_err());
    // An official placement without a server.
    let mut body = create(Placement::Automatic, String::new(), 0, 0).body();
    let at = body.len() - 5;
    body[at] = 1;
    assert!(LobbyRequest::parse(&body).is_err());
    // The `Debug` of a create shows its new fields, never the password.
    let text = format!("{:?}", create(Placement::HostOnMyPc, "x".into(), 1, 2));
    assert!(
        text.contains("HostOnMyPc") && !text.contains("moo"),
        "{text}"
    );
}

#[test]
fn decode_refuses_bad_fields() {
    // Over-long name and password.
    let long = LobbyRequest::CreateGame {
        name: "a".repeat(16),
        password: String::new(),
        difficulty: 0,
        max_players: 0,
        place: Placement::Automatic,
        description: String::new(),
        level_min: 0,
        level_max: 0,
    };
    assert!(LobbyRequest::parse(&long.body()).is_err());
    let long_pw = LobbyRequest::JoinGame {
        game_id: 1,
        password: "p".repeat(16),
    };
    assert!(LobbyRequest::parse(&long_pw.body()).is_err());
    // Empty and over-long chat.
    assert!(LobbyRequest::parse(&[kind::CHAT, 0]).is_err());
    let mut chat = vec![kind::CHAT, 201];
    chat.extend(std::iter::repeat_n(b'a', 201));
    assert!(LobbyRequest::parse(&chat).is_err());
    // Control characters.
    assert!(LobbyRequest::parse(&[kind::CHAT, 2, b'a', 7]).is_err());
    // Trailing bytes, unknown kinds, short bodies.
    assert!(LobbyRequest::parse(&[kind::LIST_GAMES, 0]).is_err());
    assert!(LobbyRequest::parse(&[0x5F]).is_err());
    assert!(LobbyRequest::parse(&[kind::JOIN_GAME, 1, 0]).is_err());
    assert!(LobbyEvent::parse(&[kind::GAME_GONE, 1]).is_err());
    // A difficulty past Hell.
    let mut bad = LobbyEvent::JoinOk {
        game_id: 1,
        ticket: [0; 16],
        difficulty: 0,
    }
    .body();
    *bad.last_mut().unwrap() = 3;
    assert!(LobbyEvent::parse(&bad).is_err());
}

#[test]
fn refusal_names_the_code() {
    assert_eq!(game().refusal(), Some(ErrorCode::DIFFICULTY_LOCKED));
    assert!(!game().can_join());
    let open = GameInfo {
        joinable: 0,
        ..game()
    };
    assert!(open.can_join());
    assert_eq!(open.refusal(), None);
}

/// A password never reaches a log: `Debug` says only whether one is set.
#[test]
fn debug_hides_passwords() {
    let create = LobbyRequest::CreateGame {
        name: "cows".into(),
        password: "hunter2".into(),
        difficulty: 1,
        max_players: 0,
        place: Placement::Automatic,
        description: String::new(),
        level_min: 0,
        level_max: 0,
    };
    let text = format!("{create:?}");
    assert!(!text.contains("hunter2"), "{text}");
    assert!(text.contains("<redacted>"), "{text}");
    assert!(text.contains("cows"), "{text}");

    let join = LobbyRequest::JoinGame {
        game_id: 9,
        password: "swordfish".into(),
    };
    let text = format!("{join:?}");
    assert!(!text.contains("swordfish"), "{text}");
    assert!(text.contains("<redacted>"), "{text}");

    // An empty password stays visibly empty.
    let open = LobbyRequest::CreateGame {
        name: "cows".into(),
        password: String::new(),
        difficulty: 1,
        max_players: 0,
        place: Placement::Automatic,
        description: String::new(),
        level_min: 0,
        level_max: 0,
    };
    assert!(format!("{open:?}").contains("password: \"\""), "{open:?}");
}

/// A join ticket never reaches a log.
#[test]
fn debug_hides_tickets() {
    let ok = LobbyEvent::JoinOk {
        game_id: 7,
        ticket: [0xAB; 16],
        difficulty: 0,
    };
    let text = format!("{ok:?}");
    assert!(!text.contains("171"), "{text}");
    assert!(!text.contains("ab"), "{text}");
    assert!(text.contains("<redacted>"), "{text}");
    assert!(text.contains("game_id: 7"), "{text}");

    // The rest of the message is not secret.
    let chat = LobbyEvent::ChatLine {
        from: "Ann".into(),
        text: "hi".into(),
        at: 5,
    };
    let text = format!("{chat:?}");
    assert!(text.contains("hi") && text.contains("Ann"), "{text}");
}

fn join_node() -> JoinNode {
    JoinNode {
        game_id: 7,
        node: "eu-1".into(),
        addr: "203.0.113.5:6113".into(),
        node_cert_sha256: [0x5A; 32],
        player_hosted: false,
        reach: reach::DIRECT,
        ticket: vec![0xAB; 190],
        difficulty: 1,
    }
}

/// `JoinNode` (0x4B, contracts 11.4): its layout and round trip.
#[test]
fn join_node_layout_and_round_trip() {
    let j = JoinNode {
        game_id: 1,
        node: "n".into(),
        addr: "a:1".into(),
        node_cert_sha256: [9; 32],
        player_hosted: true,
        reach: reach::PUNCH,
        ticket: vec![1, 2],
        difficulty: 2,
    };
    let body = LobbyEvent::JoinNode(j.clone()).body();
    let mut want = vec![0x4B, 1, 0, 0, 0, 1, b'n', 3, b'a', b':', b'1'];
    want.extend_from_slice(&[9; 32]);
    want.extend_from_slice(&[1, 1, 2, 1, 2, 2]);
    assert_eq!(body, want);
    assert_eq!(LobbyEvent::parse(&body), Ok(LobbyEvent::JoinNode(j)));
    let big = LobbyEvent::JoinNode(join_node());
    assert_eq!(LobbyEvent::parse(&big.body()), Ok(big));
}

#[test]
fn join_node_refuses_bad_fields() {
    for bad in [
        JoinNode {
            node: String::new(),
            ..join_node()
        },
        JoinNode {
            node: "n".repeat(MAX_NODE_ID + 1),
            ..join_node()
        },
        JoinNode {
            addr: "a".repeat(MAX_NODE_ADDR + 1),
            ..join_node()
        },
        JoinNode {
            reach: 2,
            ..join_node()
        },
        JoinNode {
            ticket: Vec::new(),
            ..join_node()
        },
        JoinNode {
            difficulty: DIFFICULTIES,
            ..join_node()
        },
    ] {
        let body = LobbyEvent::JoinNode(bad.clone()).body();
        assert!(LobbyEvent::parse(&body).is_err(), "{bad:?}");
    }
    let mut cut = LobbyEvent::JoinNode(join_node()).body();
    cut.pop();
    assert!(LobbyEvent::parse(&cut).is_err());
}

/// A game server's ticket never reaches a log either.
#[test]
fn debug_hides_join_node_tickets() {
    let text = format!("{:?}", LobbyEvent::JoinNode(join_node()));
    assert!(!text.contains("171"), "{text}");
    assert!(!text.contains("ab, "), "{text}");
    assert!(text.contains("<redacted>"), "{text}");
    assert!(text.contains("eu-1"), "{text}");
}
