//! The friends' rules (lobby contract 5): one-sided and mutual friends,
//! the notices, the privacy, the lists' limits.

use d2_net::msg::guild::{friend_notice, FriendEdit, FriendGame, FriendNotice, FriendRow};

use super::*;
use crate::social::friends::FriendGames;

/// `SocialDone`'s code for done.
const DONE: ErrorCode = ErrorCode(0);

/// The lobby's games as the friends see them: game 9, "Cows".
struct Games;

impl FriendGames for Games {
    fn friend_game(&self, game: u32, asker: Option<&CharSummary>) -> Option<FriendGame> {
        (game == 9).then(|| FriendGame {
            game_id: 9,
            game_name: "Cows".into(),
            joinable: if asker.is_some() {
                0
            } else {
                ErrorCode::NO_CHAR_SELECTED.0
            },
        })
    }
}

/// A service whose friends see [`Games`] (kept alive by the returned
/// handle).
async fn friends_service(
    test: &str,
    names: &[&str],
    cfg: SocialConfig,
) -> (Arc<Social>, Vec<AccountId>, Arc<dyn FriendGames>) {
    let (s, ids) = service_with(test, names, cfg).await;
    let games: Arc<dyn FriendGames> = Arc::new(Games);
    s.friends().set_games(Arc::downgrade(&games));
    (s, ids, games)
}

/// One message to a session: the chat's or the friends'.
#[derive(Debug)]
enum Msg {
    Social(FromRealm),
    Friends(g::FromRealm),
}

fn all(rx: &mut mpsc::UnboundedReceiver<ControlOut>) -> Vec<Msg> {
    bodies(rx)
        .iter()
        .filter_map(|b| match b[0] {
            k if d2_net::msg::kind::SOCIAL.contains(&k) => {
                FromRealm::parse(b).ok().map(Msg::Social)
            }
            _ => g::FromRealm::parse(b).ok().map(Msg::Friends),
        })
        .collect()
}

fn notices(v: &[Msg]) -> Vec<FriendNotice> {
    v.iter()
        .filter_map(|m| match m {
            Msg::Friends(g::FromRealm::FriendNotice(n)) => Some(n.clone()),
            _ => None,
        })
        .collect()
}

fn social_of(v: &[Msg]) -> Vec<FromRealm> {
    v.iter()
        .filter_map(|m| match m {
            Msg::Social(s) => Some(s.clone()),
            _ => None,
        })
        .collect()
}

/// Sends a friends request and waits for the notices it made.
async fn fr(s: &Arc<Social>, m: &Member, req: u32, ask: g::Ask) {
    s.clone().guild(m.clone(), g::ToRealm { req, ask }).await;
    s.friends().settled().await;
}

async fn edit(s: &Arc<Social>, m: &Member, req: u32, e: FriendEdit) -> ErrorCode {
    let (out, mut rx) = ControlTx::new(m.session);
    let probe = Member { out, ..m.clone() };
    fr(s, &probe, req, g::Ask::FriendEdit(e)).await;
    code(&social_of(&all(&mut rx)), req)
}

/// `FriendsGet` (`what`) of `m`, its rows.
async fn rows(s: &Arc<Social>, m: &Member, what: u8) -> Vec<FriendRow> {
    let (out, mut rx) = ControlTx::new(m.session);
    let probe = Member { out, ..m.clone() };
    fr(s, &probe, 50, g::Ask::FriendsGet { what }).await;
    let got = all(&mut rx);
    assert!(
        matches!(got.first(), Some(Msg::Social(FromRealm::ListBegin(h))) if h.req == 50 && h.what == g::kind::FRIENDS_GET),
        "{got:?}"
    );
    assert!(matches!(
        got.last(),
        Some(Msg::Social(FromRealm::ListEnd { req: 50 }))
    ));
    got.into_iter()
        .filter_map(|m| match m {
            Msg::Friends(g::FromRealm::FriendRow { req: 50, row }) => Some(row),
            _ => None,
        })
        .collect()
}

async fn friends_of(s: &Arc<Social>, m: &Member) -> Vec<FriendRow> {
    rows(s, m, g::friends_get::FRIENDS).await
}

fn add(who: &str) -> FriendEdit {
    FriendEdit::Add(who.into())
}

#[tokio::test]
async fn one_sided_friends_see_online_only() {
    let (s, ids, _g) = friends_service("one-sided", &["ana", "bob"], SocialConfig::default()).await;
    let (ana, mut arx) = online(&s, 1, ids[0], "ana").await;
    assert_eq!(edit(&s, &ana, 2, add("bobchar")).await, DONE);
    let r = friends_of(&s, &ana).await;
    assert_eq!(r.len(), 1);
    assert_eq!(
        (r[0].who.char.as_str(), r[0].who.account.as_str()),
        ("bobchar", "bob")
    );
    assert_eq!((r[0].place, r[0].mutual), (place::OFFLINE, false));
    assert_eq!(r[0].last_seen_ms, 0);
    bodies(&mut arx);

    // Bob comes online: Ana hears it, without the place.
    let (bob, _brx) = online(&s, 2, ids[1], "bob").await;
    s.friends().settled().await;
    let got = all(&mut arx);
    let n = notices(&got);
    assert_eq!(n.len(), 1, "{got:?}");
    assert_eq!(
        (n[0].what, n[0].place),
        (friend_notice::ONLINE, place::HIDDEN)
    );
    assert_eq!(n[0].who.char, "bobchar");
    assert!(n[0].game.is_none());
    assert_eq!(states(&social_of(&got)).last().unwrap().friends_online, 1);

    // His game stays his: no notice, the row says hidden.
    s.place(bob.session, Some(9));
    s.friends().settled().await;
    assert!(notices(&all(&mut arx)).is_empty());
    let r = friends_of(&s, &ana).await;
    assert_eq!((r[0].place, r[0].game.as_ref()), (place::HIDDEN, None));
    assert_eq!(r[0].who.flags & social::who_flags::IN_GAME, 0);
}

#[tokio::test]
async fn mutual_friends_see_the_lobby_the_game_and_last_seen() {
    let (s, ids, _g) = friends_service("mutual", &["ana", "bob"], SocialConfig::default()).await;
    let (ana, mut arx) = online(&s, 1, ids[0], "ana").await;
    let (bob, _brx) = online(&s, 2, ids[1], "bob").await;
    assert_eq!(edit(&s, &ana, 2, add("bobchar")).await, DONE);
    bodies(&mut arx);
    // Bob adds Ana back: she hears it with his place.
    assert_eq!(edit(&s, &bob, 2, add("*ana")).await, DONE);
    let n = notices(&all(&mut arx));
    assert_eq!(n.len(), 1);
    assert_eq!(
        (n[0].what, n[0].place),
        (friend_notice::ADDED_YOU, place::LOBBY)
    );
    let r = friends_of(&s, &ana).await;
    assert_eq!((r[0].place, r[0].mutual), (place::LOBBY, true));

    s.place(bob.session, Some(9));
    s.friends().settled().await;
    let n = notices(&all(&mut arx));
    assert_eq!(n.len(), 1);
    assert_eq!(
        (n[0].what, n[0].place),
        (friend_notice::ENTERED_GAME, place::IN_GAME)
    );
    let game = n[0].game.clone().unwrap();
    assert_eq!(
        (game.game_id, game.game_name.as_str(), game.joinable),
        (9, "Cows", 0)
    );
    let r = friends_of(&s, &ana).await;
    assert_eq!(r[0].place, place::IN_GAME);
    assert_eq!(r[0].game.as_ref().map(|g| g.game_id), Some(9));
    assert_ne!(r[0].who.flags & social::who_flags::IN_GAME, 0);

    s.place(bob.session, None);
    s.friends().settled().await;
    let n = notices(&all(&mut arx));
    assert_eq!(
        (n[0].what, n[0].place),
        (friend_notice::LEFT_GAME, place::LOBBY)
    );

    s.leave(bob.session);
    s.friends().settled().await;
    let got = all(&mut arx);
    let n = notices(&got);
    assert_eq!(
        (n[0].what, n[0].place),
        (friend_notice::OFFLINE, place::OFFLINE)
    );
    assert_eq!(states(&social_of(&got)).last().unwrap().friends_online, 0);
    let r = friends_of(&s, &ana).await;
    assert_eq!(r[0].place, place::OFFLINE);
    assert!(r[0].last_seen_ms > 0);
    // Offline, the row still names his character with its class and level.
    assert_eq!((r[0].who.char.as_str(), r[0].who.class), ("bobchar", 1));
}

#[tokio::test]
async fn show_where_nobody_hides_the_place_from_mutual_friends() {
    let (s, ids, _g) = friends_service("hidden", &["ana", "bob"], SocialConfig::default()).await;
    let (ana, mut arx) = online(&s, 1, ids[0], "ana").await;
    let (bob, _brx) = online(&s, 2, ids[1], "bob").await;
    edit(&s, &ana, 2, add("bobchar")).await;
    edit(&s, &bob, 2, add("anachar")).await;
    let privacy = FriendEdit::Privacy {
        whispers: social::whispers::EVERYONE,
        show_where: social::show_where::NOBODY,
    };
    assert_eq!(edit(&s, &bob, 3, privacy).await, DONE);
    bodies(&mut arx);
    s.place(bob.session, Some(9));
    s.friends().settled().await;
    assert!(notices(&all(&mut arx)).is_empty());
    let r = friends_of(&s, &ana).await;
    assert_eq!((r[0].place, r[0].mutual), (place::HIDDEN, true));
    assert!(r[0].game.is_none());
}

#[tokio::test]
async fn whispers_from_friends_only_block_strangers() {
    let (s, ids, _g) =
        friends_service("whispers", &["ana", "bob", "cid"], SocialConfig::default()).await;
    let (ana, mut arx) = online(&s, 1, ids[0], "ana").await;
    let (bob, mut brx) = online(&s, 2, ids[1], "bob").await;
    let (cid, mut crx) = online(&s, 3, ids[2], "cid").await;
    edit(&s, &bob, 2, add("anachar")).await;
    let privacy = FriendEdit::Privacy {
        whispers: social::whispers::FRIENDS_AND_GUILD,
        show_where: social::show_where::MUTUAL_FRIENDS,
    };
    assert_eq!(edit(&s, &bob, 3, privacy).await, DONE);
    let st = states(&msgs(&mut brx));
    assert_eq!(
        st.last().unwrap().whispers,
        social::whispers::FRIENDS_AND_GUILD
    );
    ask(&s, &cid, 2, whisper("bobchar", "hi")).await;
    assert_eq!(code(&msgs(&mut crx), 2), ErrorCode(129));
    ask(&s, &ana, 2, whisper("bobchar", "hi")).await;
    assert!(done(&msgs(&mut arx), 2).is_ok());
}

#[tokio::test]
async fn ignoring_takes_the_lines_away_until_unignored() {
    let (s, ids, _g) = friends_service("ignore", &["ana", "bob"], SocialConfig::default()).await;
    let (ana, mut arx) = online(&s, 1, ids[0], "ana").await;
    let (bob, mut brx) = online(&s, 2, ids[1], "bob").await;
    assert_eq!(
        edit(&s, &ana, 2, FriendEdit::Ignore("bobchar".into())).await,
        DONE
    );
    let r = rows(&s, &ana, g::friends_get::IGNORED).await;
    assert_eq!(r.len(), 1);
    assert_eq!((r[0].who.account.as_str(), r[0].mutual), ("bob", false));
    assert!(friends_of(&s, &ana).await.is_empty());
    bodies(&mut arx);
    ask(&s, &bob, 2, say(room::MOD, "hello?")).await;
    ask(&s, &bob, 3, whisper("anachar", "hello?")).await;
    assert_eq!(code(&msgs(&mut brx), 3), ErrorCode(129));
    assert!(lines(&msgs(&mut arx)).is_empty());
    assert_eq!(
        edit(&s, &ana, 3, FriendEdit::Unignore("bobchar".into())).await,
        DONE
    );
    bodies(&mut arx);
    ask(&s, &bob, 4, say(room::MOD, "now?")).await;
    assert_eq!(lines(&msgs(&mut arx)).len(), 1);
}

#[tokio::test]
async fn nobody_and_oneself_are_unknown_players() {
    let (s, ids, _g) = friends_service("unknown", &["ana"], SocialConfig::default()).await;
    let (ana, _arx) = online(&s, 1, ids[0], "ana").await;
    assert_eq!(edit(&s, &ana, 2, add("nobody")).await, ErrorCode(127));
    assert_eq!(edit(&s, &ana, 3, add("anachar")).await, ErrorCode(127));
    assert_eq!(edit(&s, &ana, 4, add("*ana")).await, ErrorCode(127));
    assert_eq!(
        edit(&s, &ana, 5, FriendEdit::Ignore("nobody".into())).await,
        ErrorCode(127)
    );
}

#[tokio::test]
async fn the_lists_are_capped() {
    let cfg = SocialConfig {
        friends_max: 2,
        ignore_max: 1,
        ..SocialConfig::default()
    };
    let (s, ids, _g) = friends_service("caps", &["ana", "bob", "cid", "dan"], cfg).await;
    let (ana, _arx) = online(&s, 1, ids[0], "ana").await;
    assert_eq!(edit(&s, &ana, 2, add("bobchar")).await, DONE);
    assert_eq!(edit(&s, &ana, 3, add("cidchar")).await, DONE);
    assert_eq!(edit(&s, &ana, 4, add("danchar")).await, ErrorCode(130));
    // A friend added again is no new row.
    assert_eq!(edit(&s, &ana, 5, add("bobchar")).await, DONE);
    assert_eq!(
        edit(&s, &ana, 6, FriendEdit::Remove("bobchar".into())).await,
        DONE
    );
    assert_eq!(edit(&s, &ana, 7, add("danchar")).await, DONE);
    assert_eq!(friends_of(&s, &ana).await.len(), 2);
    let ignore = |n: &str| FriendEdit::Ignore(n.into());
    assert_eq!(edit(&s, &ana, 8, ignore("bobchar")).await, DONE);
    assert_eq!(edit(&s, &ana, 9, ignore("cidchar")).await, ErrorCode(130));
}

#[tokio::test]
async fn adds_are_rate_limited() {
    let (s, ids, _g) = friends_service("adds", &["ana", "bob"], SocialConfig::default()).await;
    let (ana, _arx) = online(&s, 1, ids[0], "ana").await;
    for req in 0..friends::ADDS_PER_HOUR {
        assert_eq!(edit(&s, &ana, req + 2, add("bobchar")).await, DONE);
    }
    assert_eq!(
        edit(&s, &ana, 99, add("bobchar")).await,
        ErrorCode::RATE_LIMITED
    );
}

#[tokio::test]
async fn friends_serve_from_character_select() {
    let (s, ids, _g) = friends_service("no-char", &["ana", "bob"], SocialConfig::default()).await;
    let (bob, _brx) = online(&s, 2, ids[1], "bob").await;
    let (mut ana, _arx) = member(1, ids[0], "ana");
    ana.character = None;
    s.seat(&ana);
    edit(&s, &ana, 2, add("bobchar")).await;
    edit(&s, &bob, 2, add("anachar")).await;
    s.place(bob.session, Some(9));
    s.friends().settled().await;
    // Without a character she sees the game but can't join it.
    let r = friends_of(&s, &ana).await;
    assert_eq!(
        r[0].game.as_ref().map(|g| g.joinable),
        Some(ErrorCode::NO_CHAR_SELECTED.0)
    );
}
