//! Guilds (lobby contract section 4; lobby plan L5b): `GuildGet`,
//! `GuildCreate`, `GuildInvite`, `GuildAnswer`, `GuildManage` and the
//! `GuildNotice` pushes.
//!
//! - The store (`d2_accounts::social::Guilds`) keeps the rules: one guild
//!   per account, ranks, invites that last 7 days, the leader can't leave
//!   while others remain, the last member leaving disbands.
//! - After a change the accounts it touched are read again
//!   ([`Social::guild_changed`]): their sessions' Guild room, the `<TAG>`
//!   in their [`Who`] and their `SocialState` follow at once, so a kicked
//!   member stops hearing the room before the event line is said.
//! - Joined, left, kicked, promoted, demoted, leader and motd changes are
//!   an event line in the Guild room (`what` 5) and a `GuildNotice` to
//!   every session of every member (and of the invitee or the one who
//!   left or was kicked).
//! - A disbanded guild's room is forgotten with its lines: a new guild
//!   may get the same number.
//! - Limits (contract 9): one `GuildCreate` a day and 20 `GuildInvite` an
//!   hour per account; `GuildGet` shares the list limit.

use std::time::Instant;

use d2_accounts::social::{GuildMember as StoredMember, GuildRow, Rank, Refusal, ShowWhere};
use d2_limits::{Limiter, Rate};
use d2_net::msg::guild::{
    guild_get, guild_notice, guild_op, kind, rank, Ask, FromRealm, GuildInfo, GuildMember,
    GuildNotice, ToRealm,
};
use d2_net::msg::social::{self, place, who_flags, Done, ListHead, Who, NO_CLASS};
use d2_net::msg::ErrorCode;

use super::rooms::RoomKey;
use super::{lock, wire_ms, Member, Social};
use crate::accounts::AccountId;
use crate::denied::Denied;

/// What this part adds to the ready line's `caps`.
pub const CAPS: &[&str] = &["guilds"];

/// `GuildCreate`s per account per [`CREATE_WINDOW_S`].
pub const CREATES: u32 = 1;
pub const CREATE_WINDOW_S: u64 = 24 * 3600;
/// `GuildInvite`s per account per [`INVITE_WINDOW_S`].
pub const INVITES: u32 = 20;
pub const INVITE_WINDOW_S: u64 = 3600;

/// The guilds' rate limits (contract 9), per account.
#[derive(Debug)]
pub struct GuildLimits {
    create: Limiter<i64>,
    invite: Limiter<i64>,
}

fn limited(r: d2_limits::Refused) -> Denied {
    Denied::wait(ErrorCode::RATE_LIMITED, r.wait_s())
}

impl GuildLimits {
    /// `test`: every window divided by 60 (`limits = test`).
    pub fn new(test: bool) -> Self {
        let rate = |n: u32, s: u64| {
            let r = Rate::per_secs(n, s);
            if test {
                r.test()
            } else {
                r
            }
        };
        Self {
            create: Limiter::new(rate(CREATES, CREATE_WINDOW_S)),
            invite: Limiter::new(rate(INVITES, INVITE_WINDOW_S)),
        }
    }

    /// Whether `account` may create a guild now (nothing counted yet).
    fn may_create(&self, account: i64, now: Instant) -> Result<(), Denied> {
        match self.create.wait(&account, now) {
            w if w.is_zero() => Ok(()),
            wait => Err(limited(d2_limits::Refused { wait })),
        }
    }

    /// A guild was created: it counts.
    fn created(&mut self, account: i64, now: Instant) {
        let _ = self.create.check(&account, now);
    }

    /// An invite by `account`: it counts.
    fn invite(&mut self, account: i64, now: Instant) -> Result<(), Denied> {
        self.invite.check(&account, now).map_err(limited)
    }
}

fn wire_id(id: i64) -> u64 {
    u64::try_from(id).unwrap_or(0)
}

fn rank_name(r: Rank) -> &'static str {
    match r {
        Rank::Member => "member",
        Rank::Officer => "officer",
        Rank::Leader => "leader",
    }
}

impl Social {
    /// The character `account` plays now (a session with one), else
    /// `stored` (its last played), else `*account`.
    fn char_name(&self, account: i64, stored: Option<&str>, account_name: &str) -> String {
        self.presence
            .sessions_of(AccountId(account))
            .into_iter()
            .find_map(|h| h.character.map(|c| c.name))
            .or_else(|| stored.map(str::to_owned))
            .unwrap_or_else(|| format!("*{account_name}"))
    }

    /// `account` as a guild line or row shows it, with `tag`: its online
    /// character, else its last played one.
    fn member_who(
        &self,
        account: i64,
        account_name: &str,
        stored: Option<&str>,
        tag: Option<&str>,
    ) -> Who {
        let sessions = self.presence.sessions_of(AccountId(account));
        match sessions.iter().find(|h| h.character.is_some()) {
            Some(h) => h.who(tag, true),
            None => Who {
                char: stored.unwrap_or_default().to_owned(),
                account: account_name.to_owned(),
                tag: tag.map(str::to_owned),
                class: NO_CLASS,
                level: 0,
                flags: 0,
            },
        }
    }

    /// The asker as notices name it: its selected character, else its
    /// last played one.
    async fn asker_name(&self, who: &Member) -> String {
        if let Some(c) = &who.character {
            return c.name.clone();
        }
        let a = who.account.0;
        let last = self
            .accounts
            .store(move |db| Ok(db.last_character(a)?))
            .await
            .ok()
            .flatten();
        self.char_name(a, last.as_deref(), &who.account_name)
    }

    /// A `GuildInfo` of `g` for an asker of rank `my_rank`.
    fn info(&self, g: &GuildRow, my_rank: u8) -> GuildInfo {
        let leader = self
            .presence
            .sessions_of(AccountId(g.leader))
            .into_iter()
            .find_map(|h| h.character.map(|c| c.name))
            .or_else(|| g.leader_char.clone())
            .unwrap_or_default();
        GuildInfo {
            guild: wire_id(g.id),
            tag: g.tag.clone(),
            name: g.name.clone(),
            motd: g.motd.clone(),
            created_ms: wire_ms(g.created_ms),
            members: g.members,
            leader_char: leader,
            my_rank,
        }
    }

    /// Pushes a `GuildNotice` about `about` by `by` to every session of
    /// the accounts `to`.
    fn guild_notice(&self, what: u8, g: &GuildRow, about: &str, by: &str, to: &[i64]) {
        let body = FromRealm::GuildNotice(GuildNotice {
            what,
            guild: wire_id(g.id),
            tag: g.tag.clone(),
            name: g.name.clone(),
            who: about.to_owned(),
            by: by.to_owned(),
            at_ms: wire_ms(self.clock.now()),
        })
        .body();
        for a in to {
            self.presence.send_to(AccountId(*a), &body);
        }
    }

    /// Guild `guild` is gone: its room and lines are forgotten.
    fn guild_gone(&self, guild: i64) {
        lock(&self.rooms).forget(RoomKey::Guild(wire_id(guild)));
    }

    /// `GuildGet`: the asker's guild with its members, its invites, or a
    /// guild by tag. Sends the answer; a refusal is the caller's.
    async fn guild_get(&self, who: &Member, req: u32, what: u8, tag: &str) -> Result<(), Denied> {
        self.list_limit(who.account)?;
        let a = who.account.0;
        let now = self.clock.now();
        let list_head = |rows: usize| {
            social::FromRealm::ListBegin(ListHead {
                req,
                what: kind::GUILD_GET,
                total: u32::try_from(rows).unwrap_or(u32::MAX),
                page: 0,
                rows: u16::try_from(rows).unwrap_or(u16::MAX),
                now_ms: wire_ms(now),
            })
            .body()
        };
        match what {
            guild_get::MINE => {
                let (g, my_rank, members) = self
                    .accounts
                    .store(move |db| {
                        let (id, rank) = db.guilds().membership(a)?.ok_or(Refusal::NotInGuild)?;
                        let g = db.guilds().get(id)?.ok_or(Refusal::NotInGuild)?;
                        let mut members = Vec::new();
                        for m in db.guilds().members(id)? {
                            let hidden = db.prefs().get(m.account)?.show_where == ShowWhere::Nobody;
                            members.push((m, hidden));
                        }
                        Ok((g, rank, members))
                    })
                    .await?;
                let rows: Vec<GuildMember> = members
                    .iter()
                    .map(|(m, hidden)| self.member_row(&g, m, *hidden))
                    .collect();
                who.out.send(
                    FromRealm::GuildInfo {
                        req,
                        info: self.info(&g, my_rank.as_u8()),
                    }
                    .body(),
                );
                who.out.send(list_head(rows.len()));
                for member in rows {
                    who.out.send(FromRealm::GuildMember { req, member }.body());
                }
            }
            guild_get::INVITES => {
                let invites = self
                    .accounts
                    .store(move |db| Ok(db.guilds().invites(a, now)?))
                    .await?;
                who.out.send(list_head(invites.len()));
                for g in &invites {
                    let info = self.info(g, rank::NONE);
                    who.out.send(FromRealm::GuildInfo { req, info }.body());
                }
            }
            _ => {
                d2_accounts::social::check_guild_tag(tag).map_err(Denied::from)?;
                let tag = tag.to_owned();
                let (g, mine) = self
                    .accounts
                    .store(move |db| {
                        let g = db
                            .guilds()
                            .by_tag(&tag)?
                            .ok_or(Refusal::BadGuildName("No guild has that tag."))?;
                        let mine = db.guilds().membership(a)?.filter(|(id, _)| *id == g.id);
                        Ok((g, mine.map(|(_, r)| r)))
                    })
                    .await?;
                let my_rank = mine.map_or(rank::NONE, Rank::as_u8);
                let info = self.info(&g, my_rank);
                who.out.send(FromRealm::GuildInfo { req, info }.body());
                return Ok(());
            }
        }
        who.out.send(social::FromRealm::ListEnd { req }.body());
        Ok(())
    }

    /// A member row of `g`: its online character and place (hidden by
    /// its privacy), else its last played character.
    fn member_row(&self, g: &GuildRow, m: &StoredMember, hidden: bool) -> GuildMember {
        let mut w = self.member_who(
            m.account,
            &m.account_name,
            m.last_char.as_deref(),
            Some(&g.tag),
        );
        let (mut at, _) = self.presence.place_of(AccountId(m.account));
        if hidden && at != place::OFFLINE {
            at = place::HIDDEN;
            w.flags &= !who_flags::IN_GAME;
        }
        GuildMember {
            who: w,
            place: at,
            rank: m.rank.as_u8(),
            joined_ms: wire_ms(m.joined_ms),
            last_seen_ms: m.last_seen.map_or(0, |s| wire_ms(s.saturating_mul(1000))),
        }
    }

    /// The member accounts of `guild` (none if it is gone).
    async fn member_ids(&self, guild: i64) -> Vec<i64> {
        self.accounts
            .store(move |db| {
                Ok(db
                    .guilds()
                    .members(guild)?
                    .into_iter()
                    .map(|m| m.account)
                    .collect())
            })
            .await
            .unwrap_or_default()
    }

    /// `GuildCreate`: the new guild's id.
    async fn guild_create(&self, who: &Member, tag: &str, name: &str) -> Result<u64, Denied> {
        let a = who.account.0;
        lock(&self.guild_limits).may_create(a, Instant::now())?;
        let (tag, name) = (tag.to_owned(), name.to_owned());
        let now = self.clock.now();
        let id = self
            .accounts
            .store(move |db| db.guilds().create(a, &tag, &name, now))
            .await?;
        lock(&self.guild_limits).created(a, Instant::now());
        self.log_line(&format!("{}: created guild {id}", who.account));
        self.guild_changed(who.account).await;
        Ok(wire_id(id))
    }

    /// `GuildInvite` of `target` (a character or `*account`).
    async fn guild_invite(&self, who: &Member, target: &str) -> Result<(), Denied> {
        let a = who.account.0;
        let now = self.clock.now();
        let max = self.cfg.guild_max_members;
        let target = target.to_owned();
        let found = self
            .accounts
            .store(move |db| {
                let peer = db.find_player(&target)?.ok_or(Refusal::UnknownPlayer)?;
                Ok((peer.clone(), db.last_character(peer.account)?))
            })
            .await?;
        let (peer, peer_last) = found;
        if peer.account == a {
            return Err(Refusal::UnknownPlayer.into());
        }
        lock(&self.guild_limits).invite(a, Instant::now())?;
        let t = peer.account;
        let (g, members) = self
            .accounts
            .store(move |db| {
                let id = db.guilds().invite(a, t, now, max)?;
                let g = db.guilds().get(id)?.ok_or(Refusal::NotInGuild)?;
                let members: Vec<i64> = db
                    .guilds()
                    .members(id)?
                    .into_iter()
                    .map(|m| m.account)
                    .collect();
                Ok((g, members))
            })
            .await?;
        self.guild_changed(AccountId(t)).await;
        let about = peer
            .char_name
            .clone()
            .unwrap_or_else(|| self.char_name(t, peer_last.as_deref(), &peer.account_name));
        let by = self.asker_name(who).await;
        let mut to = members;
        to.push(t);
        self.guild_notice(guild_notice::INVITED_YOU, &g, &about, &by, &to);
        Ok(())
    }

    /// `GuildAnswer` to `guild`'s invite.
    async fn guild_answer(&self, who: &Member, guild: u64, accept: bool) -> Result<(), Denied> {
        let a = who.account.0;
        let g = i64::try_from(guild).map_err(|_| Denied::from(Refusal::InviteGone))?;
        let now = self.clock.now();
        let max = self.cfg.guild_max_members;
        let row = self
            .accounts
            .store(move |db| {
                db.guilds().answer(a, g, accept, now, max)?;
                Ok(db.guilds().get(g)?)
            })
            .await?;
        self.guild_changed(who.account).await;
        let Some(row) = row.filter(|_| accept) else {
            return Ok(());
        };
        let name = self.asker_name(who).await;
        let w = self.member_who(a, &who.account_name, Some(&name), Some(&row.tag));
        self.guild_event(wire_id(row.id), w, format!("{name} joined the guild."));
        let members = self.member_ids(row.id).await;
        self.guild_notice(guild_notice::JOINED, &row, &name, &name, &members);
        Ok(())
    }

    /// `GuildManage` op `op` with `arg`.
    async fn guild_manage(&self, who: &Member, op: u8, arg: &str) -> Result<(), Denied> {
        let a = who.account.0;
        let by = self.asker_name(who).await;
        // The guild as it was (its tag and name for the notices).
        let before = self
            .accounts
            .store(move |db| Ok(db.guilds().of(a)?))
            .await?
            .ok_or(Denied::from(Refusal::NotInGuild))?;
        match op {
            guild_op::LEAVE => {
                let left = self.accounts.store(move |db| db.guilds().leave(a)).await?;
                self.guild_changed(who.account).await;
                if left.disbanded {
                    self.guild_gone(left.guild);
                    self.log_line(&format!("{}: guild {} disbanded", who.account, left.guild));
                    self.guild_notice(guild_notice::DISBANDED, &before, &by, &by, &[a]);
                    return Ok(());
                }
                let w = self.member_who(a, &who.account_name, Some(&by), None);
                self.guild_event(wire_id(before.id), w, format!("{by} left the guild."));
                let mut to = self.member_ids(before.id).await;
                to.push(a);
                self.guild_notice(guild_notice::LEFT, &before, &by, &by, &to);
            }
            guild_op::DISBAND => {
                let tag = arg.to_owned();
                let (g, members) = self
                    .accounts
                    .store(move |db| db.guilds().disband(a, &tag))
                    .await?;
                for m in &members {
                    self.guild_changed(AccountId(*m)).await;
                }
                self.guild_gone(g);
                self.log_line(&format!("{}: guild {g} disbanded", who.account));
                self.guild_notice(guild_notice::DISBANDED, &before, &by, &by, &members);
            }
            guild_op::MOTD => {
                let motd = arg.trim().to_owned();
                let m = motd.clone();
                self.accounts
                    .store(move |db| db.guilds().set_motd(a, &m))
                    .await?;
                let w = self.member_who(a, &who.account_name, Some(&by), Some(&before.tag));
                let text = if motd.is_empty() {
                    format!("{by} cleared the message of the day.")
                } else {
                    format!("Message of the day: {motd}")
                };
                self.guild_event(wire_id(before.id), w, text);
                let mut after = before.clone();
                after.motd = motd;
                let to = self.member_ids(before.id).await;
                self.guild_notice(guild_notice::MOTD, &after, &by, &by, &to);
            }
            // Kick, promote, demote, make leader (the decoder refuses
            // other ops).
            _ => self.guild_rank_op(who, op, arg, &before, &by).await?,
        }
        Ok(())
    }

    /// The ops on another member: kick, promote, demote, make leader.
    async fn guild_rank_op(
        &self,
        who: &Member,
        op: u8,
        arg: &str,
        g: &GuildRow,
        by: &str,
    ) -> Result<(), Denied> {
        let a = who.account.0;
        let target = arg.to_owned();
        let (peer, peer_last) = self
            .accounts
            .store(move |db| {
                let peer = db.find_player(&target)?.ok_or(Refusal::UnknownPlayer)?;
                let last = db.last_character(peer.account)?;
                Ok((peer, last))
            })
            .await?;
        let t = peer.account;
        if t == a {
            return Err(Refusal::GuildRank.into());
        }
        let new_rank = self
            .accounts
            .store(move |db| {
                let gs = db.guilds();
                match op {
                    guild_op::KICK => gs.kick(a, t).map(|_| None),
                    guild_op::PROMOTE => gs.promote(a, t).map(|_| Some(Rank::Officer)),
                    guild_op::DEMOTE => gs.demote(a, t).map(|_| Some(Rank::Member)),
                    _ => gs.make_leader(a, t).map(|_| Some(Rank::Leader)),
                }
            })
            .await?;
        // The target first: a kicked member is out of the room before the
        // line is said.
        self.guild_changed(AccountId(t)).await;
        if op == guild_op::MAKE_LEADER {
            self.guild_changed(who.account).await;
        }
        let about = peer
            .char_name
            .clone()
            .unwrap_or_else(|| self.char_name(t, peer_last.as_deref(), &peer.account_name));
        let tag = new_rank.map(|_| g.tag.as_str());
        let w = self.member_who(t, &peer.account_name, Some(&about), tag);
        let (what, text) = match (op, new_rank) {
            (guild_op::KICK, _) => (guild_notice::KICKED, format!("{about} was kicked by {by}.")),
            (guild_op::MAKE_LEADER, _) => (
                guild_notice::LEADER_CHANGED,
                format!("{about} now leads the guild."),
            ),
            (_, Some(r)) => {
                let what = if op == guild_op::PROMOTE {
                    guild_notice::PROMOTED
                } else {
                    guild_notice::DEMOTED
                };
                (
                    what,
                    format!("{about} is now {} {}.", article(r), rank_name(r)),
                )
            }
            _ => unreachable!("only a kick has no rank"),
        };
        self.guild_event(wire_id(g.id), w, text);
        let mut to = self.member_ids(g.id).await;
        if op == guild_op::KICK {
            to.push(t);
        }
        self.guild_notice(what, g, &about, by, &to);
        Ok(())
    }
}

fn article(r: Rank) -> &'static str {
    match r {
        Rank::Officer => "an",
        _ => "a",
    }
}

/// One guild request (kinds 0xF0-0xF7).
pub(super) async fn handle(s: &Social, who: &Member, msg: ToRealm) {
    let (req, kind) = (msg.req, msg.kind());
    let r = match msg.ask {
        Ask::GuildGet { what, tag } => s.guild_get(who, req, what, &tag).await,
        Ask::GuildCreate { tag, name } => match s.guild_create(who, &tag, &name).await {
            Ok(id) => {
                let mut done = Done::ok(req, kind);
                done.id = Some(id);
                return who.out.send(social::FromRealm::Done(done).body());
            }
            Err(e) => Err(e),
        },
        Ask::GuildInvite { who: target } => {
            let r = s.guild_invite(who, &target).await;
            return Social::done(&who.out, req, kind, r);
        }
        Ask::GuildAnswer { guild, accept } => {
            let r = s.guild_answer(who, guild, accept).await;
            return Social::done(&who.out, req, kind, r);
        }
        Ask::GuildManage { op, arg } => {
            let r = s.guild_manage(who, op, &arg).await;
            return Social::done(&who.out, req, kind, r);
        }
        // The friends' kinds go to `friends.rs`.
        Ask::FriendsGet { .. } | Ask::FriendEdit(_) => {
            return who.out.error(kind, ErrorCode::NOT_SUPPORTED, 0, "");
        }
    };
    // `GuildGet` answers with its data; a refusal with `SocialDone`.
    if let Err(e) = r {
        Social::done(&who.out, req, kind, Err(e));
    }
}
