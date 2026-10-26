//! The lobby's rules on a set clock: each test names the stated rule it
//! checks (multiplayer contracts sections 3.6, 3.7, 5 and 6; lobby
//! contract section 7, "LC7").

use super::*;

fn ch(name: &str, progression: u16, flags: u16) -> LobbyChar {
    at_level(name, 1, progression, flags)
}

fn at_level(name: &str, level: u8, progression: u16, flags: u16) -> LobbyChar {
    let st = (progression << 8) | flags;
    LobbyChar {
        name: name.into(),
        level,
        status: st,
        difficulties: status::difficulties(st),
    }
}

const EXP: u16 = status::EXPANSION;

fn ng<'a>(name: &'a str, password: &'a str, difficulty: u8, max_players: u8) -> NewGame<'a> {
    NewGame {
        name,
        password,
        difficulty,
        max_players,
        description: "",
        level_min: 0,
        level_max: 0,
    }
}

/// A Normal game with a level range.
fn ranged<'a>(name: &'a str, level_min: u8, level_max: u8) -> NewGame<'a> {
    NewGame {
        level_min,
        level_max,
        ..ng(name, "", 0, 0)
    }
}

fn open_new(s: &mut LobbyState, session: u64, c: &LobbyChar, new: NewGame) -> u32 {
    let g = s.check_create(session, c, new, Instant::now()).unwrap();
    let id = g.id;
    s.opened(g);
    s.players(id, 1);
    id
}

fn code(r: Result<OpenGame, Refusal>) -> ErrorCode {
    r.unwrap_err().code
}

fn state() -> LobbyState {
    LobbyState::new(Limits::default())
}

fn open(s: &mut LobbyState, session: u64, c: &LobbyChar, name: &str, pw: &str, d: u8) -> u32 {
    let g = s
        .check_create(session, c, ng(name, pw, d, 0), Instant::now())
        .unwrap();
    let id = g.id;
    s.opened(g);
    s.players(id, 1);
    id
}

/// Rule (6): CreateGame at difficulty d needs bit d of the character's
/// mask; Normal is always open.
#[test]
fn create_needs_the_unlocked_difficulty() {
    let mut s = state();
    let fresh = ch("Fresh", 0, EXP);
    let now = Instant::now();
    assert_eq!(
        s.check_create(1, &fresh, ng("nm", "", 1, 0), now)
            .unwrap_err()
            .code,
        ErrorCode::DIFFICULTY_LOCKED
    );
    assert!(s.check_create(1, &fresh, ng("norm", "", 0, 0), now).is_ok());
    let nm = ch("Nm", 5, EXP);
    assert!(s.check_create(2, &nm, ng("nm", "", 1, 0), now).is_ok());
    assert_eq!(
        s.check_create(2, &nm, ng("hell", "", 2, 0), now)
            .unwrap_err()
            .code,
        ErrorCode::DIFFICULTY_LOCKED
    );
    // Classic: Nightmare at 4, Hell at 8.
    let classic = ch("Old", 8, 0);
    assert!(s
        .check_create(3, &classic, ng("hell", "", 2, 0), now)
        .is_ok());
    assert_eq!(
        s.check_create(3, &classic, ng("x", "", 3, 0), now)
            .unwrap_err()
            .code,
        ErrorCode::DIFFICULTY_LOCKED
    );
}

/// Rule (6): JoinGame needs the game's difficulty unlocked, the same kind,
/// and a living character; the list shows such games with the reason (LC7:
/// the other kind too).
#[test]
fn join_rules_and_list_reasons() {
    let mut s = state();
    let host = ch("Host", 10, EXP);
    let hell = open(&mut s, 1, &host, "hell", "", 2);
    let fresh = ch("Fresh", 0, EXP);
    let now = Instant::now();
    assert_eq!(
        s.check_join(2, &fresh, hell, "", now).unwrap_err().code,
        ErrorCode::DIFFICULTY_LOCKED
    );
    let list = s.list_for(&fresh);
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].joinable, ErrorCode::DIFFICULTY_LOCKED.0);
    // Another kind: listed with the reason, refused.
    let hc = ch("Hc", 10, EXP | status::HARDCORE);
    let list = s.list_for(&hc);
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].joinable, ErrorCode::WRONG_GAME_KIND.0);
    assert_eq!(
        s.check_join(3, &hc, hell, "", now).unwrap_err().code,
        ErrorCode::WRONG_GAME_KIND
    );
    let classic = ch("Classic", 10, 0);
    assert_eq!(
        s.check_join(4, &classic, hell, "", now).unwrap_err().code,
        ErrorCode::WRONG_GAME_KIND
    );
    // Dead hardcore: neither create nor join.
    let dead = ch("Dead", 10, EXP | status::HARDCORE | status::DEAD);
    assert_eq!(
        s.check_create(5, &dead, ng("x", "", 0, 0), now)
            .unwrap_err()
            .code,
        ErrorCode::DEAD_HARDCORE
    );
    assert_eq!(
        s.check_join(5, &dead, hell, "", now).unwrap_err().code,
        ErrorCode::DEAD_HARDCORE
    );
    // The right character joins.
    assert!(s.check_join(6, &ch("Pro", 10, EXP), hell, "", now).is_ok());
    assert_eq!(
        s.check_join(6, &ch("Pro", 10, EXP), 99, "", now)
            .unwrap_err()
            .code,
        ErrorCode::GAME_NOT_FOUND
    );
}

/// Rule (owner decision 3, 3.6): a blank password is a public game; a
/// password game needs it; the list never shows it.
#[test]
fn password_rule() {
    let mut s = state();
    let host = ch("Host", 0, EXP);
    let public = open(&mut s, 1, &host, "open", "", 0);
    let locked = open(&mut s, 2, &ch("Host2", 0, EXP), "secret", "moo", 0);
    let p = ch("P", 0, EXP);
    let now = Instant::now();
    assert!(s.check_join(3, &p, public, "", now).is_ok());
    assert!(s.check_join(3, &p, public, "anything", now).is_ok());
    assert_eq!(
        s.check_join(3, &p, locked, "", now).unwrap_err().code,
        ErrorCode::PASSWORD_NEEDED
    );
    assert_eq!(
        s.check_join(3, &p, locked, "oink", now).unwrap_err().code,
        ErrorCode::WRONG_PASSWORD
    );
    assert!(s.check_join(3, &p, locked, "moo", now).is_ok());
    let list = s.list_for(&p);
    assert!(list.iter().any(|g| g.game_id == locked && g.locked));
    assert!(list.iter().any(|g| g.game_id == public && !g.locked));
}

/// Rule (5): wrong join passwords 5 per minute per session.
#[test]
fn wrong_password_rate_limit() {
    let mut s = state();
    let locked = open(&mut s, 1, &ch("Host", 0, EXP), "secret", "moo", 0);
    let p = ch("P", 0, EXP);
    let t0 = Instant::now();
    for i in 0..5 {
        assert_eq!(
            s.check_join(2, &p, locked, "no", t0 + Duration::from_secs(i))
                .unwrap_err()
                .code,
            ErrorCode::WRONG_PASSWORD
        );
    }
    let r = s
        .check_join(2, &p, locked, "moo", t0 + Duration::from_secs(10))
        .unwrap_err();
    assert_eq!(r.code, ErrorCode::RATE_LIMITED);
    assert_eq!(r.wait_s, 50);
    // A minute after the first try it fits again.
    assert!(s
        .check_join(2, &p, locked, "moo", t0 + Duration::from_secs(61))
        .is_ok());
    // Another session has its own limit.
    assert!(s.check_join(3, &p, locked, "moo", t0).is_ok());
}

/// Rule (5): CreateGame 5 per minute per session; `max_games` in total.
#[test]
fn create_limits() {
    let mut s = LobbyState::new(Limits {
        max_games: 2,
        ..Limits::default()
    });
    let c = ch("C", 0, EXP);
    let t0 = Instant::now();
    let a = s.check_create(1, &c, ng("a", "", 0, 0), t0).unwrap();
    s.opened(a);
    let b = s.check_create(1, &c, ng("b", "", 0, 0), t0).unwrap();
    s.opened(b);
    assert_eq!(
        s.check_create(1, &c, ng("c", "", 0, 0), t0)
            .unwrap_err()
            .code,
        ErrorCode::TOO_MANY_GAMES
    );
    let mut s = state();
    for i in 0..5 {
        assert!(s
            .check_create(1, &c, ng(&format!("g{i}"), "", 0, 0), t0)
            .is_ok());
    }
    assert_eq!(
        s.check_create(1, &c, ng("g5", "", 0, 0), t0)
            .unwrap_err()
            .code,
        ErrorCode::RATE_LIMITED
    );
}

/// Rule (6): game names 1-15 letters, digits, space, - and _; unique among
/// open games in any case; passwords at most 15 bytes.
#[test]
fn game_names() {
    let mut s = state();
    let c = ch("C", 0, EXP);
    let now = Instant::now();
    open(&mut s, 1, &c, "Baal Run", "", 0);
    for (name, code) in [
        ("", ErrorCode::BAD_GAME_NAME),
        ("   ", ErrorCode::BAD_GAME_NAME),
        ("sixteen chars xx", ErrorCode::BAD_GAME_NAME),
        ("bad!", ErrorCode::BAD_GAME_NAME),
        ("baal run", ErrorCode::GAME_NAME_TAKEN),
        ("BAAL RUN", ErrorCode::GAME_NAME_TAKEN),
    ] {
        assert_eq!(
            s.check_create(2, &ch("D", 0, EXP), ng(name, "", 0, 0), now)
                .unwrap_err()
                .code,
            code,
            "{name:?}"
        );
    }
    assert!(s
        .check_create(3, &ch("E", 0, EXP), ng("ok-name_1", "", 0, 0), now)
        .is_ok());
}

/// Rule (3.7): a ticket is valid once, for 30 s, on its own session; its
/// seat counts toward the game's players; a character is in one game.
#[test]
fn tickets() {
    let mut s = LobbyState::new(Limits {
        default_players: 2,
        ..Limits::default()
    });
    let host = ch("Host", 0, EXP);
    let g = open(&mut s, 1, &host, "two", "", 0);
    let p = ch("P", 0, EXP);
    let t0 = Instant::now();
    assert!(s.check_join(2, &p, g, "", t0).is_ok());
    s.issue(2, &p, g, [1; 16], t0).unwrap();
    assert!(!s.listening(2));
    // The held seat fills the 2-player game.
    let q = ch("Q", 0, EXP);
    assert_eq!(
        s.check_join(3, &q, g, "", t0).unwrap_err().code,
        ErrorCode::GAME_FULL
    );
    // The same character and the same session are busy.
    assert_eq!(
        s.check_join(4, &p, g, "", t0).unwrap_err().code,
        ErrorCode::CHAR_IN_GAME
    );
    assert_eq!(
        s.check_join(2, &q, g, "", t0).unwrap_err().code,
        ErrorCode::ALREADY_IN_GAME
    );
    assert!(s.character_in_game("p"));
    // Wrong session, then the right one once.
    assert_eq!(
        s.redeem(9, [1; 16], t0).unwrap_err().refusal.code,
        ErrorCode::TICKET_INVALID
    );
    let t = s.redeem(2, [1; 16], t0 + Duration::from_secs(29)).unwrap();
    assert_eq!((t.game, t.character.as_str()), (g, "P"));
    assert_eq!(
        s.redeem(2, [1; 16], t0).unwrap_err().refusal.code,
        ErrorCode::TICKET_INVALID
    );
    // P's game counted the seat, then P left: Q's seat is free.
    s.players(g, 2);
    s.left("p");
    s.players(g, 1);
    // An expired ticket frees the character and the seat.
    s.issue(5, &q, g, [2; 16], t0).unwrap();
    assert_eq!(
        s.redeem(5, [2; 16], t0 + TICKET_TTL)
            .unwrap_err()
            .refusal
            .code,
        ErrorCode::TICKET_INVALID
    );
    assert!(!s.character_in_game("q"));
    s.issue(5, &q, g, [3; 16], t0).unwrap();
    assert_eq!(s.expire(t0 + TICKET_TTL), vec![([3; 16], g)]);
    assert!(!s.character_in_game("q"));
}

/// Rule (6): a closed game leaves the list and frees its players.
#[test]
fn closed_game_frees_everything() {
    let mut s = state();
    let host = ch("Host", 0, EXP);
    let g = open(&mut s, 1, &host, "g", "", 0);
    let p = ch("P", 0, EXP);
    s.issue(2, &p, g, [1; 16], Instant::now()).unwrap();
    s.subscribe(3, &ch("L", 0, EXP));
    assert_eq!(s.views_of(g).len(), 1);
    s.closed(g);
    assert!(s.game(g).is_none());
    assert!(!s.character_in_game("p"));
    assert_eq!(s.views_of(g), vec![(3, None)]);
    assert!(s.list_for(&p).is_empty());
}

/// Rule (3.6, LC7): every listener sees updates of every game, each with
/// its own `joinable`.
#[test]
fn views_follow_each_listener() {
    let mut s = state();
    let fresh = ch("Fresh", 0, EXP);
    let pro = ch("Pro", 10, EXP);
    let hc = ch("Hc", 10, EXP | status::HARDCORE);
    s.subscribe(1, &fresh);
    s.subscribe(2, &pro);
    s.subscribe(3, &hc);
    let g = open(&mut s, 9, &ch("Host", 10, EXP), "hell", "", 2);
    let v = s.views_of(g);
    assert_eq!(v.len(), 3);
    assert_eq!(
        v[0].1.as_ref().unwrap().joinable,
        ErrorCode::DIFFICULTY_LOCKED.0
    );
    assert_eq!(v[1].1.as_ref().unwrap().joinable, 0);
    assert_eq!(
        v[2].1.as_ref().unwrap().joinable,
        ErrorCode::WRONG_GAME_KIND.0
    );
    s.unsubscribe(1);
    assert_eq!(s.views_of(g).len(), 2);
}

/// Rule (LC7): every open game is listed to every character, also the
/// other hardcore / softcore and classic / expansion kinds, greyed with
/// `WrongGameKind`; a game of its own kind it can join says 0.
#[test]
fn other_kind_listed_with_reason() {
    let mut s = state();
    const HC: u16 = status::HARDCORE;
    let games = [
        ("sc-exp", EXP),
        ("hc-exp", EXP | HC),
        ("sc-cls", 0),
        ("hc-cls", HC),
    ];
    for (i, (name, flags)) in games.iter().enumerate() {
        open(
            &mut s,
            10 + i as u64,
            &ch(&format!("Host{i}"), 0, *flags),
            name,
            "",
            0,
        );
    }
    for (name, flags) in games {
        let viewer = ch("Viewer", 0, flags);
        let list = s.list_for(&viewer);
        assert_eq!(list.len(), 4, "{name} sees every game");
        for g in &list {
            let want = if g.name == name {
                0
            } else {
                ErrorCode::WRONG_GAME_KIND.0
            };
            assert_eq!(g.joinable, want, "{name} viewing {}", g.name);
            assert_eq!(
                (g.hardcore, g.expansion),
                (
                    games.iter().find(|x| x.0 == g.name).unwrap().1 & HC != 0,
                    games.iter().find(|x| x.0 == g.name).unwrap().1 & EXP != 0
                ),
                "{} carries its own kind",
                g.name
            );
        }
        // Listening gives the same list.
        assert_eq!(s.subscribe(1, &viewer), list);
    }
}

/// Rule (LC7): a game's level range (0 no bound) refuses characters
/// outside it on join, lists them `LevelRange`, and on create needs
/// min <= max with the creator's own level inside.
#[test]
fn level_range_refused() {
    let mut s = state();
    let now = Instant::now();
    let host = at_level("Host", 50, 0, EXP);
    // Create: min > max, and the creator outside its own range.
    assert_eq!(
        code(s.check_create(1, &host, ranged("bad", 60, 40), now)),
        ErrorCode::LEVEL_RANGE
    );
    assert_eq!(
        code(s.check_create(1, &host, ranged("low", 1, 49), now)),
        ErrorCode::LEVEL_RANGE
    );
    assert_eq!(
        code(s.check_create(1, &host, ranged("high", 51, 0), now)),
        ErrorCode::LEVEL_RANGE
    );
    let g = open_new(&mut s, 1, &host, ranged("mid", 40, 70));
    let only_min = open_new(
        &mut s,
        2,
        &at_level("Host2", 50, 0, EXP),
        ranged("min", 30, 0),
    );
    let only_max = open_new(
        &mut s,
        3,
        &at_level("Host3", 50, 0, EXP),
        ranged("max", 0, 60),
    );
    let info = s.list_for(&host);
    let mid = info.iter().find(|i| i.game_id == g).unwrap();
    assert_eq!((mid.level_min, mid.level_max, mid.joinable), (40, 70, 0));
    // Join: the bounds are inclusive; 0 is no bound.
    for (level, game, ok) in [
        (39, g, false),
        (40, g, true),
        (70, g, true),
        (71, g, false),
        (29, only_min, false),
        (99, only_min, true),
        (1, only_max, true),
        (61, only_max, false),
    ] {
        let c = at_level("P", level, 0, EXP);
        let want = if ok {
            Ok(())
        } else {
            Err(Refusal::from(ErrorCode::LEVEL_RANGE))
        };
        assert_eq!(
            s.check_join(9, &c, game, "", now),
            want,
            "level {level} game {game}"
        );
        let listed = s
            .list_for(&c)
            .into_iter()
            .find(|i| i.game_id == game)
            .unwrap();
        let want = if ok { 0 } else { ErrorCode::LEVEL_RANGE.0 };
        assert_eq!(listed.joinable, want, "level {level} game {game} listed");
    }
}

/// Rule (LC7): `joinable` gives the first reason in the order dead
/// hardcore, kind, difficulty, level range, full; each reason shows once
/// the earlier ones are out of the way.
#[test]
fn refusal_order() {
    let mut s = LobbyState::new(Limits {
        default_players: 1,
        ..Limits::default()
    });
    const HC: u16 = status::HARDCORE;
    // A full Hell hardcore game for levels 60-70.
    let host = at_level("Host", 65, 10, EXP | HC);
    let g = open_new(
        &mut s,
        1,
        &host,
        NewGame {
            difficulty: 2,
            ..ranged("hell", 60, 70)
        },
    );
    let now = Instant::now();
    let steps = [
        (
            at_level("A", 1, 0, EXP | HC | status::DEAD),
            ErrorCode::DEAD_HARDCORE,
        ),
        (at_level("B", 1, 0, EXP), ErrorCode::WRONG_GAME_KIND),
        (at_level("C", 1, 0, EXP | HC), ErrorCode::DIFFICULTY_LOCKED),
        (at_level("D", 1, 10, EXP | HC), ErrorCode::LEVEL_RANGE),
        (at_level("E", 65, 10, EXP | HC), ErrorCode::GAME_FULL),
    ];
    for (i, (c, want)) in steps.iter().enumerate() {
        let listed = &s.list_for(c)[0];
        assert_eq!(listed.joinable, want.0, "{}", c.name);
        assert_eq!(
            s.check_join(20 + i as u64, c, g, "", now).unwrap_err().code,
            *want,
            "{}",
            c.name
        );
    }
    // With a free seat the last one joins.
    s.players(g, 0);
    assert_eq!(s.list_for(&steps[4].0)[0].joinable, 0);
    assert!(s.check_join(30, &steps[4].0, g, "", now).is_ok());
}

/// Rule (LC7): the description (0-40 bytes, no control characters,
/// trimmed; else `BadText`), the level range, the server and the creation
/// time reach the list entry.
#[test]
fn list_entry_fields() {
    let mut s = state();
    let now = Instant::now();
    let host = at_level("Host", 20, 0, EXP);
    let long = "x".repeat(41);
    for bad in [long.as_str(), "line\nbreak", "tab\there", "bell\u{7}"] {
        let new = NewGame {
            description: bad,
            ..ng("bad", "", 0, 0)
        };
        assert_eq!(
            code(s.check_create(1, &host, new, now)),
            ErrorCode::BAD_TEXT,
            "{bad:?}"
        );
    }
    let forty = "y".repeat(40);
    let new = NewGame {
        description: &forty,
        ..ng("forty", "", 0, 0)
    };
    assert!(s.check_create(1, &host, new, now).is_ok());
    let new = NewGame {
        description: "  cows, fast  ",
        ..ranged("cows", 10, 30)
    };
    let mut g = s.check_create(1, &host, new, now).unwrap();
    assert_eq!(g.server, GameServer::default());
    g.server = GameServer::official("eu-n1", "EU N1");
    g.created_ms = 1_700_000_000_123;
    let id = g.id;
    s.opened(g);
    s.players(id, 1);
    let viewer = at_level("Viewer", 5, 0, EXP);
    let info = s
        .list_for(&viewer)
        .into_iter()
        .find(|i| i.game_id == id)
        .unwrap();
    assert_eq!(info.description, "cows, fast");
    assert_eq!((info.level_min, info.level_max), (10, 30));
    assert_eq!(info.joinable, ErrorCode::LEVEL_RANGE.0);
    assert_eq!(info.node_id, "eu-n1");
    assert_eq!(info.server, "EU N1");
    assert!(!info.player_hosted);
    assert_eq!(info.host_rtt_ms, 0);
    assert_eq!(info.created_ms, 1_700_000_000_123);
    // The entry fits the wire as a v6 GameInfo.
    let body = d2_net::msg::lobby::LobbyEvent::GameEntry(info.clone()).body();
    assert_eq!(
        d2_net::msg::lobby::LobbyEvent::parse(&body).unwrap(),
        d2_net::msg::lobby::LobbyEvent::GameEntry(info)
    );
}

/// An official server's label is its name, or its id when it has none.
#[test]
fn official_server_label() {
    assert_eq!(GameServer::official("eu-n1", "EU N1").label, "EU N1");
    assert_eq!(GameServer::official("eu-n1", " ").label, "eu-n1");
}

/// Rule (5): chat 5 lines per 10 s per session.
#[test]
fn chat_limit() {
    let mut s = state();
    let t0 = Instant::now();
    for _ in 0..5 {
        assert!(s.check_chat(1, t0).is_ok());
    }
    assert_eq!(
        s.check_chat(1, t0).unwrap_err().code,
        ErrorCode::RATE_LIMITED
    );
    assert!(s.check_chat(1, t0 + Duration::from_secs(10)).is_ok());
}

#[test]
fn password_compare() {
    assert!(password_eq("moo", "moo"));
    assert!(!password_eq("moo", "mo"));
    assert!(!password_eq("moo", "moO"));
    assert!(!password_eq("", "x"));
    assert!(password_eq("", ""));
}

/// The session's end drops its tickets but not a game it is playing in.
#[test]
fn session_end() {
    let mut s = state();
    let g = open(&mut s, 1, &ch("Host", 0, EXP), "g", "", 0);
    let p = ch("P", 0, EXP);
    s.issue(2, &p, g, [1; 16], Instant::now()).unwrap();
    s.session_ended(2);
    assert!(!s.character_in_game("p"));
    assert_eq!(
        s.redeem(2, [1; 16], Instant::now())
            .unwrap_err()
            .refusal
            .code,
        ErrorCode::TICKET_INVALID
    );
}

/// Rule (10.1): `limits = test` divides the windows by 60, the counts stay.
#[test]
fn test_windows_are_shorter() {
    let mut s = LobbyState::new(Limits {
        test_windows: true,
        ..Limits::default()
    });
    let t0 = Instant::now();
    for _ in 0..5 {
        assert!(s.check_chat(1, t0).is_ok());
    }
    assert_eq!(
        s.check_chat(1, t0).unwrap_err().code,
        ErrorCode::RATE_LIMITED
    );
    // 10 s / 60: free again after a sixth of a second.
    assert!(s.check_chat(1, t0 + Duration::from_millis(200)).is_ok());
}

/// Rule (3.1): a player who left the game is free to join another one; the
/// session that took it there is free too.
#[test]
fn left_frees_character_and_session() {
    let mut s = state();
    let g = open(&mut s, 1, &ch("Host", 0, EXP), "g", "", 0);
    let h = open(&mut s, 9, &ch("Host2", 0, EXP), "h", "", 0);
    let p = ch("P", 0, EXP);
    let now = Instant::now();
    s.issue(2, &p, g, [1; 16], now).unwrap();
    s.redeem(2, [1; 16], now).unwrap();
    assert_eq!(
        s.check_join(2, &p, h, "", now).unwrap_err().code,
        ErrorCode::ALREADY_IN_GAME
    );
    s.left("p");
    assert!(s.check_join(2, &p, h, "", now).is_ok());
}

/// Rule (6): a redeemed seat is taken until the game's `Players` counts
/// it; the gap cannot overbook the game.
#[test]
fn a_redeemed_seat_stays_taken_until_players_arrives() {
    let mut s = state();
    let now = Instant::now();
    let host = ch("Host", 0, EXP);
    let g = s.check_create(1, &host, ng("g", "", 0, 2), now).unwrap();
    let id = g.id;
    s.opened(g);
    s.players(id, 1);
    let p = ch("P", 0, EXP);
    let q = ch("Q", 0, EXP);
    s.issue(2, &p, id, [1; 16], now).unwrap();
    assert_eq!(
        s.check_join(3, &q, id, "", now).unwrap_err().code,
        ErrorCode::GAME_FULL
    );
    // The ticket is redeemed, but `players` still says 1: the seat is held.
    s.redeem(2, [1; 16], now).unwrap();
    assert_eq!(
        s.check_join(3, &q, id, "", now).unwrap_err().code,
        ErrorCode::GAME_FULL
    );
    // The game counts both players: the seat is the game's own count now.
    s.players(id, 2);
    assert_eq!(
        s.check_join(3, &q, id, "", now).unwrap_err().code,
        ErrorCode::GAME_FULL
    );
    s.left("p");
    s.players(id, 1);
    assert!(s.check_join(3, &q, id, "", now).is_ok());
}

/// Rule (5): no secret reaches a log: the password of a game or a request,
/// and the ticket keys of the state, print as markers.
#[test]
fn debug_hides_lobby_secrets() {
    let mut s = state();
    let now = Instant::now();
    let host = ch("Host", 0, EXP);
    let g = s
        .check_create(1, &host, ng("cows", "hunter2", 0, 0), now)
        .unwrap();
    let text = format!("{g:?}");
    assert!(!text.contains("hunter2"), "{text}");
    assert!(text.contains("locked: true"), "{text}");
    let request = ng("cows", "hunter2", 0, 0);
    let text = format!("{request:?}");
    assert!(!text.contains("hunter2"), "{text}");
    assert!(text.contains("<redacted>"), "{text}");
    let id = g.id;
    s.opened(g);
    s.players(id, 1);
    let p = ch("P", 0, EXP);
    s.issue(2, &p, id, [0xAB; 16], now).unwrap();
    let text = format!("{s:?}");
    assert!(!text.contains("171"), "{text}");
    assert!(text.contains("tickets: 1"), "{text}");
}

/// Plan N4: a seat on a game server. Its ticket is the lobby's key for
/// the seat: once taken it is no ticket any more, and the seat still fills
/// the game until the game reports its players; once ended, the character
/// and the session are free.
#[test]
fn game_server_seats() {
    let mut s = LobbyState::new(Limits {
        default_players: 2,
        ..Limits::default()
    });
    let host = ch("Host", 0, EXP);
    let g = open(&mut s, 1, &host, "two", "", 0);
    let p = ch("P", 0, EXP);
    let t0 = Instant::now();
    s.issue(2, &p, g, [1; 16], t0).unwrap();
    assert_eq!(s.session_game(2), Some(g));
    s.seated(&[1; 16]);
    assert_eq!(
        s.redeem(2, [1; 16], t0).unwrap_err().refusal.code,
        ErrorCode::TICKET_INVALID
    );
    assert_eq!(
        s.check_join(3, &ch("Q", 0, EXP), g, "", t0)
            .unwrap_err()
            .code,
        ErrorCode::GAME_FULL
    );
    assert!(s.character_in_game("p"));
    assert_eq!(s.session_game(2), Some(g));
    s.unseat(&[1; 16], "P");
    assert!(!s.character_in_game("p"));
    assert_eq!(s.session_game(2), None);
}
