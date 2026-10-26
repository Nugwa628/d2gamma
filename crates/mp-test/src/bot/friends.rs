//! The friends' verbs (`docs/plans/server/lobby-contract.md` section 5,
//! kinds 0xF8-0xFF), driven on the bot's control stream as the game's
//! client sends them. The first step of a session sends the chat's
//! `SocialHello` (the realm pushes the friends' notices to sessions that
//! said hello); the notices heard from then on are kept for `friend-list`.

use std::time::{Duration, Instant};

use d2_net::msg::guild::{
    friend_notice, friends_get, Ask, FriendEdit, FriendNotice, FriendRow, FromRealm, ToRealm,
};
use d2_net::msg::lobby::LobbyRequest;
use d2_net::msg::social::{self, show_where, whispers};
use d2_net::msg::{kind as msg_kind, ErrorMsg};

use super::chat::{done_outcome, fail, place_name, yes, Args, LIST_AGAIN};
use super::{lobby_answer, Bot, Failed, Outcome};

/// What the bot remembers of its friends during a session.
#[derive(Debug, Default)]
pub struct Memo {
    /// The notices pushed.
    notices: Vec<FriendNotice>,
    /// The last list (its words), when it was asked and its answer.
    asked: Option<(String, Instant, Outcome)>,
}

/// One answer message of a friends request.
enum Got {
    Social(social::FromRealm),
    Friends(FromRealm),
}

fn notice_name(what: u8) -> &'static str {
    match what {
        friend_notice::ONLINE => "online",
        friend_notice::OFFLINE => "offline",
        friend_notice::ENTERED_GAME => "entered",
        friend_notice::LEFT_GAME => "left",
        friend_notice::ADDED_YOU => "added",
        _ => "other",
    }
}

impl Bot {
    /// One friends verb (`friend-add`, `friend-remove`, `friend-ignore`,
    /// `friend-unignore`, `friend-privacy`, `friend-list`).
    pub fn friends(&mut self, verb: &str, args: &[String], wait: Duration) -> Outcome {
        let started = Instant::now();
        let a = Args::of(args);
        if let Err(f) = self.social_ready(wait) {
            return f.outcome(started.elapsed().as_millis());
        }
        let wait = wait.saturating_sub(started.elapsed());
        let key = format!("{verb} {}", args.join(" "));
        let list = verb == "friend-list";
        match &self.friends.asked {
            Some((k, at, o)) if list && *k == key && at.elapsed() < LIST_AGAIN => {
                return o.clone();
            }
            _ => self.friends.asked = None,
        }
        let r = match verb {
            "friend-add" | "friend-remove" | "friend-ignore" | "friend-unignore" => {
                self.friend_edit(verb, &a, wait)
            }
            "friend-privacy" => self.friend_privacy(&a, wait),
            "friend-list" => self.friend_list(&a, wait),
            _ => Err(fail(format!("{verb} has no driver"))),
        };
        let o = r.unwrap_or_else(|f| f.outcome(started.elapsed().as_millis()));
        if list {
            self.friends.asked = Some((key, started, o.clone()));
        }
        o
    }

    /// Sends one request and reads its answers: every message with its
    /// request number, up to its `SocialDone` or the list's end. Pushes in
    /// between are kept.
    fn friends_send(&mut self, ask: Ask, wait: Duration) -> Result<Vec<Got>, Failed> {
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
                    let msg = social::FromRealm::parse(&b)
                        .map_err(|e| fail(format!("a malformed social message 0x{k:02X}: {e}")))?;
                    if msg.req() != Some(req) {
                        self.social_pushed(msg);
                        continue;
                    }
                    let last = matches!(
                        msg,
                        social::FromRealm::Done(_) | social::FromRealm::ListEnd { .. }
                    );
                    got.push(Got::Social(msg));
                    if last {
                        return Ok(got);
                    }
                }
                Some(k) if msg_kind::FRIENDS.contains(k) => {
                    let msg = FromRealm::parse(&b)
                        .map_err(|e| fail(format!("a malformed friends message 0x{k:02X}: {e}")))?;
                    if msg.req() == Some(req) {
                        got.push(Got::Friends(msg));
                    } else {
                        self.friends_pushed(msg);
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

    /// A friends message that isn't the answer waited for: a notice, or a
    /// late answer to an earlier request (dropped).
    pub(super) fn friends_pushed(&mut self, msg: FromRealm) {
        if let FromRealm::FriendNotice(n) = msg {
            self.friends.notices.push(n);
        }
    }

    /// The answer's `SocialDone`, as the outcome.
    fn friend_done(&mut self, ask: Ask, wait: Duration) -> Result<Outcome, Failed> {
        let started = Instant::now();
        let got = self.friends_send(ask, wait)?;
        match got.last() {
            Some(Got::Social(social::FromRealm::Done(d))) => {
                Ok(done_outcome(d, started.elapsed().as_millis()))
            }
            _ => Err(fail("no SocialDone")),
        }
    }

    /// `friend-add|remove|ignore|unignore NAME|*ACCOUNT`.
    fn friend_edit(&mut self, verb: &str, a: &Args, wait: Duration) -> Result<Outcome, Failed> {
        let Some(who) = a.plain.first().map(|w| (*w).to_owned()) else {
            return Err(fail(format!("{verb} takes NAME|*ACCOUNT")));
        };
        let e = match verb {
            "friend-add" => FriendEdit::Add(who),
            "friend-remove" => FriendEdit::Remove(who),
            "friend-ignore" => FriendEdit::Ignore(who),
            _ => FriendEdit::Unignore(who),
        };
        self.friend_done(Ask::FriendEdit(e), wait)
    }

    /// `friend-privacy [whispers=everyone|friends|nobody]
    /// [where=friends|nobody]`; a choice it doesn't name stays as the
    /// realm holds it.
    fn friend_privacy(&mut self, a: &Args, wait: Duration) -> Result<Outcome, Failed> {
        let st = self.social_state();
        let mut w = st.map_or(whispers::EVERYONE, |s| s.whispers);
        let mut sw = st.map_or(show_where::MUTUAL_FRIENDS, |s| s.show_where);
        if let Some(v) = a.get("whispers") {
            w = match v {
                "everyone" => whispers::EVERYONE,
                "friends" => whispers::FRIENDS_AND_GUILD,
                "nobody" => whispers::NOBODY,
                _ => return Err(fail("whispers=everyone|friends|nobody")),
            };
        }
        if let Some(v) = a.get("where") {
            sw = match v {
                "friends" => show_where::MUTUAL_FRIENDS,
                "nobody" => show_where::NOBODY,
                _ => return Err(fail("where=friends|nobody")),
            };
        }
        let e = FriendEdit::Privacy {
            whispers: w,
            show_where: sw,
        };
        self.friend_done(Ask::FriendEdit(e), wait)
    }

    /// The friends list (or the ignore list), or the refusal.
    fn friend_rows(&mut self, ignored: bool, wait: Duration) -> Result<Vec<FriendRow>, Failed> {
        let what = if ignored {
            friends_get::IGNORED
        } else {
            friends_get::FRIENDS
        };
        let got = self.friends_send(Ask::FriendsGet { what }, wait)?;
        if let Some(Got::Social(social::FromRealm::Done(d))) = got.last() {
            return Err(Failed::Local(done_outcome(d, 0).status));
        }
        Ok(got
            .into_iter()
            .filter_map(|m| match m {
                Got::Friends(FromRealm::FriendRow { row, .. }) => Some(row),
                _ => None,
            })
            .collect())
    }

    /// `friend-list [ignored=yes] [NAME..]`: the list's rows, each under
    /// its character's name as the realm sends it, and has.NAME for each
    /// NAME (a character or account name, any case); the notices heard.
    fn friend_list(&mut self, a: &Args, wait: Duration) -> Result<Outcome, Failed> {
        let rows = self.friend_rows(a.get("ignored") == Some("yes"), wait)?;
        self.drain_events();
        let mut o = Outcome::ok()
            .fact("count", rows.len())
            .fact("notices", self.friends.notices.len());
        if let Some(n) = self.friends.notices.last() {
            o = o
                .fact("notice", notice_name(n.what))
                .fact("notice.from", &n.who.char)
                .fact("notice.where", place_name(n.place));
        }
        let row_facts = |mut o: Outcome, k: &str, r: &FriendRow| {
            let game = r.game.as_ref();
            o = o
                .fact(format!("has.{k}"), "yes")
                .fact(format!("{k}.where"), place_name(r.place))
                .fact(format!("{k}.mutual"), yes(r.mutual))
                .fact(
                    format!("{k}.game"),
                    game.map_or("", |g| g.game_name.as_str()),
                )
                .fact(format!("{k}.game_id"), game.map_or(0, |g| g.game_id))
                .fact(format!("{k}.joinable"), game.map_or(0, |g| g.joinable))
                .fact(format!("{k}.seen"), yes(r.last_seen_ms != 0));
            o
        };
        for r in &rows {
            o = row_facts(o, &r.who.char, r);
        }
        for name in &a.plain {
            let row = rows.iter().find(|r| {
                r.who.char.eq_ignore_ascii_case(name) || r.who.account.eq_ignore_ascii_case(name)
            });
            o = match row {
                Some(r) => row_facts(o, name, r),
                None => o
                    .fact(format!("has.{name}"), "no")
                    .fact(format!("{name}.where"), "offline"),
            };
        }
        Ok(o)
    }

    /// `friend-join NAME [password=P]`: joins the game the friends list
    /// shows for NAME (a mutual friend's), by its id, then attaches and
    /// enters as `join-game` does.
    pub fn friend_join(&mut self, args: &[String], wait: Duration, until: Instant) -> Outcome {
        let started = Instant::now();
        let a = Args::of(args);
        let Some(name) = a.plain.first().copied() else {
            return Outcome::error("friend-join takes NAME [password=P]");
        };
        let rows = match self.friend_rows(false, wait) {
            Ok(r) => r,
            Err(f) => return f.outcome(started.elapsed().as_millis()),
        };
        let game = rows
            .iter()
            .find(|r| {
                r.who.char.eq_ignore_ascii_case(name) || r.who.account.eq_ignore_ascii_case(name)
            })
            .and_then(|r| r.game.clone());
        let Some(game) = game else {
            return Outcome::error(format!("the friends list shows no game of {name}"));
        };
        self.known_games.insert(game.game_name.clone());
        let req = LobbyRequest::JoinGame {
            game_id: game.game_id,
            password: a.get("password").unwrap_or_default().into(),
        };
        let left = wait.saturating_sub(started.elapsed());
        let answers = [lobby_answer::JOIN_OK, lobby_answer::JOIN_NODE];
        match self.request(&req, &answers, left) {
            Ok(body) => self.joined(&body, &game.game_name, until, started),
            Err(f) => f.outcome(started.elapsed().as_millis()),
        }
    }
}
