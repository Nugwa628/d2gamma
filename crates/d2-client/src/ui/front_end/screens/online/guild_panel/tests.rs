use super::*;
use crate::ui::front_end::screens::online::fake::social::session;
use crate::ui::front_end::screens::online::fake::{FakeServer, SharedFake};
use crate::ui::front_end::screens::online::link::lobby_code;
use crate::ui::front_end::screens::online::lobby::PANEL;

fn adv(font: u16, t: &[u16]) -> i32 {
    let per = if font == FONT8 { 5 } else { 8 };
    t.len() as i32 * per
}

/// The realm's answers fed and the panel rebuilt (which fetches the guild
/// again after a change) until nothing more comes.
fn settle(online: &OnlineHandle, p: &mut GuildPanel) {
    for _ in 0..4 {
        // Taken first, as the lobby does: the panel reads the link too.
        let events = online.borrow_mut().take_events();
        for ev in &events {
            p.feed(ev);
        }
        p.controls(PANEL, &mut Vec::new());
    }
}

fn opened(set: impl FnOnce(&mut FakeServer)) -> (SharedFake, OnlineHandle, GuildPanel) {
    let (fake, online) = session(set);
    let mut p = GuildPanel::new(online.clone());
    p.open();
    settle(&online, &mut p);
    std::mem::take(&mut fake.0.borrow_mut().seen);
    (fake, online, p)
}

/// The session's rank in the seeded guild `<NUG>`.
fn ranked(rank: u8) -> impl FnOnce(&mut FakeServer) {
    move |f| f.social.my_guild = Some((1, rank))
}

fn no_guild(f: &mut FakeServer) {
    f.social.my_guild = None;
}

fn sent(fake: &SharedFake) -> Vec<Request> {
    std::mem::take(&mut fake.0.borrow_mut().seen)
}

fn labels(p: &GuildPanel) -> Vec<&'static str> {
    p.chips.iter().map(|c| c.label).collect()
}

fn click(p: &mut GuildPanel, id: &str) -> Option<PanelOut> {
    let at = p
        .ui_elements()
        .into_iter()
        .find(|e| e.id == id)
        .unwrap_or_else(|| panic!("no {id}"))
        .at;
    p.mouse(at, 0)
}

fn type_in(p: &mut GuildPanel, field: u32, text: &str) {
    p.action(field);
    for c in text.chars() {
        assert!(p.char(c));
    }
}

fn texts(p: &mut GuildPanel) -> Vec<String> {
    p.overlay(PANEL, &adv, 0)
        .into_iter()
        .filter_map(|i| match i {
            DrawItem::Text { text, .. } => Some(text),
            _ => None,
        })
        .collect()
}

fn note(p: &GuildPanel) -> &str {
    p.note.as_ref().map_or("", |n| n.0.as_str())
}

fn member_names(p: &GuildPanel) -> Vec<&str> {
    p.members.iter().map(|m| shown(&m.who)).collect()
}

/// The rank decides the buttons: a member whispers and leaves; an officer
/// also kicks, invites and sets the motd; the leader has them all.
#[test]
fn the_rank_decides_the_buttons() {
    let (_, _, p) = opened(ranked(guild_rank::MEMBER));
    assert_eq!(labels(&p), ["WHISPER", "LEAVE"]);
    let (_, _, p) = opened(ranked(guild_rank::OFFICER));
    assert_eq!(labels(&p), ["WHISPER", "KICK", "INVITE", "MOTD", "LEAVE"]);
    let (_, _, p) = opened(ranked(guild_rank::LEADER));
    assert_eq!(
        labels(&p),
        ["WHISPER", "KICK", "PROMOTE", "DEMOTE", "LEADER", "INVITE", "MOTD", "LEAVE", "DISBAND"]
    );
    let (_, _, p) = opened(no_guild);
    assert_eq!(labels(&p), ["ACCEPT", "DECLINE", "CREATE"]);
}

/// A button the rank doesn't show does nothing, even pressed by id.
#[test]
fn a_hidden_button_does_nothing() {
    let (fake, _, mut p) = opened(ranked(guild_rank::MEMBER));
    click(&mut p, "online_lobby.guild_member(Ashka)");
    for id in [
        act::KICK,
        act::PROMOTE,
        act::INVITE,
        act::MOTD,
        act::DISBAND,
    ] {
        assert_eq!(p.action(id), None);
    }
    assert!(sent(&fake).is_empty());
}

/// The heading, the roster (rank, then name) with ranks and places, and the
/// motd under the panel.
#[test]
fn the_roster() {
    let (_, _, mut p) = opened(|_| {});
    assert_eq!(member_names(&p), ["Werwer", "Kaern", "Ashka"]);
    let t = texts(&mut p);
    for want in [
        "<NUG> Nugwa",
        "3 members - you: Leader",
        "Leader",
        "Officer",
        "Member",
        "in the lobby",
        "seen 26 h ago",
        "Motd: Baal runs every evening, ask Kaern.",
    ] {
        assert!(t.iter().any(|s| s == want), "{want} in {t:?}");
    }
}

/// WHISPER starts a whisper to the selected member.
#[test]
fn whisper() {
    let (_, _, mut p) = opened(ranked(guild_rank::MEMBER));
    assert_eq!(p.action(act::WHISPER), Some(PanelOut::Rebuild));
    assert_eq!(note(&p), "Choose a member in the list first.");
    click(&mut p, "online_lobby.guild_member(Kaern)");
    assert_eq!(
        p.action(act::WHISPER),
        Some(PanelOut::Whisper("Kaern".into()))
    );
}

/// KICK asks to be pressed again, then kicks; nobody kicks themselves.
#[test]
fn kick() {
    let (fake, online, mut p) = opened(|_| {});
    click(&mut p, "online_lobby.guild_member(Werwer)");
    p.action(act::KICK);
    assert_eq!(note(&p), "Choose another member: that one is you.");
    click(&mut p, "online_lobby.guild_member(Ashka)");
    p.action(act::KICK);
    assert_eq!(note(&p), "Press KICK again to kick Ashka.");
    assert!(sent(&fake).is_empty());
    p.action(act::KICK);
    assert_eq!(
        sent(&fake),
        [Request::GuildManage(GuildOp::Kick("*ashka".into()))]
    );
    settle(&online, &mut p);
    assert_eq!(note(&p), "Ashka was kicked.");
    assert_eq!(member_names(&p), ["Werwer", "Kaern"]);
}

/// Another button between the two presses disarms KICK.
#[test]
fn kick_needs_two_presses_in_a_row() {
    let (fake, _, mut p) = opened(|_| {});
    click(&mut p, "online_lobby.guild_member(Ashka)");
    p.action(act::KICK);
    p.action(act::WHISPER);
    p.action(act::KICK);
    assert!(sent(&fake).is_empty());
}

/// An officer can't kick another officer: the realm says why.
#[test]
fn an_officer_cant_kick_an_officer() {
    let (fake, online, mut p) = opened(ranked(guild_rank::OFFICER));
    fake.0.borrow_mut().social.guilds[0].members[1].1 = guild_rank::OFFICER;
    p.open();
    settle(&online, &mut p);
    click(&mut p, "online_lobby.guild_member(Ashka)");
    p.action(act::KICK);
    p.action(act::KICK);
    settle(&online, &mut p);
    let want = failure_text(&Failure {
        code: lobby_code::GUILD_RANK,
        wait_s: 0,
        text: String::new(),
    });
    assert_eq!(p.note, Some((want, RED)));
}

/// PROMOTE and DEMOTE move a member between member and officer.
#[test]
fn promote_and_demote() {
    let (fake, online, mut p) = opened(|_| {});
    click(&mut p, "online_lobby.guild_member(Kaern)");
    p.action(act::PROMOTE);
    assert_eq!(note(&p), "Kaern is already an officer.");
    click(&mut p, "online_lobby.guild_member(Ashka)");
    p.action(act::DEMOTE);
    assert_eq!(note(&p), "Ashka is not an officer.");
    assert!(sent(&fake).is_empty());
    p.action(act::PROMOTE);
    assert_eq!(
        sent(&fake),
        [Request::GuildManage(GuildOp::Promote("*ashka".into()))]
    );
    settle(&online, &mut p);
    assert_eq!(note(&p), "Ashka is an officer now.");
    let rank =
        |p: &GuildPanel, n: &str| p.members.iter().find(|m| shown(&m.who) == n).unwrap().rank;
    assert_eq!(rank(&p, "Ashka"), guild_rank::OFFICER);
    sent(&fake);
    click(&mut p, "online_lobby.guild_member(Kaern)");
    p.action(act::DEMOTE);
    assert_eq!(
        sent(&fake),
        [Request::GuildManage(GuildOp::Demote("*kaern".into()))]
    );
    settle(&online, &mut p);
    assert_eq!(rank(&p, "Kaern"), guild_rank::MEMBER);
}

/// LEADER, pressed twice, hands the guild over; the player is an officer
/// after and the buttons follow.
#[test]
fn make_leader() {
    let (fake, online, mut p) = opened(|_| {});
    click(&mut p, "online_lobby.guild_member(Kaern)");
    p.action(act::LEADER);
    assert!(note(&p).starts_with("Press LEADER again"));
    p.action(act::LEADER);
    assert_eq!(
        sent(&fake),
        [Request::GuildManage(GuildOp::MakeLeader("*kaern".into()))]
    );
    settle(&online, &mut p);
    assert_eq!(p.rank(), guild_rank::OFFICER);
    assert_eq!(labels(&p), ["WHISPER", "KICK", "INVITE", "MOTD", "LEAVE"]);
}

/// INVITE sends the name in the box; an empty box asks for one.
#[test]
fn invite() {
    let (fake, online, mut p) = opened(ranked(guild_rank::OFFICER));
    p.action(act::INVITE);
    assert_eq!(
        note(&p),
        "Type the character to invite in the box, then press INVITE."
    );
    assert_eq!(p.focus, Some(Focus::Text));
    type_in(&mut p, act::TEXT, "Brisa");
    p.action(act::INVITE);
    assert_eq!(
        sent(&fake),
        [Request::GuildInvite {
            who: "Brisa".into()
        }]
    );
    settle(&online, &mut p);
    assert_eq!(note(&p), "Brisa is invited to <NUG>.");
    assert!(p.text.text.is_empty());
    sent(&fake);
    // Enter in the box invites too.
    type_in(&mut p, act::TEXT, "Corvus");
    assert_eq!(p.enter(), Some(PanelOut::Rebuild));
    assert_eq!(
        sent(&fake),
        [Request::GuildInvite {
            who: "Corvus".into()
        }]
    );
}

/// Enter in a member's box keeps the key (no INVITE for the rank, no join
/// or chat line either).
#[test]
fn enter_in_a_members_box() {
    let (fake, _, mut p) = opened(ranked(guild_rank::MEMBER));
    type_in(&mut p, act::TEXT, "Corvus");
    assert_eq!(p.enter(), Some(PanelOut::Rebuild));
    assert!(sent(&fake).is_empty());
    // Not in the box: the lobby's own.
    p.focus = None;
    assert_eq!(p.enter(), None);
    // A press outside the panel leaves the box.
    type_in(&mut p, act::TEXT, "C");
    p.blur();
    assert!(!p.char('x'));
    assert_eq!(p.enter(), None);
}

/// MOTD with an empty box takes today's message to edit; with text, it
/// saves it.
#[test]
fn motd() {
    let (fake, online, mut p) = opened(ranked(guild_rank::OFFICER));
    p.action(act::MOTD);
    assert_eq!(p.text.text, "Baal runs every evening, ask Kaern.");
    assert!(sent(&fake).is_empty());
    p.text.text = "Cows on Sunday.".into();
    p.action(act::MOTD);
    assert_eq!(
        sent(&fake),
        [Request::GuildManage(GuildOp::Motd(
            "Cows on Sunday.".into()
        ))]
    );
    settle(&online, &mut p);
    assert_eq!(p.info.as_ref().unwrap().motd, "Cows on Sunday.");
    assert!(texts(&mut p).contains(&"Motd: Cows on Sunday.".to_string()));
}

/// LEAVE, pressed twice: a member leaves; the leader of others is told to
/// hand the guild over first.
#[test]
fn leave() {
    let (fake, online, mut p) = opened(ranked(guild_rank::MEMBER));
    p.action(act::LEAVE);
    assert_eq!(note(&p), "Press LEAVE again to leave <NUG>.");
    p.action(act::LEAVE);
    assert_eq!(sent(&fake), [Request::GuildManage(GuildOp::Leave)]);
    settle(&online, &mut p);
    assert!(!p.in_guild());
    assert_eq!(note(&p), "You left <NUG>.");
    // The view without a guild: the invite and the create form.
    assert_eq!(labels(&p), ["ACCEPT", "DECLINE", "CREATE"]);

    let (_, online, mut p) = opened(|_| {});
    p.action(act::LEAVE);
    p.action(act::LEAVE);
    settle(&online, &mut p);
    assert_eq!(
        p.note,
        Some((
            "Your guild rank doesn't allow that. Make another member leader first.".into(),
            RED
        ))
    );
    assert!(p.in_guild());
}

/// DISBAND needs the guild's tag in the box.
#[test]
fn disband() {
    let (fake, online, mut p) = opened(|_| {});
    p.action(act::DISBAND);
    assert_eq!(
        note(&p),
        "Type the guild's tag (NUG) in the box, then press DISBAND."
    );
    type_in(&mut p, act::TEXT, "PK");
    p.action(act::DISBAND);
    assert!(sent(&fake).is_empty());
    p.text.text = "nug".into();
    p.action(act::DISBAND);
    assert_eq!(
        sent(&fake),
        [Request::GuildManage(GuildOp::Disband("nug".into()))]
    );
    settle(&online, &mut p);
    assert_eq!(note(&p), "<NUG> is disbanded.");
    assert!(!p.in_guild());
    assert!(fake.0.borrow().social.guilds.iter().all(|g| g.tag != "NUG"));
}

/// Without a guild: the invites, ACCEPT and DECLINE.
#[test]
fn accept_and_decline_an_invite() {
    let (fake, online, mut p) = opened(no_guild);
    assert_eq!(p.invites.len(), 1);
    assert_eq!(p.invites_waiting(), 1);
    let t = texts(&mut p);
    assert!(t.contains(&"<PK> Player Killers".to_string()), "{t:?}");
    // The only invite needs no choosing.
    p.action(act::ACCEPT);
    assert_eq!(
        sent(&fake),
        [Request::GuildAnswer {
            guild: 2,
            accept: true
        }]
    );
    settle(&online, &mut p);
    assert_eq!(note(&p), "You joined <PK> Player Killers.");
    assert!(p.in_guild());
    assert_eq!(p.rank(), guild_rank::MEMBER);
    assert_eq!(labels(&p), ["WHISPER", "LEAVE"]);

    let (fake, online, mut p) = opened(no_guild);
    click(&mut p, "online_lobby.guild_invite(PK)");
    assert_eq!(p.selected_invite, Some(2));
    p.action(act::DECLINE);
    assert_eq!(
        sent(&fake),
        [Request::GuildAnswer {
            guild: 2,
            accept: false
        }]
    );
    settle(&online, &mut p);
    assert!(p.invites.is_empty());
    assert!(!p.in_guild());
    assert_eq!(labels(&p), ["CREATE"]);
}

/// CREATE founds a guild with the typed tag and name; the realm's refusal
/// (a taken tag) says why.
#[test]
fn create_a_guild() {
    let (fake, online, mut p) = opened(no_guild);
    p.action(act::CREATE);
    assert_eq!(p.focus, Some(Focus::Tag));
    assert!(sent(&fake).is_empty());
    type_in(&mut p, act::TAG, "PK");
    type_in(&mut p, act::NAME, "Other Killers");
    p.action(act::CREATE);
    settle(&online, &mut p);
    let want = failure_text(&Failure {
        code: lobby_code::GUILD_TAG_TAKEN,
        wait_s: 0,
        text: String::new(),
    });
    assert_eq!(p.note, Some((want, RED)));
    sent(&fake);
    p.tag.text = "NEW".into();
    // Enter in a box creates.
    p.focus = Some(Focus::Name);
    p.enter();
    assert_eq!(
        sent(&fake),
        [Request::GuildCreate {
            tag: "NEW".into(),
            name: "Other Killers".into()
        }]
    );
    settle(&online, &mut p);
    assert_eq!(note(&p), "<NEW> Other Killers is founded: you lead it.");
    assert_eq!(p.rank(), guild_rank::LEADER);
    assert_eq!(member_names(&p), ["Werwer"]);
}

/// The boxes take what their rule allows: tags letters and digits (4 at
/// most), names letters, digits and spaces.
#[test]
fn the_boxes_filter() {
    let (_, _, mut p) = opened(no_guild);
    type_in(&mut p, act::TAG, "A-B");
    assert_eq!(p.tag.text, "AB");
    p.tag.text = "ABCD".into();
    p.char('E');
    assert_eq!(p.tag.text, "ABCD");
    type_in(&mut p, act::NAME, "Order of 9!");
    assert_eq!(p.name.text, "Order of 9");
}

/// A guild notice refetches the guild and fills an empty line.
#[test]
fn a_notice_refetches() {
    let (fake, online, mut p) = opened(|_| {});
    p.feed(&Event::GuildNotice(GuildNotice {
        what: guild_notice::JOINED,
        guild: 1,
        tag: "NUG".into(),
        name: "Nugwa".into(),
        who: "Brisa".into(),
        by: String::new(),
        at_ms: 0,
    }));
    assert_eq!(note(&p), "Brisa joined <NUG>.");
    p.controls(PANEL, &mut Vec::new());
    assert_eq!(
        sent(&fake),
        [
            Request::GuildGet(GuildQuery::Mine),
            Request::GuildGet(GuildQuery::Invites)
        ]
    );
    settle(&online, &mut p);
}

/// A realm without guilds says so.
#[test]
fn a_realm_without_guilds() {
    let (_, _, mut p) = opened(|f| f.social_off = true);
    assert!(p.unsupported);
    assert!(p.chips.is_empty());
    assert_eq!(texts(&mut p), ["This realm has no guilds yet."]);
}

/// Everything fits the panel in both views, the chips don't overlap, and
/// the texts stay inside.
#[test]
fn both_views_fit() {
    type Setup = Box<dyn FnOnce(&mut FakeServer)>;
    let views: [Setup; 2] = [Box::new(ranked(guild_rank::LEADER)), Box::new(no_guild)];
    for set in views {
        let (_, _, mut p) = opened(set);
        let mut v = Vec::new();
        p.controls(PANEL, &mut v);
        let bottom = PANEL.y + i32::from(PANEL.h);
        for c in &v {
            assert!(
                c.x >= PANEL.x
                    && c.x + i32::from(c.w) <= PANEL.x + i32::from(PANEL.w)
                    && c.y - i32::from(c.h) >= PANEL.y + 18
                    && c.y <= bottom,
                "{c:?}"
            );
        }
        let chips: Vec<Rect> = p.chips.iter().map(|c| c.at).collect();
        let boxes = if p.in_guild() {
            vec![p.field_rect(TEXT_BOTTOM)]
        } else {
            vec![p.field_rect(TAG_BOTTOM), p.field_rect(NAME_BOTTOM)]
        };
        let all: Vec<Rect> = chips.iter().copied().chain(boxes).collect();
        for (i, a) in all.iter().enumerate() {
            for b in &all[i + 1..] {
                let apart = a.right() <= i64::from(b.x)
                    || b.right() <= i64::from(a.x)
                    || a.bottom() <= i64::from(b.y)
                    || b.bottom() <= i64::from(a.y);
                assert!(apart, "{a:?} {b:?}");
            }
        }
        for item in p.overlay(PANEL, &adv, 0) {
            if let DrawItem::Text { at, text, font, .. } = item {
                let w = adv(font, &text.encode_utf16().collect::<Vec<u16>>());
                assert!(
                    at.x >= PANEL.x && at.x + w <= PANEL.x + i32::from(PANEL.w),
                    "{text:?} at {at:?}"
                );
            }
        }
    }
}
