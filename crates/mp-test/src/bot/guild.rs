//! The guilds' verbs (`docs/plans/server/lobby-contract.md` section 4,
//! kinds 0xF0-0xF7), driven on the bot's control stream as the game's
//! client sends them. A guild step says the session's chat hello first (so
//! its Guild room and state follow the guild); the `GuildNotice` pushes are
//! kept for the `guild` query.

use std::time::{Duration, Instant};

use d2_net::msg::guild::{
    guild_get, guild_notice, guild_op, rank, Ask, FromRealm, GuildInfo, GuildMember, GuildNotice,
    ToRealm,
};
use d2_net::msg::social::{self, Done};
use d2_net::msg::{kind as msg_kind, ErrorMsg};

use super::chat::{done_outcome, fail, place_name, yes, Args, LIST_AGAIN};
use super::{Bot, Failed, Outcome};

/// What the bot remembers of its guilds during a session.
#[derive(Debug, Default)]
pub struct Memo {
    /// The notices pushed to it, oldest first.
    notices: Vec<GuildNotice>,
    /// The last `guild` query (its words), when it was asked and its
    /// answer.
    asked: Option<(String, Instant, Outcome)>,
}

/// One message of a guild request's answer.
#[derive(Debug)]
enum Got {
    Social(social::FromRealm),
    Guild(FromRealm),
}

fn notice_name(what: u8) -> &'static str {
    match what {
        guild_notice::INVITED_YOU => "invited",
        guild_notice::JOINED => "joined",
        guild_notice::LEFT => "left",
        guild_notice::KICKED => "kicked",
        guild_notice::PROMOTED => "promoted",
        guild_notice::DEMOTED => "demoted",
        guild_notice::LEADER_CHANGED => "leader",
        guild_notice::DISBANDED => "disbanded",
        guild_notice::MOTD => "motd",
        _ => "other",
    }
}

fn rank_name(r: u8) -> &'static str {
    match r {
        rank::MEMBER => "member",
        rank::OFFICER => "officer",
        rank::LEADER => "leader",
        _ => "none",
    }
}

/// The `SocialDone` among an answer.
fn done_of(got: &[Got]) -> Option<&Done> {
    got.iter().rev().find_map(|g| match g {
        Got::Social(social::FromRealm::Done(d)) => Some(d),
        _ => None,
    })
}

fn infos(got: &[Got]) -> Vec<&GuildInfo> {
    got.iter()
        .filter_map(|g| match g {
            Got::Guild(FromRealm::GuildInfo { info, .. }) => Some(info),
            _ => None,
        })
        .collect()
}

fn members(got: &[Got]) -> Vec<&GuildMember> {
    got.iter()
        .filter_map(|g| match g {
            Got::Guild(FromRealm::GuildMember { member, .. }) => Some(member),
            _ => None,
        })
        .collect()
}

impl Bot {
    /// One guild verb (`guild-create`, `guild-invite`, `guild-answer`,
    /// `guild-leave`, `guild-kick`, `guild-promote`, `guild-demote`,
    /// `guild-leader`, `guild-disband`, `guild-motd`, the `guild` query).
    pub fn guild(&mut self, verb: &str, args: &[String], wait: Duration) -> Outcome {
        let started = Instant::now();
        if let Err(f) = self.social_ready(wait) {
            return f.outcome(started.elapsed().as_millis());
        }
        let wait = wait.saturating_sub(started.elapsed());
        let a = Args::of(args);
        let key = format!("{verb} {}", args.join(" "));
        if verb == "guild" {
            if let Some((k, at, o)) = &self.guild.asked {
                if *k == key && at.elapsed() < LIST_AGAIN {
                    return o.clone();
                }
            }
        }
        let arg = || a.plain.join(" ");
        let op = |op: u8, arg: String| Ask::GuildManage { op, arg };
        let r = match verb {
            "guild" => self.guild_query(&a, wait),
            "guild-create" => match a.plain.split_first() {
                Some((tag, name)) => self.guild_done(
                    Ask::GuildCreate {
                        tag: (*tag).to_owned(),
                        name: name.join(" "),
                    },
                    wait,
                ),
                None => Err(fail("guild-create takes TAG NAME..")),
            },
            "guild-invite" => self.guild_done(Ask::GuildInvite { who: arg() }, wait),
            "guild-answer" => self.guild_answer(&a, wait),
            "guild-leave" => self.guild_done(op(guild_op::LEAVE, String::new()), wait),
            "guild-kick" => self.guild_done(op(guild_op::KICK, arg()), wait),
            "guild-promote" => self.guild_done(op(guild_op::PROMOTE, arg()), wait),
            "guild-demote" => self.guild_done(op(guild_op::DEMOTE, arg()), wait),
            "guild-leader" => self.guild_done(op(guild_op::MAKE_LEADER, arg()), wait),
            "guild-disband" => self.guild_done(op(guild_op::DISBAND, arg()), wait),
            "guild-motd" => self.guild_done(op(guild_op::MOTD, arg()), wait),
            _ => Err(fail(format!("{verb} has no driver"))),
        };
        let o = r.unwrap_or_else(|f| f.outcome(started.elapsed().as_millis()));
        if verb == "guild" {
            self.guild.asked = Some((key, started, o.clone()));
        } else {
            // A change: the next query asks again.
            self.guild.asked = None;
        }
        o
    }

    /// Sends one guild request and reads its answer: the messages with its
    /// request number up to its last (`SocialDone`, the list's end, or the
    /// one `GuildInfo` of a guild by tag). Pushes in between are kept.
    fn guild_send(&mut self, ask: Ask, wait: Duration) -> Result<Vec<Got>, Failed> {
        let single = matches!(
            ask,
            Ask::GuildGet {
                what: guild_get::BY_TAG,
                ..
            }
        );
        let msg = ToRealm {
            req: self.social_next_req(),
            ask,
        };
        let (kind, req) = (msg.kind(), msg.req);
        self.send_control(msg.body()).map_err(Failed::Local)?;
        let deadline = Instant::now() + wait;
        let mut got = Vec::new();
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            let b = self.read_body(left).map_err(Failed::Local)?;
            match b.first() {
                Some(&msg_kind::ERROR) => {
                    let e = ErrorMsg::parse(&b).map_err(|_| fail("a malformed error"))?;
                    if e.request_kind == kind {
                        return Err(Failed::Server(e));
                    }
                    self.note_session_error(e);
                }
                Some(k) if msg_kind::SOCIAL.contains(k) => {
                    let m = social::FromRealm::parse(&b)
                        .map_err(|e| fail(format!("a malformed social message 0x{k:02X}: {e}")))?;
                    if m.req() != Some(req) {
                        self.social_pushed(m);
                        continue;
                    }
                    let last = matches!(
                        m,
                        social::FromRealm::Done(_) | social::FromRealm::ListEnd { .. }
                    );
                    got.push(Got::Social(m));
                    if last {
                        return Ok(got);
                    }
                }
                Some(k) if msg_kind::GUILD.contains(k) => {
                    let m = FromRealm::parse(&b)
                        .map_err(|e| fail(format!("a malformed guild message 0x{k:02X}: {e}")))?;
                    if m.req() != Some(req) {
                        self.guild_pushed(m);
                        continue;
                    }
                    got.push(Got::Guild(m));
                    if single {
                        return Ok(got);
                    }
                }
                Some(_) => {
                    if !self.handle_event(&b) {
                        return Err(fail(format!(
                            "unexpected answer 0x{:02X} to 0x{kind:02X}",
                            b[0]
                        )));
                    }
                }
                None => return Err(fail("the server sent an empty answer")),
            }
        }
    }

    /// A guild message that isn't the answer waited for: a notice, or a
    /// late answer to an earlier request (dropped).
    pub(super) fn guild_pushed(&mut self, msg: FromRealm) {
        if let FromRealm::GuildNotice(n) = msg {
            self.guild.notices.push(n);
        }
    }

    /// A request answered with `SocialDone`: its outcome (facts `guild`
    /// for a create).
    fn guild_done(&mut self, ask: Ask, wait: Duration) -> Result<Outcome, Failed> {
        let started = Instant::now();
        let got = self.guild_send(ask, wait)?;
        let d = done_of(&got).ok_or_else(|| fail("no SocialDone"))?;
        let mut o = done_outcome(d, started.elapsed().as_millis());
        if let Some(id) = d.id {
            o.facts.remove("dm");
            o = o.fact("guild", id);
        }
        Ok(o)
    }

    /// `guild-answer accept|decline [TAG]`: the newest invite, or TAG's,
    /// from the invites list, else from the invite notices (an expired
    /// invite is gone from the list but is still answered).
    fn guild_answer(&mut self, a: &Args, wait: Duration) -> Result<Outcome, Failed> {
        let accept = match a.plain.first().copied() {
            Some("accept") => true,
            Some("decline") => false,
            w => {
                return Err(fail(format!(
                    "guild-answer takes accept|decline, not {w:?}"
                )))
            }
        };
        let tag = a.plain.get(1).copied();
        let fits = |t: &str| tag.is_none_or(|w| w.eq_ignore_ascii_case(t));
        let got = self.guild_send(
            Ask::GuildGet {
                what: guild_get::INVITES,
                tag: String::new(),
            },
            wait,
        )?;
        let listed = infos(&got)
            .into_iter()
            .find(|g| fits(&g.tag))
            .map(|g| g.guild);
        let guild = listed.or_else(|| {
            self.guild
                .notices
                .iter()
                .rev()
                .find(|n| n.what == guild_notice::INVITED_YOU && fits(&n.tag))
                .map(|n| n.guild)
        });
        let Some(guild) = guild else {
            return Err(fail("no invite to answer"));
        };
        self.guild_done(Ask::GuildAnswer { guild, accept }, wait)
    }

    /// `guild [TAG]`: its own guild (or TAG's public facts), its invites
    /// and the notices so far.
    fn guild_query(&mut self, a: &Args, wait: Duration) -> Result<Outcome, Failed> {
        self.drain_events();
        let mut o = Outcome::ok();
        if let Some(tag) = a.plain.first() {
            let got = self.guild_send(
                Ask::GuildGet {
                    what: guild_get::BY_TAG,
                    tag: (*tag).to_owned(),
                },
                wait,
            )?;
            if let Some(d) = done_of(&got) {
                return Ok(done_outcome(d, 0));
            }
            let g = infos(&got)
                .first()
                .copied()
                .ok_or_else(|| fail("no GuildInfo"))?
                .clone();
            o = o
                .fact("in", yes(g.my_rank != rank::NONE))
                .fact("tag", &g.tag)
                .fact("name", &g.name)
                .fact("motd", &g.motd)
                .fact("members", g.members)
                .fact("leader", &g.leader_char);
        } else {
            let got = self.guild_send(
                Ask::GuildGet {
                    what: guild_get::MINE,
                    tag: String::new(),
                },
                wait,
            )?;
            match done_of(&got) {
                Some(d) if d.code == d2_net::msg::ErrorCode::NOT_IN_GUILD => {
                    o = o.fact("in", "no");
                }
                Some(d) => return Ok(done_outcome(d, 0)),
                None => {
                    let g = infos(&got)
                        .first()
                        .copied()
                        .ok_or_else(|| fail("no GuildInfo"))?
                        .clone();
                    o = o
                        .fact("in", "yes")
                        .fact("tag", &g.tag)
                        .fact("name", &g.name)
                        .fact("motd", &g.motd)
                        .fact("members", g.members)
                        .fact("leader", &g.leader_char)
                        .fact("my_rank", rank_name(g.my_rank));
                    for m in members(&got) {
                        o = o
                            .fact(format!("{}.rank", m.who.char), rank_name(m.rank))
                            .fact(format!("{}.where", m.who.char), place_name(m.place));
                    }
                }
            }
            let got = self.guild_send(
                Ask::GuildGet {
                    what: guild_get::INVITES,
                    tag: String::new(),
                },
                wait,
            )?;
            if let Some(d) = done_of(&got) {
                return Ok(done_outcome(d, 0));
            }
            o = o.fact("invites", infos(&got).len());
        }
        o = o.fact("notices", self.guild.notices.len());
        if let Some(n) = self.guild.notices.last() {
            o = o
                .fact("notice", notice_name(n.what))
                .fact("notice.who", &n.who)
                .fact("notice.by", &n.by);
        }
        Ok(o)
    }
}
