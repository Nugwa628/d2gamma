//! Putting the social service's answers together from interleaved bodies.

use super::*;
use d2_net::msg::guild::{guild_get, rank, FriendGame};
use d2_net::msg::market;
use d2_net::msg::social::{kind, place, room, Ask};
use d2_net::msg::ErrorCode;

fn who(name: &str) -> Who {
    Who {
        char: name.into(),
        class: 1,
        level: 30,
        ..Who::default()
    }
}

fn dm(req: u32, dm: u64, text: &str) -> social::FromRealm {
    social::FromRealm::DmLine(DmLine {
        req,
        dm,
        peer_account: "nugwa".into(),
        peer_char: "Kaern".into(),
        outgoing: false,
        stored: false,
        text: text.into(),
        at_ms: dm,
    })
}

fn head(req: u32, what: u8, rows: u16) -> social::FromRealm {
    social::FromRealm::ListBegin(ListHead {
        req,
        what,
        total: rows.into(),
        page: 0,
        rows,
        now_ms: 1,
    })
}

fn end(req: u32) -> social::FromRealm {
    social::FromRealm::ListEnd { req }
}

fn market_row() -> Vec<u8> {
    market::FromRealm::ListEnd { req: 3 }.body()
}

fn feed(c: &mut SocialClient, m: social::FromRealm) {
    assert!(c.feed(&m.body()).unwrap());
}

/// A `DmHistory` list with live DMs pushed between its rows and a market
/// body interleaved: the list comes out whole under its `req` with only
/// its own lines, the pushes come out as pushes, and the market body goes
/// back to the caller.
#[test]
fn social_client_keeps_lists_apart() {
    let mut c = SocialClient::new();
    let (req, body) = c.prepare(Ask::DmHistory {
        peer: "Kaern".into(),
        before_dm: 0,
        rows: 3,
    });
    assert_eq!(req, 1);
    assert_eq!(body[0], kind::DM_HISTORY);
    let (users, _) = c.prepare(Ask::UserList {
        room: room::GLOBAL,
        watch: true,
    });
    assert_eq!(c.in_flight(), 2);
    feed(&mut c, head(req, kind::DM_HISTORY, 3));
    feed(&mut c, dm(req, 10, "old"));
    feed(&mut c, dm(0, 13, "live one"));
    assert!(!c.feed(&market_row()).unwrap());
    feed(&mut c, head(users, kind::USER_LIST, 1));
    feed(&mut c, dm(req, 11, "older"));
    feed(
        &mut c,
        social::FromRealm::UserRow {
            req: users,
            who: who("Ann"),
            place: place::LOBBY,
        },
    );
    feed(
        &mut c,
        social::FromRealm::RoomLine(RoomLine {
            room: room::GLOBAL,
            who: who("Ann"),
            text: "hi".into(),
            ..RoomLine::default()
        }),
    );
    feed(&mut c, dm(req, 12, "newest"));
    feed(&mut c, dm(0, 14, "live two"));
    feed(&mut c, end(users));
    feed(&mut c, end(req));
    let got = c.take();
    assert_eq!(got.len(), 5, "{got:#?}");
    assert!(matches!(&got[0], SocialAnswer::Dm(l) if l.text == "live one"));
    assert!(matches!(&got[1], SocialAnswer::RoomLine(l) if l.text == "hi"));
    assert!(matches!(&got[2], SocialAnswer::Dm(l) if l.text == "live two"));
    match &got[3] {
        SocialAnswer::Users { req: r, rows, .. } => {
            assert_eq!(*r, users);
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0].who.char, "Ann");
        }
        other => panic!("{other:?}"),
    }
    match &got[4] {
        SocialAnswer::DmHistory {
            req: r,
            head,
            lines,
        } => {
            assert_eq!(*r, req);
            assert_eq!(head.rows, 3);
            let texts: Vec<&str> = lines.iter().map(|l| l.text.as_str()).collect();
            assert_eq!(texts, ["old", "older", "newest"]);
            assert!(lines.iter().all(|l| l.req == req));
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(got[0].req(), None);
    assert_eq!(c.in_flight(), 0);
}

#[test]
fn hello_state_and_done_answer_their_requests() {
    let mut c = SocialClient::new();
    let (hello, _) = c.prepare(Ask::Hello {
        rooms: room::bit(room::GLOBAL),
    });
    let (say, _) = c.prepare(Ask::Say {
        room: room::GUILD,
        emote: false,
        text: "hi".into(),
    });
    feed(
        &mut c,
        social::FromRealm::State(SocialState {
            req: hello,
            dm_unread: 2,
            ..SocialState::default()
        }),
    );
    feed(
        &mut c,
        social::FromRealm::Done(Done::refused(say, kind::SAY, ErrorCode::ROOM_UNAVAILABLE)),
    );
    // A pushed state, and an answer to a request never sent (dropped).
    feed(&mut c, social::FromRealm::State(SocialState::default()));
    feed(&mut c, social::FromRealm::Done(Done::ok(99, kind::SAY)));
    let got = c.take();
    assert_eq!(got.len(), 3, "{got:#?}");
    assert!(matches!(&got[0], SocialAnswer::State(s) if s.req == hello && s.dm_unread == 2));
    assert!(matches!(&got[1], SocialAnswer::Done(d) if d.req == say && !d.is_ok()));
    assert_eq!(got[2].req(), Some(0));
    assert_eq!(c.in_flight(), 0);
}

/// "My guild" is its info, then a list of members; invites are a list of
/// infos; a guild by tag is one info. Friends are a list of rows; notices
/// are pushes.
#[test]
fn guild_and_friends_answers_assemble() {
    let mut c = SocialClient::new();
    let (mine, _) = c.prepare(guild::Ask::GuildGet {
        what: guild_get::MINE,
        tag: String::new(),
    });
    let (invites, _) = c.prepare(guild::Ask::GuildGet {
        what: guild_get::INVITES,
        tag: String::new(),
    });
    let (by_tag, _) = c.prepare(guild::Ask::GuildGet {
        what: guild_get::BY_TAG,
        tag: "NUG".into(),
    });
    let (friends, _) = c.prepare(guild::Ask::FriendsGet { what: 0 });
    let info = |guild: u64| GuildInfo {
        guild,
        tag: "NUG".into(),
        name: "Nugwa".into(),
        my_rank: rank::NONE,
        ..GuildInfo::default()
    };
    let g = |m: guild::FromRealm| m.body();
    let bodies = [
        g(guild::FromRealm::GuildInfo {
            req: mine,
            info: GuildInfo {
                my_rank: rank::LEADER,
                ..info(7)
            },
        }),
        head(invites, guild::kind::GUILD_GET, 2).body(),
        head(mine, guild::kind::GUILD_GET, 2).body(),
        g(guild::FromRealm::GuildInfo {
            req: invites,
            info: info(8),
        }),
        g(guild::FromRealm::GuildMember {
            req: mine,
            member: GuildMember {
                who: who("Rhaego"),
                rank: rank::LEADER,
                ..GuildMember::default()
            },
        }),
        g(guild::FromRealm::GuildNotice(GuildNotice {
            what: guild::guild_notice::MOTD,
            ..GuildNotice::default()
        })),
        g(guild::FromRealm::GuildInfo {
            req: by_tag,
            info: info(7),
        }),
        g(guild::FromRealm::GuildInfo {
            req: invites,
            info: info(9),
        }),
        head(friends, guild::kind::FRIENDS_GET, 1).body(),
        g(guild::FromRealm::GuildMember {
            req: mine,
            member: GuildMember {
                who: who("Kaern"),
                ..GuildMember::default()
            },
        }),
        g(guild::FromRealm::FriendRow {
            req: friends,
            row: FriendRow {
                who: who("Kaern"),
                place: place::IN_GAME,
                mutual: true,
                game: Some(FriendGame {
                    game_id: 4,
                    game_name: "cows".into(),
                    joinable: 0,
                }),
                ..FriendRow::default()
            },
        }),
        g(guild::FromRealm::FriendNotice(FriendNotice {
            what: guild::friend_notice::ONLINE,
            who: who("Ann"),
            ..FriendNotice::default()
        })),
        end(mine).body(),
        end(friends).body(),
        end(invites).body(),
    ];
    for b in &bodies {
        assert!(c.feed(b).unwrap(), "{:#x}", b[0]);
    }
    let got = c.take();
    assert_eq!(got.len(), 6, "{got:#?}");
    assert!(matches!(&got[0], SocialAnswer::GuildNotice(_)));
    assert!(
        matches!(&got[1], SocialAnswer::GuildByTag { req, info } if *req == by_tag && info.guild == 7)
    );
    assert!(matches!(&got[2], SocialAnswer::FriendNotice(n) if n.who.char == "Ann"));
    match &got[3] {
        SocialAnswer::Guild { info, members, .. } => {
            assert_eq!(info.my_rank, rank::LEADER);
            let names: Vec<&str> = members.iter().map(|m| m.who.char.as_str()).collect();
            assert_eq!(names, ["Rhaego", "Kaern"]);
        }
        other => panic!("{other:?}"),
    }
    assert!(
        matches!(&got[4], SocialAnswer::Friends { rows, .. } if rows.len() == 1 && rows[0].mutual)
    );
    match &got[5] {
        SocialAnswer::GuildInvites {
            invites: list, req, ..
        } => {
            assert_eq!(*req, invites);
            assert_eq!(list.iter().map(|i| i.guild).collect::<Vec<_>>(), [8, 9]);
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(c.in_flight(), 0);
}

/// `ERROR` for a social kind (a realm without the area) answers the oldest
/// request of that kind; an `ERROR` of another area isn't the client's.
#[test]
fn errors_answer_the_oldest_request_of_their_kind() {
    let mut c = SocialClient::new();
    let (first, _) = c.prepare(guild::Ask::FriendsGet { what: 0 });
    let (_second, _) = c.prepare(guild::Ask::FriendsGet { what: 1 });
    let e = ErrorMsg::new(guild::kind::FRIENDS_GET, ErrorCode::NOT_SUPPORTED);
    assert!(SocialClient::owns(&e.body()));
    assert!(c.feed(&e.body()).unwrap());
    let other = ErrorMsg::new(0xB2, ErrorCode::MARKET_OFF);
    assert!(!SocialClient::owns(&other.body()));
    assert!(!c.feed(&other.body()).unwrap());
    let got = c.take();
    assert!(
        matches!(&got[..], [SocialAnswer::Error { req: Some(r), error }]
        if *r == first && error.code == ErrorCode::NOT_SUPPORTED)
    );
    assert_eq!(c.in_flight(), 1);
}

/// A row with no head, more rows than announced, an end with rows
/// missing, or a row of the wrong list is the realm breaking the
/// protocol. A body of another area is never the client's.
#[test]
fn broken_lists_are_malformed() {
    let bad = |steps: &[social::FromRealm]| {
        let mut c = SocialClient::new();
        let (req, _) = c.prepare(Ask::DmHistory {
            peer: "Kaern".into(),
            before_dm: 0,
            rows: 1,
        });
        assert_eq!(req, 1);
        let mut out = Ok(true);
        for m in steps {
            out = c.feed(&m.body());
            if out.is_err() {
                break;
            }
        }
        out
    };
    let malformed = Err(OnlineError::Malformed);
    assert_eq!(bad(&[dm(1, 1, "no head")]), malformed);
    assert_eq!(
        bad(&[head(1, kind::DM_HISTORY, 1), dm(1, 1, "a"), dm(1, 2, "b")]),
        malformed
    );
    assert_eq!(
        bad(&[head(1, kind::DM_HISTORY, 2), dm(1, 1, "a"), end(1)]),
        malformed
    );
    assert_eq!(
        bad(&[
            head(1, kind::DM_HISTORY, 1),
            social::FromRealm::UserRow {
                req: 1,
                who: who("x"),
                place: 0
            }
        ]),
        malformed
    );
    assert_eq!(
        bad(&[head(1, kind::DM_HISTORY, 0), head(1, kind::DM_HISTORY, 0)]),
        malformed
    );
    // A body that doesn't read.
    let mut c = SocialClient::new();
    assert_eq!(c.feed(&[kind::LIST_END, 1]), malformed);
    // Lobby and ladder bodies are not the client's.
    assert!(!c.feed(&[0x42]).unwrap());
    assert!(!c.feed(&[0x54]).unwrap());
    assert!(!c.feed(&[]).unwrap());
}

#[test]
fn request_numbers_rise_and_are_shared() {
    let mut c = SocialClient::new();
    let (a, _) = c.prepare(Ask::DmConversations { page: 0 });
    let mut ladder = crate::LadderClient::sharing(c.counter());
    let (b, _) = ladder.prepare(d2_net::msg::ladder::Ask::Seasons);
    let (d, body) = c.prepare(Ask::DmRead {
        peer_account: "nugwa".into(),
        up_to_dm: 3,
    });
    assert_eq!((a, b, d), (1, 2, 3));
    assert_eq!(social::ToRealm::parse(&body).unwrap().req, 3);
    c.reset();
    assert_eq!(c.in_flight(), 0);
    assert_eq!(c.prepare(Ask::Hello { rooms: 1 }).0, 4);
}
