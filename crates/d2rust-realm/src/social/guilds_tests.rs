//! The guilds (lobby contract 4) against a fresh store: the rules, the
//! Guild room's membership, the notices and the limits.

use d2_net::msg::guild::{guild_get, guild_notice, guild_op, rank, GuildInfo, GuildNotice};

use super::*;

/// What a session got: the social messages and the guild ones.
#[derive(Debug, Default)]
struct Got {
    social: Vec<FromRealm>,
    guild: Vec<g::FromRealm>,
}

fn got(rx: &mut mpsc::UnboundedReceiver<ControlOut>) -> Got {
    let mut out = Got::default();
    for b in bodies(rx) {
        if let Ok(m) = FromRealm::parse(&b) {
            out.social.push(m);
        } else if let Ok(m) = g::FromRealm::parse(&b) {
            out.guild.push(m);
        } else {
            panic!("neither social nor guild: {b:?}");
        }
    }
    out
}

impl Got {
    fn code(&self, req: u32) -> ErrorCode {
        code(&self.social, req)
    }

    fn notices(&self) -> Vec<GuildNotice> {
        self.guild
            .iter()
            .filter_map(|m| match m {
                g::FromRealm::GuildNotice(n) => Some(n.clone()),
                _ => None,
            })
            .collect()
    }

    fn infos(&self) -> Vec<GuildInfo> {
        self.guild
            .iter()
            .filter_map(|m| match m {
                g::FromRealm::GuildInfo { info, .. } => Some(info.clone()),
                _ => None,
            })
            .collect()
    }

    /// The members' character names and ranks.
    fn members(&self) -> Vec<(String, u8, String)> {
        self.guild
            .iter()
            .filter_map(|m| match m {
                g::FromRealm::GuildMember { member, .. } => Some((
                    member.who.char.clone(),
                    member.rank,
                    member.who.tag.clone().unwrap_or_default(),
                )),
                _ => None,
            })
            .collect()
    }

    fn guild_lines(&self) -> Vec<(u8, String)> {
        lines(&self.social)
            .into_iter()
            .filter(|l| l.0 == room::GUILD)
            .map(|l| (l.1, l.3))
            .collect()
    }
}

async fn gask(s: &Arc<Social>, m: &Member, req: u32, ask: g::Ask) {
    s.clone().guild(m.clone(), g::ToRealm { req, ask }).await;
}

fn create(tag: &str, name: &str) -> g::Ask {
    g::Ask::GuildCreate {
        tag: tag.into(),
        name: name.into(),
    }
}

fn invite(who: &str) -> g::Ask {
    g::Ask::GuildInvite { who: who.into() }
}

fn manage(op: u8, arg: &str) -> g::Ask {
    g::Ask::GuildManage {
        op,
        arg: arg.into(),
    }
}

fn mine() -> g::Ask {
    g::Ask::GuildGet {
        what: guild_get::MINE,
        tag: String::new(),
    }
}

fn by_tag(tag: &str) -> g::Ask {
    g::Ask::GuildGet {
        what: guild_get::BY_TAG,
        tag: tag.into(),
    }
}

/// `m` creates guild `tag`; its id.
async fn made(
    s: &Arc<Social>,
    m: &Member,
    rx: &mut mpsc::UnboundedReceiver<ControlOut>,
    tag: &str,
) -> u64 {
    gask(s, m, 50, create(tag, "Nugwa")).await;
    let d = done(&got(rx).social, 50);
    assert!(d.is_ok(), "{d:?}");
    d.id.expect("the guild's id")
}

/// `m` accepts guild `id`'s invite.
async fn accepts(
    s: &Arc<Social>,
    m: &Member,
    rx: &mut mpsc::UnboundedReceiver<ControlOut>,
    id: u64,
) {
    gask(
        s,
        m,
        51,
        g::Ask::GuildAnswer {
            guild: id,
            accept: true,
        },
    )
    .await;
    assert_eq!(got(rx).code(51), ErrorCode(0));
}

#[tokio::test]
async fn guild_lifecycle_follows_the_rules() {
    let (s, ids) = service("guild-life", &["ana", "bob", "cyd"]).await;
    let (ana, mut arx) = online(&s, 1, ids[0], "ana").await;
    let (bob, mut brx) = online(&s, 2, ids[1], "bob").await;
    let (cyd, mut crx) = online(&s, 3, ids[2], "cyd").await;
    assert!(s.caps().contains(&"guilds"));

    let id = made(&s, &ana, &mut arx, "NUG").await;
    // The tag is taken in any case; a second guild per account refused.
    gask(&s, &bob, 2, create("nug", "Other Guild")).await;
    assert_eq!(got(&mut brx).code(2), ErrorCode::GUILD_TAG_TAKEN);
    gask(&s, &ana, 3, create("XY", "Second Guild")).await;
    let refused = got(&mut arx).code(3);
    assert!(
        [ErrorCode::ALREADY_IN_GUILD, ErrorCode::RATE_LIMITED].contains(&refused),
        "{refused:?}"
    );

    // Invites: by character and by account; the invitee is told.
    gask(&s, &ana, 4, invite("bobchar")).await;
    assert_eq!(got(&mut arx).code(4), ErrorCode(0));
    let n = got(&mut brx).notices();
    assert_eq!(n.len(), 1, "{n:?}");
    assert_eq!(
        (n[0].what, n[0].guild, n[0].tag.as_str(), n[0].who.as_str()),
        (guild_notice::INVITED_YOU, id, "NUG", "bobchar")
    );
    assert_eq!(n[0].by, "anachar");
    gask(&s, &ana, 5, invite("*cyd")).await;
    assert_eq!(got(&mut arx).code(5), ErrorCode(0));
    gask(&s, &ana, 6, invite("anachar")).await;
    assert_eq!(got(&mut arx).code(6), ErrorCode::UNKNOWN_PLAYER);
    gask(&s, &ana, 7, invite("nobody")).await;
    assert_eq!(got(&mut arx).code(7), ErrorCode::UNKNOWN_PLAYER);
    // The invitee's state counts the invite.
    let c = got(&mut crx);
    assert_eq!(states(&c.social).last().unwrap().guild_invites, 1);
    gask(
        &s,
        &cyd,
        8,
        g::Ask::GuildGet {
            what: guild_get::INVITES,
            tag: String::new(),
        },
    )
    .await;
    let c = got(&mut crx);
    assert_eq!(c.infos().len(), 1);
    assert_eq!(c.infos()[0].my_rank, rank::NONE);

    accepts(&s, &bob, &mut brx, id).await;
    accepts(&s, &cyd, &mut crx, id).await;
    let a = got(&mut arx);
    let joined: Vec<u8> = a.notices().iter().map(|n| n.what).collect();
    assert_eq!(joined, vec![guild_notice::JOINED, guild_notice::JOINED]);

    // The roster: the leader first, with the tag.
    gask(&s, &ana, 9, mine()).await;
    let a = got(&mut arx);
    let info = &a.infos()[0];
    assert_eq!(
        (info.members, info.my_rank, info.leader_char.as_str()),
        (3, rank::LEADER, "anachar")
    );
    assert_eq!(
        a.members(),
        vec![
            ("anachar".into(), rank::LEADER, "NUG".into()),
            ("bobchar".into(), rank::MEMBER, "NUG".into()),
            ("cydchar".into(), rank::MEMBER, "NUG".into()),
        ]
    );

    // Ranks: only the leader promotes; an officer kicks members only.
    gask(&s, &bob, 10, manage(guild_op::PROMOTE, "cydchar")).await;
    assert_eq!(got(&mut brx).code(10), ErrorCode::GUILD_RANK);
    gask(&s, &ana, 11, manage(guild_op::PROMOTE, "bobchar")).await;
    assert_eq!(got(&mut arx).code(11), ErrorCode(0));
    let b = got(&mut brx);
    assert_eq!(
        states(&b.social)
            .last()
            .unwrap()
            .guild
            .as_ref()
            .unwrap()
            .my_rank,
        rank::OFFICER
    );
    gask(&s, &bob, 12, manage(guild_op::KICK, "anachar")).await;
    assert_eq!(got(&mut brx).code(12), ErrorCode::GUILD_RANK);
    gask(&s, &bob, 13, manage(guild_op::KICK, "cydchar")).await;
    assert_eq!(got(&mut brx).code(13), ErrorCode(0));
    let c = got(&mut crx);
    assert_eq!(c.notices().last().unwrap().what, guild_notice::KICKED);
    assert!(states(&c.social).last().unwrap().guild.is_none());
    gask(&s, &bob, 14, manage(guild_op::KICK, "cydchar")).await;
    assert_eq!(got(&mut brx).code(14), ErrorCode::UNKNOWN_PLAYER);

    // The leader can't leave while others remain; after handing over it can.
    gask(&s, &ana, 15, manage(guild_op::LEAVE, "")).await;
    assert_eq!(got(&mut arx).code(15), ErrorCode::GUILD_RANK);
    gask(&s, &ana, 16, manage(guild_op::MAKE_LEADER, "bobchar")).await;
    assert_eq!(got(&mut arx).code(16), ErrorCode(0));
    gask(&s, &ana, 17, manage(guild_op::LEAVE, "")).await;
    let a = got(&mut arx);
    assert_eq!(a.code(17), ErrorCode(0));
    assert_eq!(a.notices().last().unwrap().what, guild_notice::LEFT);
    let b = got(&mut brx);
    let whats: Vec<u8> = b.notices().iter().map(|n| n.what).collect();
    assert_eq!(
        whats,
        vec![guild_notice::LEADER_CHANGED, guild_notice::LEFT]
    );

    // The last member leaving disbands; the tag is free at once.
    gask(&s, &bob, 18, manage(guild_op::LEAVE, "")).await;
    let b = got(&mut brx);
    assert_eq!(b.code(18), ErrorCode(0));
    assert_eq!(b.notices().last().unwrap().what, guild_notice::DISBANDED);
    gask(&s, &cyd, 19, by_tag("NUG")).await;
    assert_eq!(got(&mut crx).code(19), ErrorCode::BAD_GUILD_NAME);
    let again = made(&s, &cyd, &mut crx, "nug").await;
    gask(&s, &bob, 20, by_tag("NUG")).await;
    let b = got(&mut brx);
    assert_eq!(b.infos()[0].guild, again);
    assert_eq!(b.infos()[0].tag, "nug");
}

#[tokio::test]
async fn the_guild_room_follows_the_members() {
    let (s, ids) = service("guild-room", &["ana", "bob", "cyd"]).await;
    let (ana, mut arx) = online(&s, 1, ids[0], "ana").await;
    let (bob, mut brx) = online(&s, 2, ids[1], "bob").await;
    let (_cyd, mut crx) = online(&s, 3, ids[2], "cyd").await;
    gask(&s, &ana, 50, create("NUG", "Nugwa")).await;
    let a = got(&mut arx);
    let id = done(&a.social, 50).id.unwrap();
    let st = states(&a.social);
    let st = st.last().unwrap();
    assert_ne!(st.rooms & room::bit(room::GUILD), 0);
    assert_eq!(st.guild.as_ref().unwrap().tag, "NUG");
    gask(&s, &ana, 2, invite("bobchar")).await;
    accepts(&s, &bob, &mut brx, id).await;
    // Ana hears bob join; the event line is in the room.
    let a = got(&mut arx);
    assert!(
        a.guild_lines()
            .contains(&(line_what::GUILD_EVENT, "bobchar joined the guild.".into())),
        "{:?}",
        a.guild_lines()
    );
    // Only members hear the room; the tag is on the line.
    ask(&s, &ana, 3, say(room::GUILD, "guild only")).await;
    let b = got(&mut brx);
    let heard = b
        .social
        .iter()
        .find_map(|m| match m {
            FromRealm::RoomLine(l) if l.what == line_what::SAID => Some(l.clone()),
            _ => None,
        })
        .expect("bob hears the guild line");
    assert_eq!(
        (heard.room, heard.who.tag.as_deref(), heard.text.as_str()),
        (room::GUILD, Some("NUG"), "guild only")
    );
    assert!(got(&mut crx).guild_lines().is_empty());
    // Global lines carry the tag too.
    ask(&s, &bob, 4, say(room::GLOBAL, "hi realm")).await;
    let c = got(&mut crx);
    let l = c
        .social
        .iter()
        .find_map(|m| match m {
            FromRealm::RoomLine(l) if l.what == line_what::SAID => Some(l.clone()),
            _ => None,
        })
        .unwrap();
    assert_eq!(l.who.tag.as_deref(), Some("NUG"));

    // A kicked member stops hearing it at once (not even its kick line).
    got(&mut arx);
    gask(&s, &ana, 5, manage(guild_op::KICK, "bobchar")).await;
    let b = got(&mut brx);
    assert!(b.guild_lines().is_empty(), "{:?}", b.guild_lines());
    let st = states(&b.social);
    let st = st.last().unwrap();
    assert_eq!(st.rooms & room::bit(room::GUILD), 0);
    assert!(st.guild.is_none());
    let a = got(&mut arx);
    assert!(a.guild_lines().contains(&(
        line_what::GUILD_EVENT,
        "bobchar was kicked by anachar.".into()
    )));
    ask(&s, &bob, 6, say(room::GUILD, "still here?")).await;
    assert_eq!(got(&mut brx).code(6), ErrorCode::NOT_IN_GUILD);
    ask(&s, &ana, 7, say(room::GUILD, "gone")).await;
    assert!(got(&mut brx).guild_lines().is_empty());

    // The motd: an event line and a notice.
    gask(&s, &ana, 8, manage(guild_op::MOTD, "raid at nine")).await;
    let a = got(&mut arx);
    assert_eq!(a.code(8), ErrorCode(0));
    assert_eq!(a.notices()[0].what, guild_notice::MOTD);
    assert!(a.guild_lines().contains(&(
        line_what::GUILD_EVENT,
        "Message of the day: raid at nine".into()
    )));
}

#[tokio::test]
async fn a_disbanded_guilds_room_is_forgotten() {
    let (s, ids) = service("guild-gone", &["ana", "bob"]).await;
    let (ana, mut arx) = online(&s, 1, ids[0], "ana").await;
    let (bob, mut brx) = online(&s, 2, ids[1], "bob").await;
    let id = made(&s, &ana, &mut arx, "NUG").await;
    ask(&s, &ana, 2, say(room::GUILD, "old secret")).await;
    gask(&s, &ana, 3, manage(guild_op::DISBAND, "XX")).await;
    assert_eq!(got(&mut arx).code(3), ErrorCode::BAD_GUILD_NAME);
    gask(&s, &bob, 4, manage(guild_op::DISBAND, "NUG")).await;
    assert_eq!(got(&mut brx).code(4), ErrorCode::NOT_IN_GUILD);
    gask(&s, &ana, 5, manage(guild_op::DISBAND, "nug")).await;
    let a = got(&mut arx);
    assert_eq!(a.code(5), ErrorCode(0));
    assert_eq!(a.notices()[0].what, guild_notice::DISBANDED);
    assert!(states(&a.social).last().unwrap().guild.is_none());
    // A new guild (which may get the same number) has none of its lines.
    let again = made(&s, &bob, &mut brx, "NEW").await;
    let (bob2, mut b2rx) = member(3, ids[1], "bob");
    s.seat(&bob2);
    ask(&s, &bob2, 1, Ask::Hello { rooms: ROOMS }).await;
    let b2 = got(&mut b2rx);
    assert!(
        !b2.guild_lines().iter().any(|l| l.1 == "old secret"),
        "{again} {id}: {:?}",
        b2.guild_lines()
    );
}

#[tokio::test]
async fn invites_expire_and_creates_are_limited() {
    let (s, ids) = service("guild-clock", &["ana", "bob"]).await;
    let (ana, mut arx) = online(&s, 1, ids[0], "ana").await;
    let (bob, mut brx) = online(&s, 2, ids[1], "bob").await;
    let id = made(&s, &ana, &mut arx, "NUG").await;
    gask(&s, &ana, 2, invite("bobchar")).await;
    assert_eq!(got(&mut arx).code(2), ErrorCode(0));
    s.advance_clock(8 * 24 * 3600 * 1000).await.unwrap();
    gask(
        &s,
        &bob,
        3,
        g::Ask::GuildAnswer {
            guild: id,
            accept: true,
        },
    )
    .await;
    assert_eq!(got(&mut brx).code(3), ErrorCode::INVITE_GONE);
    // One create a day: after a disband the next is refused for a while.
    gask(&s, &ana, 4, manage(guild_op::LEAVE, "")).await;
    assert_eq!(got(&mut arx).code(4), ErrorCode(0));
    gask(&s, &ana, 5, create("AB", "Next Guild")).await;
    let d = done(&got(&mut arx).social, 5);
    assert_eq!(d.code, ErrorCode::RATE_LIMITED);
    assert!(d.wait_s > 0);
    // Bad tags and names say which rule.
    gask(&s, &bob, 6, create("W", "Nugwa")).await;
    let d = done(&got(&mut brx).social, 6);
    assert_eq!(d.code, ErrorCode::BAD_GUILD_NAME);
    assert!(d.text.contains("2 to 4"), "{}", d.text);
}
