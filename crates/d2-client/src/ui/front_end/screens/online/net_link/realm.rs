//! The realm lobby's requests and answers on the link (chat rooms, DMs,
//! user lists, guilds, friends, the ladder:
//! `docs/plans/server/lobby-contract.md` sections 3-6): `d2_online`'s
//! [`SocialClient`] and [`LadderClient`] number the requests (one counter
//! per session, from 1) and put each list together while pushes arrive in
//! between; this maps their answers onto the screens' events.

use std::collections::HashMap;

use d2_net::msg::guild::{self, FriendEdit};
use d2_net::msg::{ladder, social, ErrorCode};
use d2_online::{LadderAnswer, LadderClient, SocialAnswer, SocialAsk, SocialClient};

use super::super::link::{
    Asked, DmConversation, DmLine, Done, Event, Failure, FriendGame, FriendNotice, FriendOp,
    FriendRow, GuildBrief, GuildInfo, GuildMember, GuildNotice, GuildOp, GuildQuery, LadderAt,
    LadderPage, LadderRow, LadderSeason, Request, RoomLine, RoomPresence, SocialState, UserRow,
    Where, Who,
};

/// The realm lobby of one session.
#[derive(Debug)]
pub(super) struct RealmLobby {
    pub(super) social: SocialClient,
    pub(super) ladder: LadderClient,
    /// The requests in flight whose events name what they asked (the DM
    /// peer, the room, friends or ignored), by number.
    pub(super) waiting: HashMap<u32, Request>,
}

/// A request as the social or the ladder client sends it.
enum Ask {
    Social(SocialAsk),
    Ladder(ladder::Ask),
}

impl RealmLobby {
    /// A new session's: its requests are numbered from 1.
    pub(super) fn new() -> Self {
        let social = SocialClient::new();
        let ladder = LadderClient::sharing(social.counter());
        Self {
            social,
            ladder,
            waiting: HashMap::new(),
        }
    }

    /// Numbers a realm-lobby request and records it as in flight: the body
    /// to send (`None`: not a realm-lobby request).
    pub(super) fn number(&mut self, r: Request) -> Option<Vec<u8>> {
        let (req, body) = match ask(&r)? {
            Ask::Social(a) => self.social.prepare(a),
            Ask::Ladder(a) => self.ladder.prepare(a),
        };
        if matches!(
            r,
            Request::DmHistory { .. } | Request::UserList { .. } | Request::FriendsGet { .. }
        ) {
            self.waiting.insert(req, r);
        }
        Some(body)
    }

    /// One S→C body: `None` if it isn't the realm lobby's, else the event
    /// it makes (`None`: a list row, an answer nobody waits for, or a body
    /// that breaks the contract).
    pub(super) fn feed(&mut self, body: &[u8]) -> Option<Option<Event>> {
        // Before the lobby's range: the ladder's kinds sit inside it. One
        // body makes at most one answer.
        match self.ladder.feed(body) {
            Ok(true) => return Some(self.ladder.take().pop().and_then(ladder_event)),
            Ok(false) => {}
            Err(_) => return Some(None),
        }
        match self.social.feed(body) {
            Ok(true) => Some(self.social.take().pop().and_then(|a| self.social_event(a))),
            Ok(false) => None,
            Err(_) => Some(None),
        }
    }

    fn social_event(&mut self, a: SocialAnswer) -> Option<Event> {
        use SocialAnswer as A;
        let request = a.req().and_then(|req| self.waiting.remove(&req));
        Some(match a {
            A::State(s) => Event::SocialState(state(s)),
            A::RoomLine(l) => Event::RoomLine(room_line(l)),
            A::Dm(l) => Event::DmLine(dm_line(l)),
            A::Presence(p) => Event::RoomPresence(RoomPresence {
                room: p.room,
                op: p.op,
                who: who(p.who),
                place: place(p.place),
                count: p.count,
            }),
            A::GuildNotice(n) => Event::GuildNotice(GuildNotice {
                what: n.what,
                guild: n.guild,
                tag: n.tag,
                name: n.name,
                who: n.who,
                by: n.by,
                at_ms: n.at_ms,
            }),
            A::FriendNotice(n) => Event::FriendNotice(FriendNotice {
                what: n.what,
                who: who(n.who),
                place: place(n.place),
                game: n.game.map(friend_game),
                at_ms: n.at_ms,
            }),
            A::Done(d) => {
                let asked = asked_of(d.request_kind)?;
                if d.is_ok() {
                    Event::Done(Done {
                        asked,
                        flags: d.flags,
                        id: d.id,
                    })
                } else {
                    refused(asked, d.code, d.wait_s, d.text)
                }
            }
            A::Error { error, .. } => refused(
                asked_of(error.request_kind)?,
                error.code,
                error.wait_s,
                error.text,
            ),
            A::DmConversations { head, rows, .. } => Event::DmConversations {
                total: head.total,
                page: head.page,
                rows: rows
                    .into_iter()
                    .map(|c| DmConversation {
                        peer_account: c.peer_account,
                        peer_char: c.peer_char,
                        place: place(c.place),
                        unread: c.unread,
                        last_ms: c.last_ms,
                        last_text: c.last_text,
                    })
                    .collect(),
            },
            A::DmHistory { lines, .. } => {
                let Some(Request::DmHistory { peer, .. }) = request else {
                    return None;
                };
                Event::DmHistory {
                    peer,
                    lines: lines.into_iter().map(dm_line).collect(),
                }
            }
            A::Users { head, rows, .. } => {
                let Some(Request::UserList { room, .. }) = request else {
                    return None;
                };
                Event::Users {
                    room,
                    total: head.total,
                    rows: rows
                        .into_iter()
                        .map(|u| UserRow {
                            who: who(u.who),
                            place: place(u.place),
                        })
                        .collect(),
                }
            }
            A::Guild { info, members, .. } => Event::Guild {
                info: guild_info(info),
                members: members
                    .into_iter()
                    .map(|m| GuildMember {
                        who: who(m.who),
                        place: place(m.place),
                        rank: m.rank,
                        joined_ms: m.joined_ms,
                        last_seen_ms: m.last_seen_ms,
                    })
                    .collect(),
            },
            A::GuildInvites { invites, .. } => {
                Event::GuildInvites(invites.into_iter().map(guild_info).collect())
            }
            A::GuildByTag { info, .. } => Event::GuildFound(guild_info(info)),
            A::Friends { rows, .. } => Event::Friends {
                ignored: matches!(request, Some(Request::FriendsGet { ignored: true })),
                rows: rows
                    .into_iter()
                    .map(|f| FriendRow {
                        who: who(f.who),
                        place: place(f.place),
                        mutual: f.mutual,
                        game: f.game.map(friend_game),
                        last_seen_ms: f.last_seen_ms,
                        added_ms: f.added_ms,
                    })
                    .collect(),
            },
        })
    }
}

/// A realm-lobby request as its client's ask (`None`: not one).
fn ask(r: &Request) -> Option<Ask> {
    use social::Ask as S;
    let s = |a: S| Some(Ask::Social(a.into()));
    let g = |a: guild::Ask| Some(Ask::Social(a.into()));
    match r.clone() {
        Request::SocialHello { rooms } => s(S::Hello { rooms }),
        Request::Say { room, emote, text } => s(S::Say { room, emote, text }),
        Request::Whisper { to, text } => s(S::Whisper { to, text }),
        Request::DmConversations { page } => s(S::DmConversations { page }),
        Request::DmHistory {
            peer,
            before_dm,
            rows,
        } => s(S::DmHistory {
            peer,
            before_dm,
            rows,
        }),
        Request::DmRead {
            peer_account,
            up_to_dm,
        } => s(S::DmRead {
            peer_account,
            up_to_dm,
        }),
        Request::UserList { room, watch } => s(S::UserList { room, watch }),
        Request::GuildGet(q) => {
            let (what, tag) = match q {
                GuildQuery::Mine => (guild::guild_get::MINE, String::new()),
                GuildQuery::Invites => (guild::guild_get::INVITES, String::new()),
                GuildQuery::ByTag(t) => (guild::guild_get::BY_TAG, t),
            };
            g(guild::Ask::GuildGet { what, tag })
        }
        Request::GuildCreate { tag, name } => g(guild::Ask::GuildCreate { tag, name }),
        Request::GuildInvite { who } => g(guild::Ask::GuildInvite { who }),
        Request::GuildAnswer { guild, accept } => g(guild::Ask::GuildAnswer { guild, accept }),
        Request::GuildManage(op) => {
            use guild::guild_op as o;
            let (op, arg) = match op {
                GuildOp::Leave => (o::LEAVE, String::new()),
                GuildOp::Kick(a) => (o::KICK, a),
                GuildOp::Promote(a) => (o::PROMOTE, a),
                GuildOp::Demote(a) => (o::DEMOTE, a),
                GuildOp::MakeLeader(a) => (o::MAKE_LEADER, a),
                GuildOp::Disband(a) => (o::DISBAND, a),
                GuildOp::Motd(a) => (o::MOTD, a),
            };
            g(guild::Ask::GuildManage { op, arg })
        }
        Request::FriendsGet { ignored } => g(guild::Ask::FriendsGet {
            what: if ignored {
                guild::friends_get::IGNORED
            } else {
                guild::friends_get::FRIENDS
            },
        }),
        Request::FriendEdit(op) => g(guild::Ask::FriendEdit(match op {
            FriendOp::Add(w) => FriendEdit::Add(w),
            FriendOp::Remove(w) => FriendEdit::Remove(w),
            FriendOp::Ignore(w) => FriendEdit::Ignore(w),
            FriendOp::Unignore(w) => FriendEdit::Unignore(w),
            FriendOp::Privacy(p) => FriendEdit::Privacy {
                whispers: p.whispers,
                show_where: p.show_where,
            },
        })),
        Request::LadderSeasons => Some(Ask::Ladder(ladder::Ask::Seasons)),
        Request::LadderQuery(q) => {
            let (mode, from_rank, find) = match q.at {
                LadderAt::Rank(r) => (ladder::mode::FROM_RANK, r, String::new()),
                LadderAt::Find(n) => (ladder::mode::FIND, 0, n),
                LadderAt::Me => (ladder::mode::AROUND_ME, 0, String::new()),
            };
            Some(Ask::Ladder(ladder::Ask::Query(ladder::LadderQuery {
                season: q.season,
                split: q.split,
                class: q.class,
                mode,
                from_rank,
                find,
                rows: q.rows,
            })))
        }
        _ => None,
    }
}

/// The request a realm-lobby kind asks.
pub(super) fn asked_of(kind: u8) -> Option<Asked> {
    use guild::kind as g;
    use ladder::kind as l;
    use social::kind as s;
    Some(match kind {
        s::HELLO => Asked::SocialHello,
        s::SAY => Asked::Say,
        s::WHISPER => Asked::Whisper,
        s::DM_CONVERSATIONS => Asked::DmConversations,
        s::DM_HISTORY => Asked::DmHistory,
        s::DM_READ => Asked::DmRead,
        s::USER_LIST => Asked::UserList,
        g::GUILD_GET => Asked::GuildGet,
        g::GUILD_CREATE => Asked::GuildCreate,
        g::GUILD_INVITE => Asked::GuildInvite,
        g::GUILD_ANSWER => Asked::GuildAnswer,
        g::GUILD_MANAGE => Asked::GuildManage,
        g::FRIENDS_GET => Asked::FriendsGet,
        g::FRIEND_EDIT => Asked::FriendEdit,
        l::SEASONS => Asked::LadderSeasons,
        l::QUERY => Asked::LadderQuery,
        _ => return None,
    })
}

fn ladder_event(a: LadderAnswer) -> Option<Event> {
    Some(match a {
        LadderAnswer::Seasons { rows, end, .. } if end.is_ok() => Event::LadderSeasons(
            rows.into_iter()
                .map(|s| LadderSeason {
                    season: s.season,
                    name: s.name,
                    started_ms: s.started_ms,
                    ended_ms: s.ended_ms,
                    test: s.test,
                    current: s.current,
                })
                .collect(),
        ),
        LadderAnswer::Seasons { end, .. } => {
            refused(Asked::LadderSeasons, end.code, end.wait_s, String::new())
        }
        LadderAnswer::Page {
            head: Some(h),
            rows,
            pinned,
            end,
            ..
        } if end.is_ok() => Event::Ladder(LadderPage {
            season: h.season,
            split: h.split,
            class: h.class,
            total: h.total,
            first_rank: h.first_rank,
            my_rank: h.my_rank,
            updated_ms: h.updated_ms,
            now_ms: h.now_ms,
            rows: rows.into_iter().map(ladder_row).collect(),
            pinned: pinned.map(ladder_row),
        }),
        LadderAnswer::Page { end, .. } => {
            refused(Asked::LadderQuery, end.code, end.wait_s, String::new())
        }
        LadderAnswer::Error { error, .. } => refused(
            asked_of(error.request_kind)?,
            error.code,
            error.wait_s,
            error.text,
        ),
    })
}

fn refused(asked: Asked, code: ErrorCode, wait_s: u32, text: String) -> Event {
    Event::Refused {
        asked,
        failure: Failure { code, wait_s, text },
    }
}

fn place(p: u8) -> Where {
    Where::from_wire(p).unwrap_or_default()
}

fn who(w: social::Who) -> Who {
    Who {
        char: w.char,
        account: w.account,
        tag: w.tag,
        class: w.class,
        level: w.level,
        flags: w.flags,
    }
}

fn state(s: social::SocialState) -> SocialState {
    SocialState {
        rooms: s.rooms,
        mod_id: s.mod_id,
        mod_label: s.mod_label,
        guild: s.guild.map(|g| GuildBrief {
            guild: g.guild,
            tag: g.tag,
            name: g.name,
            my_rank: g.my_rank,
        }),
        dm_unread: s.dm_unread,
        dm_conversations_unread: s.dm_conversations_unread,
        friends_online: s.friends_online,
        guild_invites: s.guild_invites,
        whispers: s.whispers,
        show_where: s.show_where,
        say_lines: s.limits.say_lines,
        say_window_s: s.limits.say_window_s,
        global_lines: s.limits.global_lines,
        global_window_s: s.limits.global_window_s,
        muted_until_ms: s.muted_until_ms,
        now_ms: s.now_ms,
    }
}

fn room_line(l: social::RoomLine) -> RoomLine {
    RoomLine {
        room: l.room,
        line: l.line,
        what: l.what,
        who: who(l.who),
        text: l.text,
        at_ms: l.at_ms,
    }
}

fn dm_line(l: social::DmLine) -> DmLine {
    DmLine {
        dm: l.dm,
        peer_account: l.peer_account,
        peer_char: l.peer_char,
        outgoing: l.outgoing,
        stored: l.stored,
        text: l.text,
        at_ms: l.at_ms,
    }
}

fn guild_info(g: guild::GuildInfo) -> GuildInfo {
    GuildInfo {
        guild: g.guild,
        tag: g.tag,
        name: g.name,
        motd: g.motd,
        created_ms: g.created_ms,
        members: g.members,
        leader_char: g.leader_char,
        my_rank: g.my_rank,
    }
}

fn friend_game(g: guild::FriendGame) -> FriendGame {
    FriendGame {
        game_id: g.game_id,
        game_name: g.game_name,
        joinable: g.joinable,
    }
}

fn ladder_row(r: ladder::LadderRow) -> LadderRow {
    LadderRow {
        rank: r.rank,
        char: r.char,
        class: r.class,
        level: r.level,
        experience: r.experience,
        tag: r.tag,
        flags: r.flags,
    }
}
