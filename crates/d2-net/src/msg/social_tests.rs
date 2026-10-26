//! The social messages' round trips, their decode rules, and that the
//! largest legal row of every social, guild, friends and ladder message
//! fits one frame.

use super::*;
use crate::msg::{guild, ladder};
use crate::wire::MAX_FRAME;

/// A player at every field's longest.
pub(crate) fn longest_who() -> Who {
    Who {
        char: "c".repeat(MAX_FROM),
        account: "a".repeat(MAX_ACCOUNT),
        tag: Some("T".repeat(MAX_TAG)),
        class: 6,
        level: 99,
        flags: who_flags::HARDCORE | who_flags::DEAD | who_flags::AWAY,
    }
}

fn who() -> Who {
    Who {
        char: "Kaern".into(),
        account: String::new(),
        tag: Some("NUG".into()),
        class: 1,
        level: 42,
        flags: who_flags::EXPANSION | who_flags::IN_GAME,
    }
}

fn state() -> SocialState {
    SocialState {
        req: 1,
        rooms: room::bit(room::GLOBAL) | room::bit(room::MOD) | room::bit(room::GUILD),
        mod_id: "d2gamma".into(),
        mod_label: "D2Gamma".into(),
        guild: Some(GuildBrief {
            guild: 7,
            tag: "NUG".into(),
            name: "Nugwa".into(),
            my_rank: guild::rank::OFFICER,
        }),
        dm_unread: 3,
        dm_conversations_unread: 2,
        friends_online: 5,
        guild_invites: 1,
        whispers: whispers::FRIENDS_AND_GUILD,
        show_where: show_where::NOBODY,
        limits: ChatLimits {
            say_lines: 5,
            say_window_s: 10,
            global_lines: 2,
            global_window_s: 30,
        },
        muted_until_ms: Some(1_800_000_000_000),
        now_ms: 1_799_999_000_000,
    }
}

fn dm_line(req: u32, dm: u64) -> DmLine {
    DmLine {
        req,
        dm,
        peer_account: "nugwa".into(),
        peer_char: "Kaern".into(),
        outgoing: true,
        stored: true,
        text: "see you in cows".into(),
        at_ms: 5,
    }
}

#[test]
fn social_messages_round_trip() {
    for ask in [
        Ask::Hello {
            rooms: room::bit(room::GLOBAL) | room::bit(room::MOD),
        },
        Ask::Say {
            room: room::GLOBAL,
            emote: true,
            text: "waves".into(),
        },
        Ask::Whisper {
            to: "*nugwa".into(),
            text: "hi".into(),
        },
        Ask::DmConversations { page: 2 },
        Ask::DmHistory {
            peer: "Kaern".into(),
            before_dm: 99,
            rows: MAX_HISTORY_ROWS,
        },
        Ask::DmRead {
            peer_account: "nugwa".into(),
            up_to_dm: 98,
        },
        Ask::UserList {
            room: room::MOD,
            watch: true,
        },
    ] {
        let m = ToRealm { req: 77, ask };
        let body = m.body();
        assert_eq!(body[0], m.kind());
        assert!(super::super::kind::SOCIAL.contains(&body[0]));
        assert_eq!(ToRealm::parse(&body), Ok(m));
    }
    for m in [
        FromRealm::State(state()),
        FromRealm::State(SocialState {
            guild: None,
            muted_until_ms: None,
            ..state()
        }),
        FromRealm::RoomLine(RoomLine {
            room: room::GUILD,
            line: 12,
            what: line_what::GUILD_EVENT,
            who: who(),
            text: "Kaern joined the guild".into(),
            at_ms: 3,
        }),
        FromRealm::DmLine(dm_line(0, 4)),
        FromRealm::DmConversationRow {
            req: 8,
            row: DmConversationRow {
                peer_account: "nugwa".into(),
                peer_char: "Kaern".into(),
                place: place::IN_GAME,
                unread: 4,
                last_ms: 9,
                last_text: "last words".into(),
            },
        },
        FromRealm::UserRow {
            req: 9,
            who: who(),
            place: place::LOBBY,
        },
        FromRealm::RoomPresence(RoomPresence {
            room: room::GLOBAL,
            op: presence_op::CHANGED,
            who: who(),
            place: place::HIDDEN,
            count: 38,
        }),
        FromRealm::Done(Done {
            flags: done_flags::STORED,
            id: Some(41),
            ..Done::ok(10, kind::WHISPER)
        }),
        FromRealm::Done(Done::refused(11, kind::SAY, ErrorCode::ROOM_UNAVAILABLE)),
        FromRealm::ListBegin(ListHead {
            req: 12,
            what: kind::USER_LIST,
            total: 38,
            page: 0,
            rows: 38,
            now_ms: 1,
        }),
        FromRealm::ListEnd { req: 12 },
    ] {
        let body = m.body();
        assert_eq!(body[0], m.kind());
        assert!(super::super::kind::SOCIAL.contains(&body[0]));
        assert_eq!(FromRealm::parse(&body), Ok(m));
    }
}

/// The exact layout of `Say` and `SocialDone` (contract section 3).
#[test]
fn say_and_done_layout() {
    let body = ToRealm {
        req: 2,
        ask: Ask::Say {
            room: 1,
            emote: false,
            text: "hi".into(),
        },
    }
    .body();
    assert_eq!(body, [0x72, 2, 0, 0, 0, 1, 0, 2, b'h', b'i']);
    let body = FromRealm::Done(Done {
        wait_s: 3,
        ..Done::refused(2, kind::SAY, ErrorCode::RATE_LIMITED)
    })
    .body();
    let mut want = vec![0x7D, 2, 0, 0, 0, 0x72, 24, 0, 3, 0, 0, 0, 0, 0];
    let text = ErrorCode::RATE_LIMITED.text();
    want.push(text.len() as u8);
    want.extend_from_slice(text.as_bytes());
    assert_eq!(body, want);
}

#[test]
fn req_of_each_message() {
    assert_eq!(FromRealm::State(state()).req(), Some(1));
    assert_eq!(FromRealm::DmLine(dm_line(0, 1)).req(), Some(0));
    assert_eq!(FromRealm::DmLine(dm_line(6, 1)).req(), Some(6));
    assert_eq!(FromRealm::ListEnd { req: 5 }.req(), Some(5));
    let line = RoomLine {
        who: who(),
        ..RoomLine::default()
    };
    assert_eq!(FromRealm::RoomLine(line).req(), None);
}

#[test]
fn done_message_reads_the_code_or_the_realms_text() {
    let d = Done::refused(1, kind::WHISPER, ErrorCode::WHISPER_BLOCKED);
    assert!(!d.is_ok());
    assert_eq!(d.message(), ErrorCode::WHISPER_BLOCKED.text());
    let muted = Done {
        wait_s: 60,
        ..Done::refused(1, kind::SAY, ErrorCode::MUTED)
    };
    assert_eq!(muted.message(), "You are muted. Wait 60 s.");
    let unknown = Done {
        text: "A new rule.".into(),
        ..Done::refused(1, kind::SAY, ErrorCode(999))
    };
    assert_eq!(unknown.message(), "A new rule.");
    assert_eq!(Done::ok(1, kind::SAY).message(), "");
}

/// The largest legal row of every social, guild, friends and ladder
/// message (the longest names, tag and text) fits `MAX_FRAME`.
#[test]
fn social_rows_fit_a_frame() {
    let text = "t".repeat(MAX_CHAT);
    let social = [
        FromRealm::State(SocialState {
            mod_id: "m".repeat(MAX_MOD),
            mod_label: "l".repeat(MAX_MOD),
            guild: Some(GuildBrief {
                guild: u64::MAX,
                tag: "T".repeat(MAX_TAG),
                name: "n".repeat(MAX_GUILD_NAME),
                my_rank: 2,
            }),
            req: u32::MAX,
            ..state()
        }),
        FromRealm::RoomLine(RoomLine {
            room: room::MAX,
            line: u64::MAX,
            what: line_what::SAID,
            who: longest_who(),
            text: text.clone(),
            at_ms: u64::MAX,
        }),
        FromRealm::DmLine(DmLine {
            req: u32::MAX,
            dm: u64::MAX,
            peer_account: "a".repeat(MAX_ACCOUNT),
            peer_char: "c".repeat(MAX_FROM),
            outgoing: true,
            stored: true,
            text: text.clone(),
            at_ms: u64::MAX,
        }),
        FromRealm::DmConversationRow {
            req: u32::MAX,
            row: DmConversationRow {
                peer_account: "a".repeat(MAX_ACCOUNT),
                peer_char: "c".repeat(MAX_FROM),
                place: place::HIDDEN,
                unread: u16::MAX,
                last_ms: u64::MAX,
                last_text: "l".repeat(MAX_LAST_TEXT),
            },
        },
        FromRealm::UserRow {
            req: u32::MAX,
            who: longest_who(),
            place: place::IN_GAME,
        },
        FromRealm::RoomPresence(RoomPresence {
            room: room::MAX,
            op: presence_op::JOINED,
            who: longest_who(),
            place: place::LOBBY,
            count: u16::MAX,
        }),
        FromRealm::Done(Done {
            id: Some(u64::MAX),
            text: "x".repeat(MAX_ERROR_TEXT),
            ..Done::refused(u32::MAX, kind::WHISPER, ErrorCode::INBOX_FULL)
        }),
    ];
    for m in social {
        let body = m.body();
        assert!(body.len() <= MAX_FRAME, "{:#x}: {}", body[0], body.len());
        assert_eq!(FromRealm::parse(&body), Ok(m));
    }
    let guild_rows = [
        guild::FromRealm::GuildInfo {
            req: u32::MAX,
            info: guild::GuildInfo {
                guild: u64::MAX,
                tag: "T".repeat(MAX_TAG),
                name: "n".repeat(MAX_GUILD_NAME),
                motd: "m".repeat(guild::MAX_MOTD),
                created_ms: u64::MAX,
                members: u16::MAX,
                leader_char: "c".repeat(MAX_FROM),
                my_rank: guild::rank::NONE,
            },
        },
        guild::FromRealm::GuildMember {
            req: u32::MAX,
            member: guild::GuildMember {
                who: longest_who(),
                place: place::IN_GAME,
                rank: guild::rank::LEADER,
                joined_ms: u64::MAX,
                last_seen_ms: u64::MAX,
            },
        },
        guild::FromRealm::GuildNotice(guild::GuildNotice {
            what: guild::guild_notice::MOTD,
            guild: u64::MAX,
            tag: "T".repeat(MAX_TAG),
            name: "n".repeat(MAX_GUILD_NAME),
            who: "c".repeat(MAX_FROM),
            by: "b".repeat(MAX_FROM),
            at_ms: u64::MAX,
        }),
        guild::FromRealm::FriendRow {
            req: u32::MAX,
            row: guild::FriendRow {
                who: longest_who(),
                place: place::IN_GAME,
                mutual: true,
                game: Some(guild::FriendGame {
                    game_id: u32::MAX,
                    game_name: "g".repeat(guild::MAX_GAME_NAME),
                    joinable: ErrorCode::LEVEL_RANGE.0,
                }),
                last_seen_ms: u64::MAX,
                added_ms: u64::MAX,
            },
        },
        guild::FromRealm::FriendNotice(guild::FriendNotice {
            what: guild::friend_notice::ENTERED_GAME,
            who: longest_who(),
            place: place::IN_GAME,
            game: Some(guild::FriendGame {
                game_id: u32::MAX,
                game_name: "g".repeat(guild::MAX_GAME_NAME),
                joinable: 0,
            }),
            at_ms: u64::MAX,
        }),
    ];
    for m in guild_rows {
        let body = m.body();
        assert!(body.len() <= MAX_FRAME, "{:#x}: {}", body[0], body.len());
        assert_eq!(guild::FromRealm::parse(&body), Ok(m));
    }
    let ladder_rows = [
        ladder::FromRealm::SeasonRow {
            req: u32::MAX,
            row: ladder::SeasonRow {
                season: u16::MAX,
                name: "s".repeat(ladder::MAX_SEASON_NAME),
                started_ms: u64::MAX,
                ended_ms: u64::MAX,
                test: true,
                current: true,
            },
        },
        ladder::FromRealm::Head(ladder::LadderHead {
            req: u32::MAX,
            season: u16::MAX,
            split: 3,
            class: ladder::ALL_CLASSES,
            total: u32::MAX,
            first_rank: u32::MAX,
            rows: ladder::MAX_ROWS,
            my_rank: Some(u32::MAX),
            updated_ms: u64::MAX,
            now_ms: u64::MAX,
        }),
        ladder::FromRealm::Row {
            req: u32::MAX,
            row: ladder::LadderRow {
                rank: u32::MAX,
                char: "c".repeat(MAX_FROM),
                class: 6,
                level: 99,
                experience: u64::MAX,
                tag: Some("T".repeat(MAX_TAG)),
                flags: 0x1F,
            },
        },
    ];
    for m in ladder_rows {
        let body = m.body();
        assert!(body.len() <= MAX_FRAME, "{:#x}: {}", body[0], body.len());
        assert_eq!(ladder::FromRealm::parse(&body), Ok(m));
    }
    // The longest requests fit too.
    for ask in [
        Ask::Whisper {
            to: "t".repeat(MAX_TARGET),
            text: text.clone(),
        },
        Ask::Say {
            room: room::GUILD,
            emote: false,
            text,
        },
    ] {
        let m = ToRealm { req: u32::MAX, ask };
        assert!(m.body().len() <= MAX_FRAME);
        assert_eq!(ToRealm::parse(&m.body()), Ok(m));
    }
}

/// Trailing bytes, over-long fields, values outside their field and kinds
/// of other areas are malformed.
#[test]
fn social_malformed_refused() {
    // Trailing bytes after a request and after an answer.
    let mut body = ToRealm {
        req: 1,
        ask: Ask::Hello { rooms: 3 },
    }
    .body();
    body.push(0);
    assert_eq!(ToRealm::parse(&body), bad());
    let mut body = FromRealm::ListEnd { req: 1 }.body();
    body.push(0);
    assert_eq!(FromRealm::parse(&body), bad());
    // Over-long text, target, account, tag.
    let long_say = ToRealm {
        req: 1,
        ask: Ask::Say {
            room: 0,
            emote: false,
            text: "t".repeat(MAX_CHAT + 1),
        },
    };
    assert_eq!(ToRealm::parse(&long_say.body()), bad());
    let long_to = ToRealm {
        req: 1,
        ask: Ask::Whisper {
            to: "t".repeat(MAX_TARGET + 1),
            text: "hi".into(),
        },
    };
    assert_eq!(ToRealm::parse(&long_to.body()), bad());
    for who in [
        Who {
            account: "a".repeat(MAX_ACCOUNT + 1),
            ..longest_who()
        },
        Who {
            tag: Some("T".repeat(MAX_TAG + 1)),
            ..longest_who()
        },
        Who {
            char: "c".repeat(MAX_FROM + 1),
            ..longest_who()
        },
        // A class past the Assassin that isn't "none", an unknown flag.
        Who {
            class: 7,
            ..longest_who()
        },
        Who {
            flags: 0x80,
            ..longest_who()
        },
    ] {
        let m = FromRealm::UserRow {
            req: 1,
            who,
            place: 0,
        };
        assert_eq!(FromRealm::parse(&m.body()), bad());
    }
    let long_line = FromRealm::DmLine(DmLine {
        text: "t".repeat(MAX_CHAT + 1),
        ..dm_line(0, 1)
    });
    assert_eq!(FromRealm::parse(&long_line.body()), bad());
    let long_last = FromRealm::DmConversationRow {
        req: 1,
        row: DmConversationRow {
            last_text: "l".repeat(MAX_LAST_TEXT + 1),
            ..DmConversationRow::default()
        },
    };
    assert_eq!(FromRealm::parse(&long_last.body()), bad());
    // Values outside their field: a room past 15, a place past hidden, a
    // line kind past guild events, a presence op past changed, privacy and
    // rank values past their last.
    let say_room_16 = [kind::SAY, 1, 0, 0, 0, 16, 0, 1, b'x'];
    assert_eq!(ToRealm::parse(&say_room_16), bad());
    let user_place_4 = FromRealm::UserRow {
        req: 1,
        who: who(),
        place: 4,
    };
    assert_eq!(FromRealm::parse(&user_place_4.body()), bad());
    let line_what_6 = FromRealm::RoomLine(RoomLine {
        what: 6,
        who: who(),
        ..RoomLine::default()
    });
    assert_eq!(FromRealm::parse(&line_what_6.body()), bad());
    let op_3 = FromRealm::RoomPresence(RoomPresence {
        op: 3,
        who: who(),
        ..RoomPresence::default()
    });
    assert_eq!(FromRealm::parse(&op_3.body()), bad());
    for s in [
        SocialState {
            whispers: 3,
            ..state()
        },
        SocialState {
            show_where: 2,
            ..state()
        },
        SocialState {
            guild: Some(GuildBrief {
                my_rank: 3,
                ..GuildBrief::default()
            }),
            ..state()
        },
    ] {
        assert_eq!(FromRealm::parse(&FromRealm::State(s).body()), bad());
    }
    // A short body, an empty body, kinds of other areas.
    assert_eq!(ToRealm::parse(&[kind::HELLO, 1, 0]), bad());
    assert_eq!(ToRealm::parse(&[]), bad());
    assert_eq!(ToRealm::parse(&[kind::STATE, 1, 0, 0, 0]), bad());
    assert_eq!(FromRealm::parse(&[kind::SAY, 1, 0, 0, 0]), bad());
    assert_eq!(FromRealm::parse(&[0x49, 0]), bad());
}

fn bad<T>() -> Result<T, NetError> {
    Err(NetError::Malformed)
}
