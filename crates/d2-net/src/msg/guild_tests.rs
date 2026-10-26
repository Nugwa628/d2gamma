//! The guild and friends messages' round trips and their decode rules.

use super::*;
use crate::msg::social::{place, who_flags};

fn who() -> Who {
    Who {
        char: "Kaern".into(),
        account: "nugwa".into(),
        tag: Some("NUG".into()),
        class: 4,
        level: 80,
        flags: who_flags::EXPANSION,
    }
}

fn game() -> FriendGame {
    FriendGame {
        game_id: 9,
        game_name: "baal run".into(),
        joinable: 0,
    }
}

#[test]
fn guild_messages_round_trip() {
    for ask in [
        Ask::GuildGet {
            what: guild_get::MINE,
            tag: String::new(),
        },
        Ask::GuildGet {
            what: guild_get::BY_TAG,
            tag: "NUG".into(),
        },
        Ask::GuildCreate {
            tag: "NUG".into(),
            name: "Nugwa".into(),
        },
        Ask::GuildInvite { who: "*ann".into() },
        Ask::GuildAnswer {
            guild: 7,
            accept: true,
        },
        Ask::GuildManage {
            op: guild_op::MOTD,
            arg: "m".repeat(MAX_MOTD),
        },
        Ask::GuildManage {
            op: guild_op::LEAVE,
            arg: String::new(),
        },
        Ask::FriendsGet {
            what: friends_get::IGNORED,
        },
        Ask::FriendEdit(FriendEdit::Add("Kaern".into())),
        Ask::FriendEdit(FriendEdit::Remove("Kaern".into())),
        Ask::FriendEdit(FriendEdit::Ignore("*spam".into())),
        Ask::FriendEdit(FriendEdit::Unignore("*spam".into())),
        Ask::FriendEdit(FriendEdit::Privacy {
            whispers: 2,
            show_where: 1,
        }),
    ] {
        let m = ToRealm { req: 5, ask };
        let body = m.body();
        assert_eq!(body[0], m.kind());
        let k = &body[0];
        assert!(super::super::kind::GUILD.contains(k) || super::super::kind::FRIENDS.contains(k));
        assert_eq!(ToRealm::parse(&body), Ok(m));
    }
    for m in [
        FromRealm::GuildInfo {
            req: 3,
            info: GuildInfo {
                guild: 7,
                tag: "NUG".into(),
                name: "Nugwa".into(),
                motd: "cows at 8".into(),
                created_ms: 1,
                members: 3,
                leader_char: "Rhaego".into(),
                my_rank: rank::NONE,
            },
        },
        FromRealm::GuildMember {
            req: 3,
            member: GuildMember {
                who: who(),
                place: place::IN_GAME,
                rank: rank::OFFICER,
                joined_ms: 2,
                last_seen_ms: 3,
            },
        },
        FromRealm::GuildNotice(GuildNotice {
            what: guild_notice::INVITED_YOU,
            guild: 7,
            tag: "NUG".into(),
            name: "Nugwa".into(),
            who: "Kaern".into(),
            by: "Rhaego".into(),
            at_ms: 4,
        }),
        FromRealm::FriendRow {
            req: 4,
            row: FriendRow {
                who: who(),
                place: place::IN_GAME,
                mutual: true,
                game: Some(game()),
                last_seen_ms: 5,
                added_ms: 6,
            },
        },
        FromRealm::FriendRow {
            req: 4,
            row: FriendRow {
                who: who(),
                place: place::OFFLINE,
                mutual: false,
                game: None,
                last_seen_ms: 5,
                added_ms: 6,
            },
        },
        FromRealm::FriendNotice(FriendNotice {
            what: friend_notice::ADDED_YOU,
            who: who(),
            place: place::LOBBY,
            game: None,
            at_ms: 7,
        }),
    ] {
        let body = m.body();
        assert_eq!(body[0], m.kind());
        assert_eq!(FromRealm::parse(&body), Ok(m));
    }
    assert_eq!(
        FromRealm::GuildNotice(GuildNotice {
            what: 1,
            ..GuildNotice::default()
        })
        .req(),
        None
    );
}

/// Privacy is op 5 with an empty name, then the two choices.
#[test]
fn privacy_layout() {
    let body = ToRealm {
        req: 1,
        ask: Ask::FriendEdit(FriendEdit::Privacy {
            whispers: 1,
            show_where: 0,
        }),
    }
    .body();
    assert_eq!(body, [0xFA, 1, 0, 0, 0, 5, 0, 1, 0]);
}

#[test]
fn guild_malformed_refused() {
    let parse = |ask: Ask| ToRealm::parse(&ToRealm { req: 1, ask }.body());
    // Ops, lists and privacy values past their last; op 4 of friends is
    // not used; privacy with a name.
    assert_eq!(
        parse(Ask::GuildManage {
            op: 7,
            arg: String::new()
        }),
        bad()
    );
    assert_eq!(
        parse(Ask::GuildGet {
            what: 3,
            tag: String::new()
        }),
        bad()
    );
    assert_eq!(parse(Ask::FriendsGet { what: 2 }), bad());
    assert_eq!(
        parse(Ask::FriendEdit(FriendEdit::Privacy {
            whispers: 3,
            show_where: 0
        })),
        bad()
    );
    assert_eq!(ToRealm::parse(&[0xFA, 1, 0, 0, 0, 4, 0]), bad());
    assert_eq!(ToRealm::parse(&[0xFA, 1, 0, 0, 0, 5, 1, b'x', 0, 0]), bad());
    // Over-long names, tags and motd.
    assert_eq!(
        parse(Ask::GuildCreate {
            tag: "T".repeat(MAX_TAG_WIRE + 1),
            name: "ok name".into()
        }),
        bad()
    );
    assert_eq!(
        parse(Ask::GuildCreate {
            tag: "NUG".into(),
            name: "n".repeat(MAX_GUILD_NAME_WIRE + 1)
        }),
        bad()
    );
    assert_eq!(
        parse(Ask::GuildManage {
            op: guild_op::MOTD,
            arg: "m".repeat(MAX_MOTD + 1)
        }),
        bad()
    );
    assert_eq!(
        parse(Ask::GuildInvite {
            who: "w".repeat(MAX_TARGET + 1)
        }),
        bad()
    );
    // A tag or name the rules refuse but the wire carries decodes: the
    // realm answers `BadGuildName`.
    assert!(parse(Ask::GuildCreate {
        tag: "TOOLONG".into(),
        name: "x".into()
    })
    .is_ok());
    // A rank past the leader, a notice kind outside 1-9, a friend notice
    // outside 1-5, a place past hidden.
    let member = |rank| FromRealm::GuildMember {
        req: 1,
        member: GuildMember {
            who: who(),
            rank,
            ..GuildMember::default()
        },
    };
    assert_eq!(FromRealm::parse(&member(3).body()), bad());
    let info = FromRealm::GuildInfo {
        req: 1,
        info: GuildInfo {
            my_rank: 3,
            ..GuildInfo::default()
        },
    };
    assert_eq!(FromRealm::parse(&info.body()), bad());
    for what in [0, 10] {
        let n = FromRealm::GuildNotice(GuildNotice {
            what,
            ..GuildNotice::default()
        });
        assert_eq!(FromRealm::parse(&n.body()), bad());
    }
    for what in [0, 6] {
        let n = FromRealm::FriendNotice(FriendNotice {
            what,
            who: who(),
            ..FriendNotice::default()
        });
        assert_eq!(FromRealm::parse(&n.body()), bad());
    }
    let row = FromRealm::FriendRow {
        req: 1,
        row: FriendRow {
            who: who(),
            place: 4,
            ..FriendRow::default()
        },
    };
    assert_eq!(FromRealm::parse(&row.body()), bad());
    // A game name past the lobby's limit.
    let row = FromRealm::FriendRow {
        req: 1,
        row: FriendRow {
            who: who(),
            game: Some(FriendGame {
                game_name: "g".repeat(MAX_GAME_NAME + 1),
                ..game()
            }),
            ..FriendRow::default()
        },
    };
    assert_eq!(FromRealm::parse(&row.body()), bad());
    // Trailing bytes; the social done is not this module's.
    let mut body = ToRealm {
        req: 1,
        ask: Ask::FriendsGet { what: 0 },
    }
    .body();
    body.push(0);
    assert_eq!(ToRealm::parse(&body), bad());
    assert_eq!(FromRealm::parse(&[0x7D]), bad());
}

fn bad<T>() -> Result<T, NetError> {
    Err(NetError::Malformed)
}
