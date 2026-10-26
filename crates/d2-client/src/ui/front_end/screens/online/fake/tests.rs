//! The fake server's rules: every request gets its answer, and the realm
//! lobby's rules (rooms, DMs, guild ranks, friends' places, the ladder's
//! order and pages, the game list's reasons) hold.

use d2_formats::d2s::status;

use super::super::link::{
    guild_rank, ladder_flag, ladder_split, lobby_code, room, Asked, Done, ErrorCode, Event,
    FriendOp, GameInfo, GuildOp, GuildQuery, LadderAt, LadderPage, LadderQuery, Placement, Privacy,
    Request, Where,
};
use super::*;

/// A logged-in account "Nugwa" with "Werwer" (a level 34 Barbarian,
/// Nightmare open, expansion) selected; nothing queued.
fn scene() -> FakeServer {
    let mut f = FakeServer::default();
    f.add_account(
        "Nugwa",
        "password1",
        vec![
            character("Werwer", 4, 34, 5, status::EXPANSION),
            character("Hardy", 1, 20, 0, status::EXPANSION | status::HARDCORE),
        ],
    );
    ask(&mut f, Request::Connect { index: 0 });
    ask(
        &mut f,
        Request::Login {
            account: "Nugwa".into(),
            password: "password1".into(),
        },
    );
    ask(
        &mut f,
        Request::CharSelect {
            name: "Werwer".into(),
        },
    );
    f
}

fn ask(f: &mut FakeServer, r: Request) -> Vec<Event> {
    f.send(r);
    std::iter::from_fn(|| f.poll()).collect()
}

fn refusal(evs: &[Event]) -> Option<(Asked, ErrorCode)> {
    evs.iter().find_map(|e| match e {
        Event::Refused { asked, failure } => Some((*asked, failure.code)),
        _ => None,
    })
}

fn done(evs: &[Event]) -> Option<&Done> {
    evs.iter().find_map(|e| match e {
        Event::Done(d) => Some(d),
        _ => None,
    })
}

const ALL_ROOMS: u16 = 0b111;

/// Every request of the link, once. The match has no wildcard: a new
/// request doesn't compile until it has a sample here (and so an answer
/// checked below).
fn samples() -> Vec<Request> {
    fn covered(r: &Request) {
        match r {
            Request::Connect { .. }
            | Request::Register { .. }
            | Request::Login { .. }
            | Request::CharList
            | Request::CharCreate { .. }
            | Request::CharDelete { .. }
            | Request::CharSelect { .. }
            | Request::ListGames
            | Request::CreateGame { .. }
            | Request::JoinGame { .. }
            | Request::LeaveLobby
            | Request::Chat { .. }
            | Request::Logout
            | Request::SocialHello { .. }
            | Request::Say { .. }
            | Request::Whisper { .. }
            | Request::DmConversations { .. }
            | Request::DmHistory { .. }
            | Request::DmRead { .. }
            | Request::UserList { .. }
            | Request::GuildGet(_)
            | Request::GuildCreate { .. }
            | Request::GuildInvite { .. }
            | Request::GuildAnswer { .. }
            | Request::GuildManage(_)
            | Request::FriendsGet { .. }
            | Request::FriendEdit(_)
            | Request::LadderSeasons
            | Request::LadderQuery(_) => {}
        }
    }
    let list = vec![
        Request::CharList,
        Request::ListGames,
        Request::Chat { text: "hi".into() },
        Request::SocialHello { rooms: ALL_ROOMS },
        Request::Say {
            room: room::GLOBAL,
            emote: false,
            text: "hello realm".into(),
        },
        Request::Whisper {
            to: "Kaern".into(),
            text: "baal?".into(),
        },
        Request::DmConversations { page: 0 },
        Request::DmHistory {
            peer: "Kaern".into(),
            before_dm: 0,
            rows: 50,
        },
        Request::DmRead {
            peer_account: "kaern".into(),
            up_to_dm: 3,
        },
        Request::UserList {
            room: room::GLOBAL,
            watch: true,
        },
        Request::GuildGet(GuildQuery::Mine),
        Request::GuildGet(GuildQuery::Invites),
        Request::GuildGet(GuildQuery::ByTag("pk".into())),
        Request::GuildInvite {
            who: "Mirelle".into(),
        },
        Request::GuildManage(GuildOp::Motd("Cows at nine.".into())),
        Request::GuildManage(GuildOp::Promote("Ashka".into())),
        Request::GuildManage(GuildOp::Demote("Ashka".into())),
        Request::GuildManage(GuildOp::Kick("Ashka".into())),
        Request::GuildManage(GuildOp::MakeLeader("Kaern".into())),
        Request::GuildManage(GuildOp::Leave),
        Request::GuildAnswer {
            guild: 2,
            accept: false,
        },
        Request::GuildCreate {
            tag: "NEW".into(),
            name: "New Guild".into(),
        },
        Request::GuildManage(GuildOp::Disband("new".into())),
        Request::FriendsGet { ignored: false },
        Request::FriendsGet { ignored: true },
        Request::FriendEdit(FriendOp::Add("Grim".into())),
        Request::FriendEdit(FriendOp::Remove("Grim".into())),
        Request::FriendEdit(FriendOp::Ignore("Grim".into())),
        Request::FriendEdit(FriendOp::Unignore("*grim".into())),
        Request::FriendEdit(FriendOp::Privacy(Privacy {
            whispers: 1,
            show_where: 1,
        })),
        Request::LadderSeasons,
        Request::LadderQuery(LadderQuery {
            season: 1,
            split: ladder_split::EXPANSION,
            class: 255,
            at: LadderAt::Rank(1),
            rows: 50,
        }),
        Request::CreateGame {
            name: "cows".into(),
            password: String::new(),
            difficulty: 0,
            place: Placement::Automatic,
            description: "chill cows".into(),
            level_min: 30,
            level_max: 40,
        },
        Request::JoinGame {
            game_id: 1,
            password: String::new(),
        },
        Request::LeaveLobby,
        Request::CharCreate {
            name: "Fresh".into(),
            class: 2,
            hardcore: false,
            expansion: true,
            ladder: false,
        },
        Request::CharDelete {
            name: "Fresh".into(),
            password: "password1".into(),
        },
        Request::Logout,
        Request::Register {
            account: "Newbie".into(),
            password: "password2".into(),
        },
        Request::Login {
            account: "Nugwa".into(),
            password: "password1".into(),
        },
        Request::CharSelect {
            name: "Werwer".into(),
        },
        Request::Connect { index: 0 },
    ];
    list.iter().for_each(covered);
    list
}

/// Rule (lobby-ui L7): every request gets the fake's answer, in a scene
/// where each can go through: the realm lobby's with their own event
/// (never a refusal), the others as before. `LeaveLobby` and `Logout`
/// are the two the realm doesn't answer.
#[test]
fn fake_answers_every_request() {
    let mut f = scene();
    for r in samples() {
        let quiet = matches!(r, Request::LeaveLobby | Request::Logout);
        let asked = r.asked();
        let evs = ask(&mut f, r.clone());
        if quiet {
            assert!(evs.is_empty(), "{r:?}: {evs:?}");
            continue;
        }
        assert!(!evs.is_empty(), "no answer to {r:?}");
        assert_eq!(refusal(&evs), None, "{r:?} refused: {evs:?}");
        assert!(
            !evs.iter().any(|e| matches!(e, Event::Error(_))),
            "{r:?}: {evs:?}"
        );
        let Some(asked) = asked else { continue };
        let answered = evs.iter().any(|e| match (asked, e) {
            (Asked::SocialHello, Event::SocialState(_)) => true,
            (Asked::DmConversations, Event::DmConversations { .. }) => true,
            (Asked::DmHistory, Event::DmHistory { .. }) => true,
            (Asked::UserList, Event::Users { .. }) => true,
            (Asked::GuildGet, Event::Guild { .. })
            | (Asked::GuildGet, Event::GuildInvites(_))
            | (Asked::GuildGet, Event::GuildFound(_)) => true,
            (Asked::FriendsGet, Event::Friends { .. }) => true,
            (Asked::LadderSeasons, Event::LadderSeasons(_)) => true,
            (Asked::LadderQuery, Event::Ladder(_)) => true,
            (a, Event::Done(d)) => d.asked == a,
            _ => false,
        });
        assert!(answered, "{r:?} got {evs:?}");
    }
}

/// Rule (POLISH 1): the fake realm is "D2Gamma Test", no leftover name.
#[test]
fn the_fake_realm_is_d2gamma_test() {
    let mut f = FakeServer::default();
    assert_eq!(f.server_name, "D2Gamma Test");
    assert_eq!(
        ask(&mut f, Request::Connect { index: 0 }),
        vec![Event::Connected {
            server_name: "D2Gamma Test".into(),
            registration_open: true,
        }]
    );
}

/// Rule (contract 1, 8): a realm without the area answers `NotSupported`
/// as a refusal of that request, never as `Error` (which the old chat
/// reads as "chat off"); a session needs a login.
#[test]
fn areas_off_and_logged_out_are_refusals() {
    let mut f = scene();
    f.social_off = true;
    let evs = ask(&mut f, Request::FriendsGet { ignored: false });
    assert_eq!(
        refusal(&evs),
        Some((Asked::FriendsGet, ErrorCode::NOT_SUPPORTED))
    );
    f.ladder_off = true;
    let evs = ask(&mut f, Request::LadderSeasons);
    assert_eq!(
        refusal(&evs),
        Some((Asked::LadderSeasons, ErrorCode::NOT_SUPPORTED))
    );
    let mut f = FakeServer::default();
    let evs = ask(&mut f, Request::SocialHello { rooms: ALL_ROOMS });
    assert_eq!(
        refusal(&evs),
        Some((Asked::SocialHello, ErrorCode::NOT_LOGGED_IN))
    );
}

/// Rule (contract 3.1): the hello grants Global, Mod and (with a guild)
/// Guild, then each room's backlog; a line needs a selected character,
/// text and a granted room, and comes back as a room line.
#[test]
fn rooms_backlog_and_say_rules() {
    let mut f = scene();
    let evs = ask(&mut f, Request::SocialHello { rooms: ALL_ROOMS });
    let Some(Event::SocialState(st)) = evs.first() else {
        panic!("{evs:?}");
    };
    assert_eq!(st.rooms, ALL_ROOMS);
    assert_eq!(st.mod_label, "D2Gamma");
    assert_eq!(st.guild.as_ref().map(|g| g.tag.as_str()), Some("NUG"));
    assert_eq!((st.dm_unread, st.dm_conversations_unread), (1, 1));
    let backlog: Vec<u8> = evs
        .iter()
        .filter_map(|e| match e {
            Event::RoomLine(l) => Some(l.room),
            _ => None,
        })
        .collect();
    assert_eq!(backlog, [0, 0, 0, 1, 1, 2]);

    let say = |room, text: &str| Request::Say {
        room,
        emote: false,
        text: text.into(),
    };
    let evs = ask(&mut f, say(room::MOD, "  wtb ber  "));
    assert!(done(&evs).is_some());
    assert!(evs.iter().any(|e| matches!(
        e,
        Event::RoomLine(l) if l.text == "wtb ber" && l.who.char == "Werwer"
            && l.who.tag.as_deref() == Some("NUG")
    )));
    for (r, code) in [
        (say(room::GLOBAL, "   "), lobby_code::BAD_TEXT),
        (say(room::GLOBAL, &"x".repeat(201)), lobby_code::BAD_TEXT),
        (say(room::GAME, "hi"), lobby_code::ROOM_UNAVAILABLE),
    ] {
        assert_eq!(refusal(&ask(&mut f, r)), Some((Asked::Say, code)));
    }
    // Without a guild the Guild room is refused and not granted.
    f.social.my_guild = None;
    assert_eq!(
        refusal(&ask(&mut f, say(room::GUILD, "hi"))),
        Some((Asked::Say, lobby_code::NOT_IN_GUILD))
    );
    let evs = ask(&mut f, Request::SocialHello { rooms: ALL_ROOMS });
    assert!(matches!(&evs[0], Event::SocialState(s) if s.rooms == 0b011));
    // A muted account is told how long.
    f.social.muted_until_ms = Some(f.now_ms + 90_000);
    let evs = ask(&mut f, say(room::GLOBAL, "hi"));
    assert!(matches!(
        &evs[..],
        [Event::Refused { failure, .. }] if failure.code == lobby_code::MUTED && failure.wait_s == 90
    ));
    // At character select: friends and DMs work, rooms don't.
    let mut f = scene();
    f.selected = None;
    ask(&mut f, Request::SocialHello { rooms: ALL_ROOMS });
    assert_eq!(
        refusal(&ask(&mut f, say(room::GLOBAL, "hi"))),
        Some((Asked::Say, ErrorCode::NO_CHAR_SELECTED))
    );
    assert_eq!(
        refusal(&ask(&mut f, Request::FriendsGet { ignored: false })),
        None
    );
}

/// Rule (contract 3.2): a whisper to an offline player is stored; a live
/// one comes back as a DM line; history pages and read marks follow; a
/// stranger who takes whispers from nobody blocks it.
#[test]
fn whispers_history_and_read_marks() {
    let mut f = scene();
    let evs = ask(
        &mut f,
        Request::Whisper {
            to: "tobben".into(),
            text: "see you".into(),
        },
    );
    let d = done(&evs).expect("done");
    assert!(d.stored() && d.id.is_some());
    assert!(
        matches!(&evs[1], Event::DmLine(l) if l.outgoing && l.stored && l.peer_char == "Tobben")
    );

    let evs = ask(
        &mut f,
        Request::DmHistory {
            peer: "*KAERN".into(),
            before_dm: 3,
            rows: 1,
        },
    );
    assert!(matches!(
        &evs[..],
        [Event::DmHistory { peer, lines }] if peer == "*KAERN" && lines.len() == 1 && lines[0].dm == 2
    ));
    let evs = ask(&mut f, Request::DmConversations { page: 0 });
    let Some(Event::DmConversations { total, rows, .. }) = evs.first() else {
        panic!("{evs:?}");
    };
    assert_eq!(*total, 2);
    assert_eq!(rows[0].peer_account, "tobben", "newest first");
    assert_eq!(
        (rows[1].peer_account.as_str(), rows[1].unread),
        ("kaern", 1)
    );

    let evs = ask(
        &mut f,
        Request::DmRead {
            peer_account: "kaern".into(),
            up_to_dm: 3,
        },
    );
    assert!(done(&evs).is_some());
    assert!(matches!(&evs[1], Event::SocialState(s) if s.dm_unread == 0));

    let i = f.social.find("Grim").unwrap();
    f.social.players[i].privacy.whispers = 2;
    assert_eq!(
        refusal(&ask(
            &mut f,
            Request::Whisper {
                to: "Grim".into(),
                text: "hi".into()
            }
        )),
        Some((Asked::Whisper, lobby_code::WHISPER_BLOCKED))
    );
    assert_eq!(
        refusal(&ask(
            &mut f,
            Request::Whisper {
                to: "Nobody".into(),
                text: "hi".into()
            }
        )),
        Some((Asked::Whisper, lobby_code::UNKNOWN_PLAYER))
    );
    // A pushed whisper arrives with the unread count.
    let evs = {
        f.whisper_from("Rhaego", "join us");
        std::iter::from_fn(|| f.poll()).collect::<Vec<_>>()
    };
    assert!(matches!(&evs[..], [Event::DmLine(l), Event::SocialState(s)]
        if !l.outgoing && l.text == "join us" && s.dm_unread == 1));
}

/// Rule (preview, contract 3): Global's list has 38 players with the
/// session, sorted by name; a watched list follows a player leaving.
#[test]
fn user_lists_and_presence() {
    let mut f = scene();
    ask(&mut f, Request::SocialHello { rooms: ALL_ROOMS });
    let evs = ask(
        &mut f,
        Request::UserList {
            room: room::GLOBAL,
            watch: true,
        },
    );
    let Some(Event::Users { total, rows, .. }) = evs.first() else {
        panic!("{evs:?}");
    };
    assert_eq!(*total, 38);
    let names: Vec<String> = rows
        .iter()
        .map(|u| u.who.char.to_ascii_lowercase())
        .collect();
    assert!(names.windows(2).all(|w| w[0] <= w[1]));
    let evs = ask(
        &mut f,
        Request::UserList {
            room: room::GUILD,
            watch: false,
        },
    );
    assert!(
        matches!(&evs[..], [Event::Users { total: 2, .. }]),
        "Kaern and the session"
    );
    f.player_moves("Aldric", Where::Offline, None);
    let evs: Vec<Event> = std::iter::from_fn(|| f.poll()).collect();
    assert!(
        matches!(&evs[..], [Event::RoomPresence(p)] if p.count == 37 && p.who.char == "Aldric")
    );
}

/// Rule (contract 4): ranks decide what a member may do; the leader can't
/// leave with members; the last member leaving disbands; a tag is unique
/// in any case.
#[test]
fn guild_ranks_and_rules() {
    let mut f = scene();
    let evs = ask(&mut f, Request::GuildGet(GuildQuery::Mine));
    let Some(Event::Guild { info, members }) = evs.first() else {
        panic!("{evs:?}");
    };
    assert_eq!(
        (info.tag.as_str(), info.members, info.my_rank),
        ("NUG", 3, guild_rank::LEADER)
    );
    assert_eq!(members.len(), 3);
    assert_eq!(members[0].rank, guild_rank::LEADER);
    let manage = |op| Request::GuildManage(op);
    assert_eq!(
        refusal(&ask(&mut f, manage(GuildOp::Leave))),
        Some((Asked::GuildManage, lobby_code::GUILD_RANK))
    );
    assert_eq!(
        refusal(&ask(&mut f, manage(GuildOp::Disband("XX".into())))),
        Some((Asked::GuildManage, lobby_code::BAD_GUILD_NAME))
    );
    // As an officer: kick members only, no promotions.
    f.social.my_guild = Some((1, guild_rank::OFFICER));
    assert_eq!(
        refusal(&ask(&mut f, manage(GuildOp::Kick("Kaern".into())))),
        Some((Asked::GuildManage, lobby_code::GUILD_RANK))
    );
    assert_eq!(
        refusal(&ask(&mut f, manage(GuildOp::Promote("Ashka".into())))),
        Some((Asked::GuildManage, lobby_code::GUILD_RANK))
    );
    let evs = ask(&mut f, manage(GuildOp::Kick("Ashka".into())));
    assert!(done(&evs).is_some());
    assert!(evs
        .iter()
        .any(|e| matches!(e, Event::GuildNotice(n) if n.who == "Ashka")));
    // As a member: no invites, no motd.
    f.social.my_guild = Some((1, guild_rank::MEMBER));
    assert_eq!(
        refusal(&ask(
            &mut f,
            Request::GuildInvite {
                who: "Mirelle".into()
            }
        )),
        Some((Asked::GuildInvite, lobby_code::GUILD_RANK))
    );
    // Somebody in a guild already can't be invited, or create one.
    f.social.my_guild = Some((1, guild_rank::LEADER));
    assert_eq!(
        refusal(&ask(&mut f, Request::GuildInvite { who: "Grim".into() })),
        Some((Asked::GuildInvite, lobby_code::ALREADY_IN_GUILD))
    );
    let create = |tag: &str, name: &str| Request::GuildCreate {
        tag: tag.into(),
        name: name.into(),
    };
    assert_eq!(
        refusal(&ask(&mut f, create("AB", "Some Guild"))),
        Some((Asked::GuildCreate, lobby_code::ALREADY_IN_GUILD))
    );
    // Hand over, leave: Kaern leads, the session is out.
    assert!(done(&ask(&mut f, manage(GuildOp::MakeLeader("Kaern".into())))).is_some());
    assert_eq!(f.social.my_guild, Some((1, guild_rank::OFFICER)));
    assert!(done(&ask(&mut f, manage(GuildOp::Leave))).is_some());
    assert_eq!(f.social.my_guild, None);
    for (tag, name, code) in [
        ("pk", "Other Name", lobby_code::GUILD_TAG_TAKEN),
        ("A", "Fine Name", lobby_code::BAD_GUILD_NAME),
        ("AB", "Two  Spaces", lobby_code::BAD_GUILD_NAME),
    ] {
        assert_eq!(
            refusal(&ask(&mut f, create(tag, name))),
            Some((Asked::GuildCreate, code))
        );
    }
    let evs = ask(&mut f, create("Solo", "Alone Here"));
    let id = done(&evs).and_then(|d| d.id).expect("a guild id");
    // The last member leaving disbands; the tag is free again.
    assert!(done(&ask(&mut f, manage(GuildOp::Leave))).is_some());
    assert!(!f.social.guilds.iter().any(|g| g.id == id));
    assert!(done(&ask(&mut f, create("SOLO", "Again Here"))).is_some());
    // Invites: declining one that is gone says so.
    assert_eq!(
        refusal(&ask(
            &mut f,
            Request::GuildAnswer {
                guild: 99,
                accept: true
            }
        )),
        Some((Asked::GuildAnswer, lobby_code::INVITE_GONE))
    );
}

/// Rule (contract 5): mutual friends see where (and the game, with the
/// selected character's `joinable`), one-sided ones only online (hidden)
/// or offline; a hidden mutual friend shows hidden.
#[test]
fn friends_in_each_place() {
    let mut f = scene();
    let hc = GameInfo {
        id: 0,
        name: "baal run".into(),
        difficulty: 0,
        players: 3,
        max_players: 8,
        hardcore: true,
        expansion: true,
        ..GameInfo::default()
    };
    let id = f.add_game(hc, "");
    let i = f.social.find("Rhaego").unwrap();
    f.social.players[i].game = Some((id, "baal run".into()));
    let evs = ask(&mut f, Request::FriendsGet { ignored: false });
    let Some(Event::Friends {
        rows,
        ignored: false,
    }) = evs.first()
    else {
        panic!("{evs:?}");
    };
    let row = |n: &str| rows.iter().find(|r| r.who.char == n).unwrap();
    assert_eq!(
        (row("Kaern").place, row("Kaern").mutual),
        (Where::Lobby, true)
    );
    assert_eq!(row("Kaern").who.account, "kaern");
    let game = row("Rhaego").game.clone().expect("a mutual friend's game");
    assert_eq!(
        (game.game_id, game.joinable),
        (id, ErrorCode::WRONG_GAME_KIND.0)
    );
    assert_eq!(
        (row("Mirelle").place, row("Mirelle").mutual),
        (Where::Hidden, false)
    );
    assert_eq!(row("Vexa").place, Where::Hidden);
    assert_eq!(row("Tobben").place, Where::Offline);
    assert_eq!(row("Tobben").last_seen_ms, f.now_ms - 2 * 3_600_000);
    let evs = ask(&mut f, Request::FriendsGet { ignored: true });
    assert!(matches!(&evs[..], [Event::Friends { ignored: true, rows }] if rows.len() == 1));
    // Adding twice is refused; an unknown name too.
    assert_eq!(
        refusal(&ask(
            &mut f,
            Request::FriendEdit(FriendOp::Add("kaern".into()))
        )),
        Some((Asked::FriendEdit, ErrorCode::OTHER))
    );
    assert_eq!(
        refusal(&ask(
            &mut f,
            Request::FriendEdit(FriendOp::Add("Nobody".into()))
        )),
        Some((Asked::FriendEdit, lobby_code::UNKNOWN_PLAYER))
    );
    // A mutual friend entering a game is a notice with the place.
    f.player_moves("Kaern", Where::InGame, Some((id, "baal run".into())));
    let evs: Vec<Event> = std::iter::from_fn(|| f.poll()).collect();
    assert!(evs.iter().any(|e| matches!(e,
        Event::FriendNotice(n) if n.place == Where::InGame && n.game.as_ref().is_some_and(|g| g.game_id == id))));
}

/// Rule (owner decision 6, contract 7): every open game is listed, the
/// ones the character can't join with the reason in the contract's
/// order; a create's level range must hold the creator.
#[test]
fn lists_every_game_with_the_reason() {
    let mut f = scene();
    let game = |name: &str, hardcore, difficulty, players, min, max| GameInfo {
        name: name.into(),
        difficulty,
        players,
        max_players: 8,
        hardcore,
        expansion: true,
        level_min: min,
        level_max: max,
        ..GameInfo::default()
    };
    f.add_game(game("hc", true, 0, 1, 0, 0), "");
    f.add_game(game("hell", false, 2, 1, 0, 0), "");
    f.add_game(game("range", false, 0, 1, 40, 70), "");
    f.add_game(game("full", false, 0, 8, 0, 0), "");
    f.add_game(game("open", false, 1, 1, 30, 0), "");
    let evs = ask(&mut f, Request::ListGames);
    let why: Vec<(String, u16)> = evs
        .iter()
        .filter_map(|e| match e {
            Event::Game(g) => Some((g.name.clone(), g.joinable)),
            _ => None,
        })
        .collect();
    assert_eq!(
        why,
        [
            ("hc".to_string(), ErrorCode::WRONG_GAME_KIND.0),
            ("hell".into(), ErrorCode::DIFFICULTY_LOCKED.0),
            ("range".into(), lobby_code::LEVEL_RANGE.0),
            ("full".into(), ErrorCode::GAME_FULL.0),
            ("open".into(), 0),
        ]
    );
    let create = |description: &str, min, max| Request::CreateGame {
        name: "mine".into(),
        password: String::new(),
        difficulty: 0,
        place: Placement::Automatic,
        description: description.into(),
        level_min: min,
        level_max: max,
    };
    let failed = |evs: Vec<Event>| match &evs[..] {
        [Event::Error(f)] => Some(f.code),
        _ => None,
    };
    assert_eq!(
        failed(ask(&mut f, create("", 40, 50))),
        Some(lobby_code::LEVEL_RANGE)
    );
    assert_eq!(
        failed(ask(&mut f, create("", 30, 20))),
        Some(lobby_code::LEVEL_RANGE)
    );
    assert_eq!(
        failed(ask(&mut f, create(&"d".repeat(41), 0, 0))),
        Some(lobby_code::BAD_TEXT)
    );
    let evs = ask(&mut f, create("cows, chill", 30, 40));
    assert!(matches!(&evs[..], [Event::JoinOk(_)]), "{evs:?}");
    ask(&mut f, Request::ListGames);
    let mine = f.games.iter().find(|g| g.info.name == "mine").unwrap();
    assert_eq!(
        (
            mine.info.description.as_str(),
            mine.info.level_min,
            mine.info.level_max
        ),
        ("cows, chill", 30, 40)
    );
    assert_eq!(mine.info.created_ms, f.now_ms);
}

fn ladder(f: &mut FakeServer, split: u8, class: u8, at: LadderAt, rows: u8) -> LadderPage {
    let evs = ask(
        f,
        Request::LadderQuery(LadderQuery {
            season: 1,
            split,
            class,
            at,
            rows,
        }),
    );
    match evs.into_iter().next() {
        Some(Event::Ladder(p)) => p,
        e => panic!("{e:?}"),
    }
}

/// Rule (contract 6): 60 ranked characters in the main view, pages of
/// up to 50, ties by when the experience was reached; find and "around
/// me" give the page with the character; the own row is pinned off the
/// page; dead hardcore is marked; class and split filter.
#[test]
fn ladder_pages_find_pins_and_marks() {
    let mut f = scene();
    let evs = ask(&mut f, Request::LadderSeasons);
    assert!(matches!(&evs[..], [Event::LadderSeasons(s)]
        if s.iter().map(|s| s.season).collect::<Vec<_>>() == [1, 0] && s[0].current));

    let exp = ladder_split::EXPANSION;
    let p = ladder(&mut f, exp, 255, LadderAt::Rank(1), 50);
    assert_eq!(
        (p.total, p.first_rank, p.rows.len(), p.my_rank),
        (60, 1, 50, None)
    );
    assert_eq!(p.rows[0].experience, p.rows[1].experience, "a seeded tie");
    assert!(p
        .rows
        .windows(2)
        .all(|w| w[0].experience >= w[1].experience));
    let p2 = ladder(&mut f, exp, 255, LadderAt::Rank(51), 50);
    assert_eq!(
        (p2.first_rank, p2.rows.len(), p2.rows[0].rank),
        (51, 10, 51)
    );

    let target = p2.rows[3].char.to_ascii_uppercase();
    let found = ladder(&mut f, exp, 255, LadderAt::Find(target), 20);
    assert_eq!(found.first_rank, 41);
    let evs = ask(
        &mut f,
        Request::LadderQuery(LadderQuery {
            season: 1,
            split: exp,
            class: 255,
            at: LadderAt::Find("Nobody".into()),
            rows: 20,
        }),
    );
    assert_eq!(
        refusal(&evs),
        Some((Asked::LadderQuery, lobby_code::UNKNOWN_PLAYER))
    );
    let evs = ask(
        &mut f,
        Request::LadderQuery(LadderQuery {
            season: 7,
            split: exp,
            class: 255,
            at: LadderAt::Rank(1),
            rows: 20,
        }),
    );
    assert_eq!(
        refusal(&evs),
        Some((Asked::LadderQuery, lobby_code::NO_SUCH_SEASON))
    );

    // The session's character on the ladder (level 34): ranked below 20,
    // pinned under the first page, on its own page "around me".
    f.accounts[0].chars[0].season = 1;
    f.selected = Some(f.accounts[0].chars[0].clone());
    let p = ladder(&mut f, exp, 255, LadderAt::Rank(1), 20);
    let my = p.my_rank.expect("ranked");
    let pinned = p.pinned.clone().expect("pinned off the page");
    assert_eq!(pinned.rank, my);
    assert_eq!(
        pinned.flags & (ladder_flag::PINNED | ladder_flag::ME | ladder_flag::MY_ACCOUNT),
        ladder_flag::PINNED | ladder_flag::ME | ladder_flag::MY_ACCOUNT
    );
    let around = ladder(&mut f, exp, 255, LadderAt::Me, 20);
    assert!(around.pinned.is_none());
    assert!(around
        .rows
        .iter()
        .any(|r| r.rank == my && r.flags & ladder_flag::ME != 0));

    let amazons = ladder(&mut f, exp, 0, LadderAt::Rank(1), 50);
    assert!(amazons.rows.iter().all(|r| r.class == 0));
    let hc = ladder(
        &mut f,
        exp | ladder_split::HARDCORE,
        255,
        LadderAt::Rank(1),
        50,
    );
    assert_eq!(hc.total, 6);
    let dead: Vec<&str> = hc
        .rows
        .iter()
        .filter(|r| r.flags & ladder_flag::DEAD != 0)
        .map(|r| r.char.as_str())
        .collect();
    assert_eq!(dead, ["Sablethorn", "Brinehollow"]);
    let classic = ladder(&mut f, 0, 255, LadderAt::Rank(1), 50);
    assert_eq!(classic.total, 3);
}
