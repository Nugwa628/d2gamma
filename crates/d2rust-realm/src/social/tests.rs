use std::net::IpAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use d2_accounts::hash::HashCost;
use d2_accounts::{AccountsConfig, NewChar};
use d2_net::msg::account::CharSummary;
use d2_net::msg::guild as g;
use d2_net::msg::social::place;
use d2_net::msg::ErrorCode;
use tokio::sync::mpsc;

use super::*;
use crate::session::ControlOut;

fn temp(test: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join("d2rust-realm-social")
        .join(format!("{test}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A service on a fresh database with accounts `names` (each with a
/// character of the same name and "char" after it); their ids.
async fn service(test: &str, names: &[&str]) -> (Arc<Social>, Vec<AccountId>) {
    service_with(test, names, SocialConfig::default()).await
}

/// [`service`] with the config `cfg`.
async fn service_with(
    test: &str,
    names: &[&str],
    cfg: SocialConfig,
) -> (Arc<Social>, Vec<AccountId>) {
    let dir = temp(test);
    let accounts = Accounts::open_with(AccountsConfig {
        cost: HashCost::Test,
        fail_delay: Duration::ZERO,
        ..AccountsConfig::new(&dir)
    })
    .unwrap();
    let mut ids = Vec::new();
    for (i, n) in names.iter().enumerate() {
        let ip = IpAddr::from([10, 0, 0, i as u8 + 1]);
        let a = accounts.register(n, "correct horse", ip).await.unwrap().id;
        accounts
            .create_character(
                a,
                NewChar {
                    name: format!("{n}char"),
                    class: 1,
                    hardcore: false,
                    expansion: true,
                    progression: 0,
                },
            )
            .await
            .unwrap();
        ids.push(a);
    }
    let log = Arc::new(ServerLog::open_named(&dir, "realm.log").unwrap());
    let s = Social::new(cfg, accounts, "d2rust", "D2Gamma", false, log);
    (s, ids)
}

fn summary(name: &str) -> CharSummary {
    CharSummary {
        name: name.into(),
        class: 1,
        level: 12,
        status: d2_formats::d2s::status::EXPANSION | d2_formats::d2s::status::HARDCORE,
        difficulties: 1,
        look: Vec::new(),
        last_played: 0,
        season: 0,
    }
}

fn member(
    session: SessionId,
    account: AccountId,
    name: &str,
) -> (Member, mpsc::UnboundedReceiver<ControlOut>) {
    let (out, rx) = ControlTx::new(session);
    let m = Member {
        session,
        account,
        account_name: name.into(),
        character: Some(summary(&format!("{name}char"))),
        out,
    };
    (m, rx)
}

/// Every body queued for a session.
fn bodies(rx: &mut mpsc::UnboundedReceiver<ControlOut>) -> Vec<Vec<u8>> {
    let mut v = Vec::new();
    while let Ok(ControlOut::Body(b)) = rx.try_recv() {
        v.push(b);
    }
    v
}

#[tokio::test]
async fn caps_list_the_friends_and_the_guilds() {
    let (s, _) = service("caps", &["ana"]).await;
    assert_eq!(s.caps(), vec!["social", "friends", "guilds"]);
}

#[tokio::test]
async fn presence_follows_sessions_and_games() {
    let (s, ids) = service("presence", &["ana", "bob"]).await;
    let (a1, _rx1) = member(1, ids[0], "ana");
    let (a2, _rx2) = member(2, ids[0], "ana");
    let p = s.presence();
    assert_eq!(p.place_of(ids[0]), (place::OFFLINE, None));
    s.seat(&a1);
    assert_eq!(p.place_of(ids[0]), (place::LOBBY, None));
    s.seat(&a2);
    assert_eq!(p.sessions_of(ids[0]).len(), 2);
    s.place(2, Some(9));
    assert_eq!(p.place_of(ids[0]), (place::IN_GAME, Some(9)));
    let who = p.get(2).unwrap().who(Some("NUG"), false);
    assert_eq!(who.char, "anachar");
    assert_eq!(who.tag.as_deref(), Some("NUG"));
    assert!(who.account.is_empty());
    use d2_net::msg::social::who_flags as f;
    assert_eq!(who.flags, f::HARDCORE | f::EXPANSION | f::IN_GAME);
    // The same seat again is no change; a new character is.
    assert!(p.seat(&a1).is_none());
    let mut other = a1.clone();
    other.character = None;
    assert!(p.seat(&other).is_some());
    s.leave(2);
    assert_eq!(p.place_of(ids[0]), (place::LOBBY, None));
    s.leave(1);
    assert!(!p.online(ids[0]));
    assert_eq!(p.send_to(ids[0], b"x"), 0);
    assert!(p.is_empty());
}

use d2_net::msg::lobby::LobbyEvent;
use d2_net::msg::social::{done_flags, presence_op, DmLine, ToRealm};

const ROOMS: u16 = room::bit(room::GLOBAL) | room::bit(room::MOD) | room::bit(room::GUILD);

async fn ask(s: &Arc<Social>, m: &Member, req: u32, ask: Ask) {
    s.clone().social(m.clone(), ToRealm { req, ask }).await;
}

/// The social messages queued for a session.
fn msgs(rx: &mut mpsc::UnboundedReceiver<ControlOut>) -> Vec<FromRealm> {
    bodies(rx)
        .iter()
        .filter_map(|b| FromRealm::parse(b).ok())
        .collect()
}

/// The `SocialDone` of request `req` among `v`.
fn done(v: &[FromRealm], req: u32) -> Done {
    v.iter()
        .find_map(|m| match m {
            FromRealm::Done(d) if d.req == req => Some(d.clone()),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no done for {req}: {v:?}"))
}

fn code(v: &[FromRealm], req: u32) -> ErrorCode {
    done(v, req).code
}

fn lines(v: &[FromRealm]) -> Vec<(u8, u8, String, String)> {
    v.iter()
        .filter_map(|m| match m {
            FromRealm::RoomLine(l) => Some((l.room, l.what, l.who.char.clone(), l.text.clone())),
            _ => None,
        })
        .collect()
}

fn dm_lines(v: &[FromRealm]) -> Vec<DmLine> {
    v.iter()
        .filter_map(|m| match m {
            FromRealm::DmLine(l) => Some(l.clone()),
            _ => None,
        })
        .collect()
}

fn states(v: &[FromRealm]) -> Vec<SocialState> {
    v.iter()
        .filter_map(|m| match m {
            FromRealm::State(s) => Some(s.clone()),
            _ => None,
        })
        .collect()
}

fn say(r: u8, text: &str) -> Ask {
    Ask::Say {
        room: r,
        emote: false,
        text: text.into(),
    }
}

fn whisper(to: &str, text: &str) -> Ask {
    Ask::Whisper {
        to: to.into(),
        text: text.into(),
    }
}

/// A seated session that sent its hello (all three rooms), its queue
/// emptied.
async fn online(
    s: &Arc<Social>,
    session: SessionId,
    account: AccountId,
    name: &str,
) -> (Member, mpsc::UnboundedReceiver<ControlOut>) {
    let (m, mut rx) = member(session, account, name);
    s.seat(&m);
    ask(s, &m, 1, Ask::Hello { rooms: ROOMS }).await;
    bodies(&mut rx);
    (m, rx)
}

#[tokio::test]
async fn hello_answers_the_state_then_the_backlog() {
    let (s, ids) = service("hello", &["ana", "bob"]).await;
    let (bob, mut brx) = online(&s, 2, ids[1], "bob").await;
    ask(&s, &bob, 2, say(room::MOD, "hi all")).await;
    assert!(done(&msgs(&mut brx), 2).is_ok());
    let (ana, mut arx) = member(1, ids[0], "ana");
    s.seat(&ana);
    ask(&s, &ana, 1, Ask::Hello { rooms: ROOMS }).await;
    let got = msgs(&mut arx);
    let FromRealm::State(st) = &got[0] else {
        panic!("{got:?}")
    };
    // No guild: Global and Mod only.
    assert_eq!(st.req, 1);
    assert_eq!(st.rooms, room::bit(room::GLOBAL) | room::bit(room::MOD));
    assert_eq!(
        (st.mod_id.as_str(), st.mod_label.as_str()),
        ("d2rust", "D2Gamma")
    );
    assert_eq!(st.limits.say_lines, 5);
    assert_eq!(st.muted_until_ms, None);
    // The backlog: bob's joins and line, then ana's own joins.
    let l = lines(&got);
    assert!(
        l.contains(&(
            room::MOD,
            line_what::SAID,
            "bobchar".into(),
            "hi all".into()
        )),
        "{l:?}"
    );
    assert!(l.contains(&(
        room::MOD,
        line_what::JOINED,
        "anachar".into(),
        String::new()
    )));
    // Bob saw ana join both rooms.
    let b = lines(&msgs(&mut brx));
    assert_eq!(
        b,
        vec![
            (
                room::GLOBAL,
                line_what::JOINED,
                "anachar".into(),
                String::new()
            ),
            (
                room::MOD,
                line_what::JOINED,
                "anachar".into(),
                String::new()
            ),
        ]
    );
    assert!(s.caps().contains(&"social"));
}

#[tokio::test]
async fn say_follows_the_room_rules() {
    let (s, ids) = service("say", &["ana", "bob"]).await;
    let (ana, mut arx) = online(&s, 1, ids[0], "ana").await;
    let (bob, mut brx) = online(&s, 2, ids[1], "bob").await;
    bodies(&mut arx);
    ask(&s, &ana, 2, say(room::MOD, "  hello bob  ")).await;
    let a = msgs(&mut arx);
    assert!(done(&a, 2).is_ok());
    let want = (
        room::MOD,
        line_what::SAID,
        "anachar".to_owned(),
        "hello bob".to_owned(),
    );
    assert_eq!(lines(&a), vec![want.clone()]);
    assert_eq!(lines(&msgs(&mut brx)), vec![want]);
    for (req, ask_, want) in [
        (3, say(room::GUILD, "guild?"), ErrorCode::NOT_IN_GUILD),
        (4, say(room::GAME, "game?"), ErrorCode::ROOM_UNAVAILABLE),
        (5, say(room::MOD, "   "), ErrorCode(134)),
        (6, say(room::MOD, "bell\u{7}"), ErrorCode(134)),
        (7, say(room::MOD, "hello bob"), ErrorCode::RATE_LIMITED),
    ] {
        ask(&s, &ana, req, ask_).await;
        assert_eq!(code(&msgs(&mut arx), req), want, "{req}");
    }
    assert!(lines(&msgs(&mut brx)).is_empty());
    // Global: 2 lines per 30 s.
    ask(&s, &ana, 8, say(room::GLOBAL, "one")).await;
    ask(&s, &ana, 9, say(room::GLOBAL, "two")).await;
    ask(&s, &ana, 10, say(room::GLOBAL, "three")).await;
    let a = msgs(&mut arx);
    assert!(done(&a, 9).is_ok());
    let d = done(&a, 10);
    assert_eq!(d.code, ErrorCode::RATE_LIMITED);
    assert!(d.wait_s >= 1, "{d:?}");
    // Bob heard the two Global lines.
    let heard = lines(&msgs(&mut brx));
    assert_eq!(heard.len(), 2, "{heard:?}");
    drop(bob);
}

#[tokio::test]
async fn a_session_without_a_character_cant_speak() {
    let (s, ids) = service("nochar", &["ana"]).await;
    let (mut m, mut rx) = member(1, ids[0], "ana");
    m.character = None;
    s.seat(&m);
    ask(&s, &m, 1, Ask::Hello { rooms: ROOMS }).await;
    ask(&s, &m, 2, say(room::MOD, "hi")).await;
    let v = msgs(&mut rx);
    assert_eq!(states(&v).len(), 1);
    assert_eq!(code(&v, 2), ErrorCode::NO_CHAR_SELECTED);
}

#[tokio::test]
async fn the_old_chat_line_goes_to_the_mod_room_and_back() {
    let (s, ids) = service("legacy", &["ana", "bob"]).await;
    let (ana, mut arx) = online(&s, 1, ids[0], "ana").await;
    // Bob's client never sends a hello.
    let (bob, mut brx) = member(2, ids[1], "bob");
    s.seat(&bob);
    s.legacy_say(2, "old client here").await.unwrap();
    assert_eq!(
        lines(&msgs(&mut arx)),
        vec![(
            room::MOD,
            line_what::SAID,
            "bobchar".into(),
            "old client here".into()
        )]
    );
    let old = |rx: &mut mpsc::UnboundedReceiver<ControlOut>| -> Vec<(String, String)> {
        bodies(rx)
            .iter()
            .filter_map(|b| match LobbyEvent::parse(b) {
                Ok(LobbyEvent::ChatLine { from, text, .. }) => Some((from, text)),
                _ => None,
            })
            .collect()
    };
    assert_eq!(
        old(&mut brx),
        vec![("bobchar".into(), "old client here".into())]
    );
    ask(&s, &ana, 2, say(room::MOD, "new client")).await;
    ask(&s, &ana, 3, say(room::GLOBAL, "only in Global")).await;
    assert_eq!(old(&mut brx), vec![("anachar".into(), "new client".into())]);
    assert_eq!(
        s.legacy_say(2, "\u{1b}[31m").await.unwrap_err().code,
        ErrorCode(134)
    );
}

#[tokio::test]
async fn a_whisper_reaches_every_session_of_both() {
    let (s, ids) = service("whisper", &["ana", "bob"]).await;
    let (ana, mut arx) = online(&s, 1, ids[0], "ana").await;
    let (_a2, mut a2rx) = online(&s, 3, ids[0], "ana").await;
    let (_bob, mut brx) = online(&s, 2, ids[1], "bob").await;
    ask(&s, &ana, 2, whisper("BOBCHAR", "psst")).await;
    let a = msgs(&mut arx);
    let d = done(&a, 2);
    assert!(d.is_ok(), "{d:?}");
    assert_eq!(d.flags & done_flags::STORED, 0);
    let mine = dm_lines(&a);
    assert_eq!(mine.len(), 1);
    assert!(mine[0].outgoing && !mine[0].stored);
    assert_eq!(Some(mine[0].dm), d.id);
    assert_eq!(
        (mine[0].peer_account.as_str(), mine[0].peer_char.as_str()),
        ("bob", "bobchar")
    );
    assert_eq!(dm_lines(&msgs(&mut a2rx)), mine);
    let b = msgs(&mut brx);
    let theirs = dm_lines(&b);
    assert_eq!(theirs.len(), 1);
    assert!(!theirs[0].outgoing);
    assert_eq!(
        (
            theirs[0].peer_account.as_str(),
            theirs[0].peer_char.as_str()
        ),
        ("ana", "anachar")
    );
    // Bob's unread count follows.
    assert_eq!(states(&b).last().unwrap().dm_unread, 1);
    // By account name; to oneself or nobody: UnknownPlayer.
    ask(&s, &ana, 3, whisper("*Bob", "again")).await;
    ask(&s, &ana, 4, whisper("anachar", "me")).await;
    ask(&s, &ana, 5, whisper("nobody", "hm")).await;
    ask(&s, &ana, 6, whisper("bobchar", "")).await;
    let a = msgs(&mut arx);
    assert!(done(&a, 3).is_ok());
    assert_eq!(code(&a, 4), ErrorCode(127));
    assert_eq!(code(&a, 5), ErrorCode(127));
    assert_eq!(code(&a, 6), ErrorCode(134));
}

#[tokio::test]
async fn an_offline_whisper_waits_for_the_next_hello() {
    let (s, ids) = service("offline", &["ana", "bob"]).await;
    let (ana, mut arx) = online(&s, 1, ids[0], "ana").await;
    ask(&s, &ana, 2, whisper("bobchar", "see you later")).await;
    let d = done(&msgs(&mut arx), 2);
    assert!(d.is_ok());
    assert_eq!(d.flags & done_flags::STORED, done_flags::STORED);
    let (bob, mut brx) = member(2, ids[1], "bob");
    s.seat(&bob);
    ask(&s, &bob, 1, Ask::Hello { rooms: ROOMS }).await;
    let b = msgs(&mut brx);
    assert_eq!(states(&b)[0].dm_unread, 1);
    assert_eq!(states(&b)[0].dm_conversations_unread, 1);
    let waited = dm_lines(&b);
    assert_eq!(waited.len(), 1);
    assert!(waited[0].stored && !waited[0].outgoing);
    assert_eq!(waited[0].text, "see you later");
    // Delivered once: a second hello doesn't push it again.
    ask(&s, &bob, 2, Ask::Hello { rooms: ROOMS }).await;
    assert!(dm_lines(&msgs(&mut brx)).is_empty());
    // The list, the history and the read mark.
    ask(&s, &bob, 3, Ask::DmConversations { page: 0 }).await;
    let b = msgs(&mut brx);
    let rows: Vec<_> = b
        .iter()
        .filter_map(|m| match m {
            FromRealm::DmConversationRow { req: 3, row } => Some(row.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(rows.len(), 1);
    assert_eq!((rows[0].peer_account.as_str(), rows[0].unread), ("ana", 1));
    assert_eq!(rows[0].place, place::LOBBY);
    assert!(matches!(b.last(), Some(FromRealm::ListEnd { req: 3 })));
    ask(
        &s,
        &bob,
        4,
        Ask::DmHistory {
            peer: "*ana".into(),
            before_dm: 0,
            rows: 50,
        },
    )
    .await;
    let h = dm_lines(&msgs(&mut brx));
    assert_eq!(h.len(), 1);
    assert_eq!(h[0].req, 4);
    ask(
        &s,
        &bob,
        5,
        Ask::DmRead {
            peer_account: "ana".into(),
            up_to_dm: h[0].dm,
        },
    )
    .await;
    let b = msgs(&mut brx);
    assert!(done(&b, 5).is_ok());
    assert_eq!(states(&b).last().unwrap().dm_unread, 0);
}

#[tokio::test]
async fn dms_go_after_the_keep_days() {
    let (s, ids) = service("sweep", &["ana", "bob"]).await;
    let (ana, mut arx) = online(&s, 1, ids[0], "ana").await;
    ask(&s, &ana, 2, whisper("bobchar", "old news")).await;
    assert!(done(&msgs(&mut arx), 2).is_ok());
    s.advance_clock(29 * 86_400_000).await.unwrap();
    assert!(s
        .advance_clock(2 * 86_400_000)
        .await
        .unwrap()
        .contains("swept 1 DM"));
    let (bob, mut brx) = member(2, ids[1], "bob");
    s.seat(&bob);
    ask(&s, &bob, 1, Ask::Hello { rooms: ROOMS }).await;
    let b = msgs(&mut brx);
    assert_eq!(states(&b)[0].dm_unread, 0);
    assert!(dm_lines(&b).is_empty());
}

#[tokio::test]
async fn a_muted_account_reads_but_cant_speak() {
    let (s, ids) = service("mute", &["ana", "bob"]).await;
    let (ana, mut arx) = online(&s, 1, ids[0], "ana").await;
    let (bob, _brx) = online(&s, 2, ids[1], "bob").await;
    s.mute("ANA", 5, "spam").await.unwrap();
    let st = states(&msgs(&mut arx));
    assert!(st.last().unwrap().muted_until_ms.is_some());
    ask(&s, &ana, 2, say(room::MOD, "let me")).await;
    ask(&s, &ana, 3, whisper("bobchar", "let me")).await;
    let a = msgs(&mut arx);
    for req in [2, 3] {
        let d = done(&a, req);
        assert_eq!(d.code, ErrorCode::MUTED);
        assert!((299..=300).contains(&d.wait_s), "{d:?}");
    }
    ask(&s, &bob, 3, say(room::MOD, "you still read this")).await;
    assert_eq!(lines(&msgs(&mut arx)).len(), 1);
    s.unmute("ana").await.unwrap();
    assert!(s.unmute("ana").await.is_err());
    assert!(s.mute("nobody", 5, "").await.is_err());
    ask(&s, &ana, 4, say(room::MOD, "back")).await;
    assert!(done(&msgs(&mut arx), 4).is_ok());
}

#[tokio::test]
async fn an_ignored_account_is_not_heard() {
    let (s, ids) = service("ignore", &["ana", "bob"]).await;
    let (ana, mut arx) = online(&s, 1, ids[0], "ana").await;
    let (bob, mut brx) = online(&s, 2, ids[1], "bob").await;
    let (a, b) = (ids[0].0, ids[1].0);
    s.accounts()
        .store(move |db| db.friends().ignore(a, b, 0, 200))
        .await
        .unwrap();
    s.reload(ids[0]).await;
    bodies(&mut arx);
    ask(&s, &bob, 3, say(room::MOD, "can you hear me")).await;
    ask(&s, &bob, 4, whisper("anachar", "hello?")).await;
    let bm = msgs(&mut brx);
    assert_eq!(lines(&bm).len(), 1);
    assert_eq!(code(&bm, 4), ErrorCode(129));
    assert!(msgs(&mut arx).is_empty());
    // Ana still speaks to bob.
    ask(&s, &ana, 2, say(room::MOD, "I hear nothing")).await;
    assert_eq!(lines(&msgs(&mut brx)).len(), 1);
}

#[tokio::test]
async fn the_guild_room_is_its_members() {
    let (s, ids) = service("guild", &["ana", "bob"]).await;
    let a = ids[0].0;
    s.accounts()
        .store(move |db| db.guilds().create(a, "NUG", "Nugwa", 0))
        .await
        .unwrap();
    let (ana, mut arx) = member(1, ids[0], "ana");
    s.seat(&ana);
    ask(&s, &ana, 1, Ask::Hello { rooms: ROOMS }).await;
    let st = states(&msgs(&mut arx));
    assert_eq!(st[0].rooms, ROOMS);
    assert_eq!(st[0].guild.as_ref().map(|g| g.tag.as_str()), Some("NUG"));
    let (_bob, mut brx) = online(&s, 2, ids[1], "bob").await;
    bodies(&mut arx);
    ask(&s, &ana, 2, say(room::GUILD, "guild only")).await;
    let l = lines(&msgs(&mut arx));
    assert_eq!(
        l,
        vec![(
            room::GUILD,
            line_what::SAID,
            "anachar".into(),
            "guild only".into()
        )]
    );
    assert!(lines(&msgs(&mut brx)).iter().all(|l| l.0 != room::GUILD));
    // The guild's tag shows on her lines in the other rooms.
    ask(&s, &ana, 3, say(room::MOD, "tagged")).await;
    let b = msgs(&mut brx);
    let line = b
        .iter()
        .find_map(|m| match m {
            FromRealm::RoomLine(l) if l.what == line_what::SAID => Some(l.clone()),
            _ => None,
        })
        .unwrap();
    assert_eq!(line.who.tag.as_deref(), Some("NUG"));
    assert!(line.who.account.is_empty());
    // Leaving the guild (L5b calls `guild_changed`) takes the room away.
    s.accounts()
        .store(move |db| db.guilds().disband(a, "NUG"))
        .await
        .unwrap();
    s.guild_changed(ids[0]).await;
    let st = states(&msgs(&mut arx));
    assert_eq!(st.last().unwrap().rooms, ROOMS & !room::bit(room::GUILD));
    ask(&s, &ana, 4, say(room::GUILD, "anyone?")).await;
    assert_eq!(code(&msgs(&mut arx), 4), ErrorCode::NOT_IN_GUILD);
}

#[tokio::test]
async fn a_watched_user_list_stays_live() {
    let (s, ids) = service("users", &["ana", "bob"]).await;
    let (ana, mut arx) = online(&s, 1, ids[0], "ana").await;
    let (_bob, _brx) = online(&s, 2, ids[1], "bob").await;
    bodies(&mut arx);
    ask(
        &s,
        &ana,
        2,
        Ask::UserList {
            room: room::MOD,
            watch: true,
        },
    )
    .await;
    let v = msgs(&mut arx);
    let FromRealm::ListBegin(head) = &v[0] else {
        panic!("{v:?}")
    };
    assert_eq!(
        (head.req, head.what, head.rows),
        (2, social::kind::USER_LIST, 2)
    );
    let rows: Vec<(String, u8)> = v
        .iter()
        .filter_map(|m| match m {
            FromRealm::UserRow { who, place, .. } => Some((who.char.clone(), *place)),
            _ => None,
        })
        .collect();
    assert_eq!(
        rows,
        vec![
            ("anachar".into(), place::LOBBY),
            ("bobchar".into(), place::LOBBY)
        ]
    );
    // Bob enters a game, then ends his session.
    s.place(2, Some(7));
    s.leave(2);
    let ops: Vec<(u8, String, u8, u16)> = msgs(&mut arx)
        .iter()
        .filter_map(|m| match m {
            FromRealm::RoomPresence(p) => Some((p.op, p.who.char.clone(), p.place, p.count)),
            _ => None,
        })
        .collect();
    assert_eq!(
        ops,
        vec![
            (presence_op::CHANGED, "bobchar".into(), place::IN_GAME, 2),
            (presence_op::LEFT, "bobchar".into(), place::IN_GAME, 1),
        ]
    );
    // A room it isn't in: refused.
    ask(
        &s,
        &ana,
        3,
        Ask::UserList {
            room: room::GUILD,
            watch: false,
        },
    )
    .await;
    assert_eq!(code(&msgs(&mut arx), 3), ErrorCode::ROOM_UNAVAILABLE);
}

#[tokio::test]
async fn a_notice_reaches_global() {
    let (s, ids) = service("notice", &["ana"]).await;
    let (_ana, mut arx) = online(&s, 1, ids[0], "ana").await;
    s.notice("Realm restart in 5 minutes").unwrap();
    assert_eq!(
        lines(&msgs(&mut arx)),
        vec![(
            room::GLOBAL,
            line_what::REALM_NOTICE,
            String::new(),
            "Realm restart in 5 minutes".into()
        )]
    );
    assert!(s.notice(" ").is_err());
}

/// A reload reads the account from the store between its check that the
/// account is online and the rooms taking the view: an account that logged
/// out meanwhile (a friend going offline reloads every follower, often as
/// they log out too) must keep no view.
#[tokio::test]
async fn a_view_read_as_the_account_left_is_not_kept() {
    let (s, ids) = service("view-left", &["ana", "bob"]).await;
    let (ana, _arx) = online(&s, 1, ids[0], "ana").await;
    let (_bob, _brx) = online(&s, 2, ids[1], "bob").await;
    let read = s.load(ids[0]).await.unwrap();
    s.leave(ana.session);
    assert!(lock(&s.rooms).view(ids[0].0).is_none());
    lock(&s.rooms).set_view(&s.presence, ids[0].0, read.view, 0);
    assert!(lock(&s.rooms).view(ids[0].0).is_none());
    assert!(s.stats().contains(" views=1 "), "{}", s.stats());
}

#[path = "guilds_tests.rs"]
mod guilds;

#[path = "friends_tests.rs"]
mod friends_tests;
