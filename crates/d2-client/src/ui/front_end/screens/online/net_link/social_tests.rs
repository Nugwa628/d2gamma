//! The realm lobby's messages through the link: requests are numbered
//! and written in the contract's layouts; answers, list rows and pushes
//! become the screens' events, lists put together by number while pushes
//! arrive in between. The S→C bodies are written here byte by byte from
//! the contract, not with `d2_net`'s encoders.

use d2_net::msg::{ErrorCode, ErrorMsg, Writer};

use super::super::link::{
    guild_notice, ladder_flag, lobby_code, room, Asked, DmLine, Done, Event, FriendOp, GuildOp,
    GuildQuery, LadderAt, LadderQuery, Privacy, Request, Where, Who,
};
use super::realm::asked_of;
use super::*;

/// The contract's kinds by the names these tests use.
mod kind {
    pub use d2_net::msg::guild::kind::{
        FRIENDS_GET, FRIEND_EDIT, FRIEND_NOTICE, FRIEND_ROW, GUILD_GET, GUILD_INFO, GUILD_MANAGE,
        GUILD_MEMBER, GUILD_NOTICE,
    };
    pub use d2_net::msg::ladder::kind::{
        END as LADDER_END, HEAD as LADDER_HEAD, QUERY as LADDER_QUERY, ROW as LADDER_ROW,
        SEASON_ROW as LADDER_SEASON_ROW,
    };
    pub use d2_net::msg::social::kind::{
        DM_CONVERSATIONS, DM_HISTORY, DM_LINE, DONE as SOCIAL_DONE, HELLO as SOCIAL_HELLO,
        LIST_BEGIN as SOCIAL_LIST_BEGIN, LIST_END as SOCIAL_LIST_END, ROOM_LINE, ROOM_PRESENCE,
        SAY, STATE as SOCIAL_STATE, USER_LIST, USER_ROW, WHISPER,
    };
}

fn link() -> NetLink {
    NetLink::new(OnlineSession::default(), ServerList::default())
}

fn who(w: &mut Writer, who: &Who) {
    w.str8(&who.char)
        .str8(&who.account)
        .opt(who.tag.as_ref(), |w, t| {
            w.str8(t);
        })
        .u8(who.class)
        .u8(who.level)
        .u8(who.flags);
}

fn kaern() -> Who {
    Who {
        char: "Kaern".into(),
        account: "kaern".into(),
        tag: Some("NUG".into()),
        class: 4,
        level: 82,
        flags: 0b100,
    }
}

fn list_begin(req: u32, what: u8, total: u32, rows: u16) -> Vec<u8> {
    Writer::new(kind::SOCIAL_LIST_BEGIN)
        .u32(req)
        .u8(what)
        .u32(total)
        .u16(0)
        .u16(rows)
        .u64(5)
        .finish()
}

fn list_end(req: u32) -> Vec<u8> {
    Writer::new(kind::SOCIAL_LIST_END).u32(req).finish()
}

fn dm_line(req: u32, dm: u64, text: &str) -> Vec<u8> {
    Writer::new(kind::DM_LINE)
        .u32(req)
        .u64(dm)
        .str8("kaern")
        .str8("Kaern")
        .bool(false)
        .bool(false)
        .str8(text)
        .u64(77)
        .finish()
}

fn guild_info(req: u32, tag: &str) -> Vec<u8> {
    Writer::new(kind::GUILD_INFO)
        .u32(req)
        .u64(1)
        .str8(tag)
        .str8("Nugwa")
        .str8("motd")
        .u64(3)
        .u16(3)
        .str8("Werwer")
        .u8(2)
        .finish()
}

fn ladder_row(req: u32, rank: u32, name: &str, flags: u8) -> Vec<u8> {
    Writer::new(kind::LADDER_ROW)
        .u32(req)
        .u32(rank)
        .str8(name)
        .u8(4)
        .u8(90)
        .u64(1_000_000)
        .opt(None::<&String>, |_, _| {})
        .u8(flags)
        .finish()
}

/// Feeds bodies and collects the events they make.
fn feed(l: &mut NetLink, bodies: &[Vec<u8>]) -> Vec<Event> {
    bodies.iter().filter_map(|b| l.event(b)).collect()
}

/// Rule (lobby contract 1, 3-6): each request goes out as its kind with
/// the session's next number and its fields in order.
#[test]
fn requests_are_numbered_in_the_contract_layout() {
    let mut l = link();
    let hello = l
        .realm
        .number(Request::SocialHello { rooms: 0b111 })
        .unwrap();
    assert_eq!(hello, [kind::SOCIAL_HELLO, 1, 0, 0, 0, 7, 0]);
    let say = l
        .realm
        .number(Request::Say {
            room: room::MOD,
            emote: true,
            text: "hi".into(),
        })
        .unwrap();
    assert_eq!(say, [kind::SAY, 2, 0, 0, 0, 1, 1, 2, b'h', b'i']);
    let privacy = l
        .realm
        .number(Request::FriendEdit(FriendOp::Privacy(Privacy {
            whispers: 1,
            show_where: 1,
        })))
        .unwrap();
    assert_eq!(privacy, [kind::FRIEND_EDIT, 3, 0, 0, 0, 5, 0, 1, 1]);
    let disband = l
        .realm
        .number(Request::GuildManage(GuildOp::Disband("NUG".into())))
        .unwrap();
    assert_eq!(
        disband,
        [kind::GUILD_MANAGE, 4, 0, 0, 0, 5, 3, b'N', b'U', b'G']
    );
    let find = l
        .realm
        .number(Request::LadderQuery(LadderQuery {
            season: 1,
            split: 2,
            class: 255,
            at: LadderAt::Find("Kaern".into()),
            rows: 50,
        }))
        .unwrap();
    let mut want = vec![
        kind::LADDER_QUERY,
        5,
        0,
        0,
        0,
        1,
        0,
        2,
        255,
        1,
        0,
        0,
        0,
        0,
        5,
    ];
    want.extend_from_slice(b"Kaern");
    want.push(50);
    assert_eq!(find, want);
    // Every realm-lobby request has its kind; others aren't numbered.
    for r in [
        Request::Whisper {
            to: "*kaern".into(),
            text: "x".into(),
        },
        Request::DmConversations { page: 0 },
        Request::DmHistory {
            peer: "Kaern".into(),
            before_dm: 0,
            rows: 20,
        },
        Request::DmRead {
            peer_account: "kaern".into(),
            up_to_dm: 9,
        },
        Request::UserList {
            room: 0,
            watch: true,
        },
        Request::GuildGet(GuildQuery::ByTag("NUG".into())),
        Request::GuildCreate {
            tag: "NUG".into(),
            name: "Nugwa".into(),
        },
        Request::GuildInvite {
            who: "Ashka".into(),
        },
        Request::GuildAnswer {
            guild: 2,
            accept: true,
        },
        Request::FriendsGet { ignored: true },
        Request::LadderSeasons,
    ] {
        let asked = r.asked().unwrap();
        let body = l.realm.number(r).unwrap();
        assert_eq!(asked_of(body[0]), Some(asked));
    }
    assert_eq!(l.realm.number(Request::ListGames), None);
    // The social and the ladder requests share one counter.
    assert_eq!(l.realm.number(Request::LadderSeasons).unwrap()[1], 17);
    // A new session numbers from 1 again.
    l.drop_session();
    assert_eq!(l.realm.number(Request::LadderSeasons).unwrap()[1], 1);
}

/// Rule (lobby-ui L7, contract 3.5): every S→C message of the contract
/// maps onto the screens' events; lists come whole at their end with
/// pushes passed on in between; refusals name the request.
#[test]
fn net_link_maps_social() {
    let mut l = link();
    // 1 hello, 2 DM history, 3 whisper, 4 users, 5 my guild, 6 by tag,
    // 7 friends, 8 seasons, 9 ladder, 10 ladder (refused), 11 user list
    // (realm without the area), 12 conversations.
    for r in [
        Request::SocialHello { rooms: 7 },
        Request::DmHistory {
            peer: "Kaern".into(),
            before_dm: 0,
            rows: 50,
        },
        Request::Whisper {
            to: "Tobben".into(),
            text: "hi".into(),
        },
        Request::UserList {
            room: room::GLOBAL,
            watch: true,
        },
        Request::GuildGet(GuildQuery::Mine),
        Request::GuildGet(GuildQuery::ByTag("NUG".into())),
        Request::FriendsGet { ignored: false },
        Request::LadderSeasons,
        Request::LadderQuery(LadderQuery {
            season: 1,
            split: 2,
            class: 255,
            at: LadderAt::Me,
            rows: 2,
        }),
        Request::LadderQuery(LadderQuery {
            season: 9,
            split: 2,
            class: 255,
            at: LadderAt::Rank(1),
            rows: 2,
        }),
        Request::UserList {
            room: room::MOD,
            watch: false,
        },
        Request::DmConversations { page: 0 },
    ] {
        l.realm.number(r).unwrap();
    }

    let state = Writer::new(kind::SOCIAL_STATE)
        .u32(1)
        .u16(7)
        .str8("d2gamma")
        .str8("D2Gamma")
        .opt(Some(&()), |w, _| {
            w.u64(1).str8("NUG").str8("Nugwa").u8(2);
        })
        .u16(3)
        .u16(1)
        .u16(4)
        .u16(0)
        .u8(0)
        .u8(1)
        .u8(5)
        .u16(10)
        .u8(2)
        .u16(30)
        .opt(None::<&u64>, |_, _| {})
        .u64(99)
        .finish();
    let evs = feed(&mut l, &[state]);
    let [Event::SocialState(st)] = &evs[..] else {
        panic!("{evs:?}");
    };
    assert_eq!(
        (st.rooms, st.mod_label.as_str(), st.dm_unread),
        (7, "D2Gamma", 3)
    );
    assert_eq!(
        st.guild.as_ref().map(|g| (g.tag.as_str(), g.my_rank)),
        Some(("NUG", 2))
    );
    assert_eq!((st.show_where, st.global_window_s, st.now_ms), (1, 30, 99));

    // A DM history with a live DM, a room line and a market body in
    // between: the pushes pass, the market body is not ours, the list
    // comes whole at its end.
    let mut line = Writer::new(kind::ROOM_LINE);
    line.u8(room::GLOBAL).u64(12).u8(0);
    who(&mut line, &kaern());
    line.str8("lf baal").u64(50);
    let evs = feed(
        &mut l,
        &[
            list_begin(2, kind::DM_HISTORY, 2, 2),
            dm_line(2, 1, "old"),
            dm_line(0, 9, "live"),
            line.finish(),
            vec![0xB0, 1, 2],
            dm_line(2, 2, "newer"),
            list_end(2),
        ],
    );
    assert_eq!(evs.len(), 3, "{evs:?}");
    assert!(matches!(&evs[0], Event::DmLine(DmLine { dm: 9, .. })));
    assert!(matches!(&evs[1], Event::RoomLine(r) if r.who == kaern() && r.text == "lf baal"));
    let Event::DmHistory { peer, lines } = &evs[2] else {
        panic!("{evs:?}");
    };
    assert_eq!(peer, "Kaern");
    assert_eq!(lines.iter().map(|d| d.dm).collect::<Vec<_>>(), [1, 2]);

    // Done: the whisper was stored for an offline player.
    let done = Writer::new(kind::SOCIAL_DONE)
        .u32(3)
        .u8(kind::WHISPER)
        .u16(0)
        .u32(0)
        .u8(1)
        .opt(Some(&44u64), |w, v| {
            w.u64(*v);
        })
        .str8("")
        .finish();
    let evs = feed(&mut l, &[done]);
    assert_eq!(
        evs,
        [Event::Done(Done {
            asked: Asked::Whisper,
            flags: 1,
            id: Some(44),
        })]
    );
    assert!(matches!(&evs[0], Event::Done(d) if d.stored()));

    // The user list, and a presence push for the watched room.
    let mut user = Writer::new(kind::USER_ROW);
    user.u32(4);
    who(&mut user, &kaern());
    user.u8(Where::InGame.wire());
    let mut presence = Writer::new(kind::ROOM_PRESENCE);
    presence.u8(room::GLOBAL).u8(1);
    who(&mut presence, &kaern());
    presence.u8(0).u16(37);
    let evs = feed(
        &mut l,
        &[
            list_begin(4, kind::USER_LIST, 38, 1),
            user.finish(),
            presence.finish(),
            list_end(4),
        ],
    );
    assert!(
        matches!(&evs[0], Event::RoomPresence(p) if p.count == 37 && p.place == Where::Offline)
    );
    assert!(matches!(&evs[1], Event::Users { room: 0, total: 38, rows }
        if rows.len() == 1 && rows[0].place == Where::InGame));

    // My guild: the info, then its members as a list; by tag: alone.
    let mut member = Writer::new(kind::GUILD_MEMBER);
    member.u32(5);
    who(&mut member, &kaern());
    member.u8(1).u8(1).u64(10).u64(20);
    let notice = Writer::new(kind::GUILD_NOTICE)
        .u8(guild_notice::MOTD)
        .u64(1)
        .str8("NUG")
        .str8("Nugwa")
        .str8("Werwer")
        .str8("Werwer")
        .u64(30)
        .finish();
    let evs = feed(
        &mut l,
        &[
            guild_info(5, "NUG"),
            list_begin(5, kind::GUILD_GET, 1, 1),
            member.finish(),
            notice,
            list_end(5),
            guild_info(6, "NUG"),
        ],
    );
    assert!(matches!(&evs[0], Event::GuildNotice(n) if n.what == guild_notice::MOTD));
    assert!(matches!(&evs[1], Event::Guild { info, members }
        if info.tag == "NUG" && info.my_rank == 2 && members.len() == 1 && members[0].rank == 1));
    assert!(matches!(&evs[2], Event::GuildFound(g) if g.members == 3));

    // Friends, with a mutual friend's game; a friend notice push.
    let mut friend = Writer::new(kind::FRIEND_ROW);
    friend.u32(7);
    who(&mut friend, &kaern());
    friend
        .u8(Where::InGame.wire())
        .bool(true)
        .opt(Some(&()), |w, _| {
            w.u32(5).str8("baal run").u16(ErrorCode::GAME_FULL.0);
        })
        .u64(1)
        .u64(2);
    let mut fnotice = Writer::new(kind::FRIEND_NOTICE);
    fnotice.u8(1);
    who(&mut fnotice, &kaern());
    fnotice.u8(1).opt(None::<&()>, |_, _| {}).u64(3);
    let evs = feed(
        &mut l,
        &[
            list_begin(7, kind::FRIENDS_GET, 1, 1),
            fnotice.finish(),
            friend.finish(),
            list_end(7),
        ],
    );
    assert!(matches!(&evs[0], Event::FriendNotice(n) if n.place == Where::Lobby));
    assert!(matches!(&evs[1], Event::Friends { ignored: false, rows }
        if rows[0].mutual && rows[0].game.as_ref().is_some_and(|g| g.game_id == 5 && g.joinable == ErrorCode::GAME_FULL.0)));

    // Seasons, then a ladder page with the pinned own row, then a
    // refused page.
    let season = |s: u16, name: &str| {
        Writer::new(kind::LADDER_SEASON_ROW)
            .u32(8)
            .u16(s)
            .str8(name)
            .u64(1)
            .u64(0)
            .bool(false)
            .bool(s == 1)
            .finish()
    };
    let head = Writer::new(kind::LADDER_HEAD)
        .u32(9)
        .u16(1)
        .u8(2)
        .u8(255)
        .u32(60)
        .u32(1)
        .u8(2)
        .opt(Some(&47u32), |w, v| {
            w.u32(*v);
        })
        .u64(10)
        .u64(11)
        .finish();
    let end = |req: u32, code: u16| {
        Writer::new(kind::LADDER_END)
            .u32(req)
            .u16(code)
            .u32(0)
            .finish()
    };
    let evs = feed(
        &mut l,
        &[
            season(1, "Season 1"),
            season(0, "Non-ladder"),
            end(8, 0),
            head,
            ladder_row(9, 1, "Ashana", ladder_flag::ONLINE),
            ladder_row(9, 2, "Belana", 0),
            ladder_row(9, 47, "Werwer", ladder_flag::PINNED | ladder_flag::ME),
            end(9, 0),
            end(10, lobby_code::NO_SUCH_SEASON.0),
        ],
    );
    assert!(matches!(&evs[0], Event::LadderSeasons(s) if s.len() == 2 && s[0].current));
    let Event::Ladder(page) = &evs[1] else {
        panic!("{evs:?}");
    };
    assert_eq!(
        (page.total, page.my_rank, page.rows.len()),
        (60, Some(47), 2)
    );
    assert_eq!(page.pinned.as_ref().map(|r| r.rank), Some(47));
    assert!(
        matches!(&evs[2], Event::Refused { asked: Asked::LadderQuery, failure }
        if failure.code == lobby_code::NO_SUCH_SEASON)
    );

    // A realm without the area answers ERROR: a refusal of that request
    // (not `Error`, which the old chat reads as "chat off").
    let e = ErrorMsg::new(kind::USER_LIST, ErrorCode::NOT_SUPPORTED);
    let evs = feed(&mut l, &[e.body()]);
    assert!(
        matches!(&evs[..], [Event::Refused { asked: Asked::UserList, failure }]
        if failure.code == ErrorCode::NOT_SUPPORTED)
    );
    assert_eq!(l.realm.social.in_flight(), 1, "only the conversations wait");

    // A refused list is a `SocialDone` with a code.
    let refused = Writer::new(kind::SOCIAL_DONE)
        .u32(12)
        .u8(kind::DM_CONVERSATIONS)
        .u16(ErrorCode::RATE_LIMITED.0)
        .u32(20)
        .u8(0)
        .opt(None::<&u64>, |_, _| {})
        .str8("slow down")
        .finish();
    let evs = feed(&mut l, &[refused]);
    assert!(
        matches!(&evs[..], [Event::Refused { asked: Asked::DmConversations, failure }]
        if failure.wait_s == 20 && failure.text == "slow down")
    );
    assert_eq!(
        (l.realm.social.in_flight(), l.realm.ladder.in_flight()),
        (0, 0)
    );
    assert!(l.realm.waiting.is_empty(), "{:?}", l.realm.waiting.keys());

    // Rows nobody asked for and broken bodies show nothing.
    assert!(feed(&mut l, &[dm_line(77, 1, "stray"), list_end(77)]).is_empty());
    let mut trailing = list_end(1);
    trailing.push(0);
    assert!(feed(&mut l, &[trailing, vec![kind::ROOM_LINE, 0]]).is_empty());
}
