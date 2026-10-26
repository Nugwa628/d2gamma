//! Friends, ignores and privacy (lobby contract section 5; lobby plan
//! L5a): `FriendsGet`, `FriendEdit` and the `FriendNotice` pushes.
//!
//! - Friends are one-sided: adding someone needs no answer, and the added
//!   account gets an "added you" notice. Two accounts that added each
//!   other are mutual friends.
//! - **Where:** a mutual friend sees the other's place (lobby, or the game
//!   with its name and whether the asker's selected character can join
//!   it) unless the other's `show_where` is "nobody". Everyone else on a
//!   friends list sees online or offline only: an online player whose
//!   place isn't shown is `Where` 3 (hidden).
//! - **Notices** go to every session (with a hello) of the accounts that
//!   added the one that moved: online and offline to all of them, entered
//!   and left a game only to those who see where it is. One worker sends
//!   them in the order the sessions moved; it also sets
//!   `accounts.last_seen` when an account's last session ends and gives
//!   the followers their new `SocialState` (its `friends_online`).
//! - Ignores and privacy are applied by the rooms and the DMs (the
//!   account's view is read again after every change); this part only
//!   stores them.
//! - A game's name and the join rule come from the lobby ([`FriendGames`]).

use std::collections::HashMap;
use std::sync::{OnceLock, Weak};
use std::time::Instant;

use d2_accounts::db::{Db, DbError};
use d2_accounts::social::{Peer, Privacy, Refusal, ShowWhere, Whispers};
use d2_limits::{Limiter, Rate};
use d2_net::msg::account::CharSummary;
use d2_net::msg::guild::{
    friend_notice, friends_get, kind, Ask, FriendEdit, FriendGame, FriendNotice, FriendRow,
    FromRealm, ToRealm,
};
use d2_net::msg::social::{self, place, who_flags, ListHead, Who, NO_CLASS};
use d2_net::msg::ErrorCode;
use tokio::sync::{mpsc, oneshot};

use super::presence::{who_of, Changed};
use super::{lock, wire_ms, Member, Social};
use crate::accounts::AccountId;
use crate::denied::Denied;

/// What this part adds to the ready line's `caps`.
pub const CAPS: &[&str] = &["friends"];

/// `FriendEdit` adds per account per hour (contract 9).
pub const ADDS_PER_HOUR: u32 = 20;

/// The open games, as the friends see them (the lobby's).
pub trait FriendGames: Send + Sync {
    /// Game `game`'s entry for a friend's row: its name, and 0 or the code
    /// why `asker` (the asker's selected character) can't join it.
    /// `None` when the game is gone.
    fn friend_game(&self, game: u32, asker: Option<&CharSummary>) -> Option<FriendGame>;
}

/// An account's place after one of its sessions moved.
#[derive(Clone, Debug)]
struct Moved {
    account: AccountId,
    account_name: String,
    /// [`place`]: offline, lobby or in a game.
    place: u8,
    game: Option<u32>,
    /// The character it shows (or showed last).
    character: Option<CharSummary>,
}

enum Work {
    Moved(Moved),
    /// Answered when everything before it went out.
    Flush(oneshot::Sender<()>),
}

/// The friends' part of the social service.
pub struct Friends {
    me: Weak<Social>,
    adds: std::sync::Mutex<Limiter<i64>>,
    games: OnceLock<Weak<dyn FriendGames>>,
    worker: OnceLock<mpsc::UnboundedSender<Work>>,
}

impl std::fmt::Debug for Friends {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Friends").finish_non_exhaustive()
    }
}

impl Friends {
    /// The part of the service `me` (limits divided by 60 with
    /// `test_windows`).
    pub(super) fn new(test_windows: bool, me: Weak<Social>) -> Self {
        let rate = Rate::per_secs(ADDS_PER_HOUR, 3600);
        Self {
            me,
            adds: std::sync::Mutex::new(Limiter::new(if test_windows {
                rate.test()
            } else {
                rate
            })),
            games: OnceLock::new(),
            worker: OnceLock::new(),
        }
    }

    /// The lobby's open games (set once, at the start).
    pub fn set_games(&self, games: Weak<dyn FriendGames>) {
        let _ = self.games.set(games);
    }

    fn game(&self, game: u32, asker: Option<&CharSummary>) -> Option<FriendGame> {
        self.games.get()?.upgrade()?.friend_game(game, asker)
    }

    /// Hands `w` to the worker, started on the first change. Without a
    /// runtime (nothing to send from) the change is dropped.
    fn queue(&self, w: Work) {
        let tx = self.worker.get_or_init(|| {
            let (tx, rx) = mpsc::unbounded_channel();
            if let Ok(rt) = tokio::runtime::Handle::try_current() {
                rt.spawn(run(self.me.clone(), rx));
            }
            tx
        });
        let _ = tx.send(w);
    }

    /// Waits until every notice of the changes so far went out.
    pub async fn settled(&self) {
        let (tx, rx) = oneshot::channel();
        self.queue(Work::Flush(tx));
        let _ = rx.await;
    }
}

/// The worker: one change after another, in order.
async fn run(me: Weak<Social>, mut rx: mpsc::UnboundedReceiver<Work>) {
    // What each online account's followers were told last.
    let mut told: HashMap<i64, (u8, Option<u32>)> = HashMap::new();
    while let Some(w) = rx.recv().await {
        match w {
            Work::Flush(done) => {
                let _ = done.send(());
            }
            Work::Moved(m) => {
                let Some(s) = me.upgrade() else {
                    return;
                };
                moved(&s, &mut told, m).await;
            }
        }
    }
}

/// The account's guild tag, if it is in a guild.
fn tag_of(db: &Db, account: i64) -> Result<Option<String>, DbError> {
    Ok(match db.guilds().membership(account)? {
        Some((id, _)) => db.guilds().get(id)?.map(|g| g.tag),
        None => None,
    })
}

/// A friend as rows and notices show them: the character, the account and
/// the guild tag; the in-game flag only for those who see where.
fn friend_who(
    character: Option<&CharSummary>,
    name: Option<&str>,
    account: &str,
    tag: Option<String>,
) -> Who {
    let mut w = match character {
        Some(c) => who_of(c),
        None => Who {
            char: name.unwrap_or_default().to_owned(),
            class: NO_CLASS,
            ..Who::default()
        },
    };
    w.account = account.to_owned();
    w.tag = tag;
    w
}

/// The place one who sees `sees` is shown of a player at `at`.
fn shown(at: u8, sees: bool) -> u8 {
    match at {
        place::OFFLINE => place::OFFLINE,
        _ if sees => at,
        _ => place::HIDDEN,
    }
}

/// Whether the friend's `privacy` shows its place to a mutual friend.
fn shows_where(privacy: &Privacy) -> bool {
    privacy.show_where == ShowWhere::MutualFriends
}

/// What one move tells the followers.
async fn moved(s: &Social, told: &mut HashMap<i64, (u8, Option<u32>)>, m: Moved) {
    let a = m.account.0;
    let before = told.get(&a).copied().unwrap_or((place::OFFLINE, None));
    let after = (m.place, m.game);
    if before == after {
        return;
    }
    if after.0 == place::OFFLINE {
        told.remove(&a);
    } else {
        told.insert(a, after);
    }
    let what = match (before.0, after.0) {
        (_, place::OFFLINE) => friend_notice::OFFLINE,
        (place::OFFLINE, _) => friend_notice::ONLINE,
        (_, place::IN_GAME) => friend_notice::ENTERED_GAME,
        (place::IN_GAME, _) => friend_notice::LEFT_GAME,
        _ => return,
    };
    let now = s.clock().now();
    let offline = what == friend_notice::OFFLINE;
    let read = s
        .accounts()
        .store(move |db| {
            if offline {
                db.friends().set_last_seen(a, now.div_euclid(1000))?;
            }
            let friends = db.friends();
            let mine: Vec<i64> = friends.list_friends(a)?.iter().map(|f| f.account).collect();
            Ok((
                friends.followers(a)?,
                mine,
                db.prefs().get(a)?,
                tag_of(db, a)?,
            ))
        })
        .await;
    let (followers, mine, privacy, tag) = match read {
        Ok(r) => r,
        Err(d) => return s.log_line(&format!("{}: friend notice: {}", m.account, d.text)),
    };
    let base = friend_who(m.character.as_ref(), None, &m.account_name, tag);
    let edge = matches!(what, friend_notice::ONLINE | friend_notice::OFFLINE);
    for f in followers {
        let sees = mine.contains(&f) && shows_where(&privacy);
        if !edge && !sees {
            continue;
        }
        notify(s, AccountId(f), what, &base, after, sees, now);
        if edge {
            s.reload(AccountId(f)).await;
        }
    }
}

/// Sends `FriendNotice` `what` about `who` (at `at`) to every session of
/// `to` with a hello; the game as each session's character sees it.
fn notify(
    s: &Social,
    to: AccountId,
    what: u8,
    who: &Who,
    at: (u8, Option<u32>),
    sees: bool,
    now: i64,
) {
    for h in s.hello_sessions(to) {
        let mut w = who.clone();
        let game = match at {
            (place::IN_GAME, Some(g)) if sees => {
                w.flags |= who_flags::IN_GAME;
                s.friends.game(g, h.character.as_ref())
            }
            _ => None,
        };
        let n = FriendNotice {
            what,
            who: w,
            place: shown(at.0, sees),
            game,
            at_ms: wire_ms(now),
        };
        h.out.send(FromRealm::FriendNotice(n).body());
    }
}

/// The account (not `me`) a request names: a character name or
/// `*account`.
async fn peer(s: &Social, me: AccountId, name: &str) -> Result<Peer, Denied> {
    let name = name.to_owned();
    s.accounts()
        .store(move |db| {
            Ok(db
                .find_player(&name)?
                .filter(|p| p.account != me.0)
                .ok_or(Refusal::UnknownPlayer)?)
        })
        .await
}

/// `FriendsGet`: the friends (or the ignored) as `FriendRow`s in a list.
async fn get(s: &Social, who: &Member, req: u32, what: u8) -> Result<(), Denied> {
    s.list_limit(who.account)?;
    let a = who.account.0;
    let ignored = what == friends_get::IGNORED;
    let rows = s
        .accounts()
        .store(move |db| {
            let list = if ignored {
                db.friends().list_ignored(a)?
            } else {
                db.friends().list_friends(a)?
            };
            let mut rows = Vec::with_capacity(list.len());
            for f in list {
                let last = match &f.last_char {
                    Some(n) => db.characters(f.account)?.into_iter().find(|c| &c.name == n),
                    None => None,
                };
                let privacy = db.prefs().get(f.account)?;
                let tag = tag_of(db, f.account)?;
                rows.push((f, last, privacy, tag));
            }
            Ok(rows)
        })
        .await?;
    let asker = who.character.as_ref();
    let out: Vec<FriendRow> = rows
        .into_iter()
        .map(|(f, last, privacy, tag)| {
            let id = AccountId(f.account);
            let sessions = s.presence().sessions_of(id);
            let character = sessions.iter().find_map(|h| h.character.clone()).or(last);
            let (at, game) = s.presence().place_of(id);
            let sees = f.mutual && shows_where(&privacy);
            let mut w = friend_who(
                character.as_ref(),
                f.last_char.as_deref(),
                &f.account_name,
                tag,
            );
            let game = match game {
                Some(g) if sees => {
                    w.flags |= who_flags::IN_GAME;
                    s.friends.game(g, asker)
                }
                _ => None,
            };
            FriendRow {
                who: w,
                place: shown(at, sees),
                mutual: f.mutual,
                game,
                last_seen_ms: f
                    .last_seen
                    .map_or(0, |unix_s| wire_ms(unix_s.saturating_mul(1000))),
                added_ms: wire_ms(f.added_ms),
            }
        })
        .collect();
    who.out.send(
        social::FromRealm::ListBegin(ListHead {
            req,
            what: kind::FRIENDS_GET,
            total: u32::try_from(out.len()).unwrap_or(u32::MAX),
            page: 0,
            rows: u16::try_from(out.len()).unwrap_or(u16::MAX),
            now_ms: wire_ms(s.clock().now()),
        })
        .body(),
    );
    for row in out {
        who.out.send(FromRealm::FriendRow { req, row }.body());
    }
    who.out.send(social::FromRealm::ListEnd { req }.body());
    Ok(())
}

/// `FriendEdit` add: stored, then "added you" to the added account.
async fn add(s: &Social, who: &Member, name: &str) -> Result<(), Denied> {
    lock(&s.friends.adds)
        .check(&who.account.0, Instant::now())
        .map_err(|r| Denied::wait(ErrorCode::RATE_LIMITED, r.wait_s()))?;
    let p = peer(s, who.account, name).await?;
    let (a, b, max, now) = (
        who.account.0,
        p.account,
        s.config().friends_max,
        s.clock().now(),
    );
    let (added, mutual, privacy, tag, last) = s
        .accounts()
        .store(move |db| {
            let added = db.friends().add_friend(a, b, now, max)?;
            Ok((
                added,
                db.friends().is_friend(b, a)?,
                db.prefs().get(a)?,
                tag_of(db, a)?,
                db.last_character(a)?,
            ))
        })
        .await?;
    s.reload(who.account).await;
    if added {
        let me = friend_who(
            who.character.as_ref(),
            last.as_deref(),
            &who.account_name,
            tag,
        );
        let sees = mutual && shows_where(&privacy);
        let at = s.presence().place_of(who.account);
        notify(
            s,
            AccountId(b),
            friend_notice::ADDED_YOU,
            &me,
            at,
            sees,
            now,
        );
    }
    Ok(())
}

/// One `FriendEdit`.
async fn edit(s: &Social, who: &Member, e: FriendEdit) -> Result<(), Denied> {
    let a = who.account.0;
    let now = s.clock().now();
    match e {
        FriendEdit::Add(name) => return add(s, who, &name).await,
        FriendEdit::Remove(name) => {
            let b = peer(s, who.account, &name).await?.account;
            s.accounts()
                .store(move |db| Ok(db.friends().remove_friend(a, b)?))
                .await?;
        }
        FriendEdit::Ignore(name) => {
            let b = peer(s, who.account, &name).await?.account;
            let max = s.config().ignore_max;
            s.accounts()
                .store(move |db| db.friends().ignore(a, b, now, max))
                .await?;
        }
        FriendEdit::Unignore(name) => {
            let b = peer(s, who.account, &name).await?.account;
            s.accounts()
                .store(move |db| Ok(db.friends().unignore(a, b)?))
                .await?;
        }
        FriendEdit::Privacy {
            whispers,
            show_where,
        } => {
            let p = Privacy {
                whispers: Whispers::from_u8(whispers).ok_or(ErrorCode::MALFORMED)?,
                show_where: ShowWhere::from_u8(show_where).ok_or(ErrorCode::MALFORMED)?,
            };
            s.accounts().store(move |db| db.prefs().set(a, p)).await?;
        }
    }
    s.reload(who.account).await;
    Ok(())
}

/// One friends request (kinds 0xF8-0xFF).
pub(super) async fn handle(s: &Social, who: &Member, msg: ToRealm) {
    let (req, k) = (msg.req, msg.kind());
    let r = match msg.ask {
        Ask::FriendsGet { what } => get(s, who, req, what).await,
        Ask::FriendEdit(e) => edit(s, who, e).await,
        _ => return who.out.error(k, ErrorCode::NOT_SUPPORTED, 0, ""),
    };
    match r {
        // A list answers with its rows; everything else with `SocialDone`.
        Ok(()) if k == kind::FRIENDS_GET => {}
        r => Social::done(&who.out, req, k, r),
    }
}

/// A session's place changed (logged in, changed its character, entered
/// or left a game, ended): the worker tells the friends.
pub(super) fn presence_changed(s: &Social, change: &Changed) {
    let account = change.here.account;
    let (at, game) = s.presence().place_of(account);
    let character = s
        .presence()
        .sessions_of(account)
        .into_iter()
        .find_map(|h| h.character)
        .or_else(|| change.here.character.clone());
    s.friends.queue(Work::Moved(Moved {
        account,
        account_name: change.here.account_name.clone(),
        place: at,
        game,
        character,
    }));
}
