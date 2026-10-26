//! The realm chat's verbs (`docs/plans/server/lobby-contract.md` section 3,
//! kinds 0x70-0x7F), driven on the bot's control stream as the game's
//! client sends them. The first chat step of a session sends its
//! `SocialHello` (all three rooms); what the main server pushes from then
//! on (room lines, DMs, the state, the watched user list) is kept for the
//! queries.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use d2_net::msg::social::{
    line_what, place, presence_op, room, Ask, DmLine, Done, FromRealm, RoomLine, SocialState,
    ToRealm,
};
use d2_net::msg::{kind as msg_kind, ErrorCode, ErrorMsg};

use super::{Bot, Failed, Outcome, Status};

/// The rooms a bot's hello asks for.
const ROOMS: u16 = room::bit(room::GLOBAL) | room::bit(room::MOD) | room::bit(room::GUILD);

/// How long a list query's answer is reused: the runner asks a query again
/// every few milliseconds until it holds, and the realm takes 30 list
/// requests a minute.
pub(super) const LIST_AGAIN: Duration = Duration::from_millis(500);

/// What the bot remembers of the chat during a session.
#[derive(Debug, Default)]
pub struct Memo {
    /// The last request number sent.
    req: u32,
    /// The hello's state came (the session is in its rooms).
    hello: bool,
    /// The realm's time at the hello: older lines are its backlog.
    hello_ms: u64,
    /// The last state (the hello's answer or a push).
    state: Option<SocialState>,
    /// The lines heard, by room.
    lines: BTreeMap<u8, Vec<RoomLine>>,
    /// The DM lines pushed (`req` 0).
    dms: Vec<DmLine>,
    /// The watched user list.
    watch: Option<Watch>,
    /// The last list query (its verb and words), when it was asked and
    /// its answer.
    asked: Option<(String, Instant, Outcome)>,
}

/// A watched room's users as its pushes keep them.
#[derive(Debug, Default)]
struct Watch {
    room: u8,
    /// Character name (any case) to its place.
    users: BTreeMap<String, u8>,
    joined: u32,
    left: u32,
    changed: u32,
}

/// The step's words: plain ones and `key=value` ones.
pub(super) struct Args<'a> {
    pub(super) plain: Vec<&'a str>,
    kv: BTreeMap<&'a str, &'a str>,
}

impl<'a> Args<'a> {
    pub(super) fn of(args: &'a [String]) -> Self {
        let mut plain = Vec::new();
        let mut kv = BTreeMap::new();
        for a in args {
            match a.split_once('=') {
                Some((k, v)) => {
                    kv.insert(k, v);
                }
                None => plain.push(a.as_str()),
            }
        }
        Self { plain, kv }
    }

    pub(super) fn get(&self, k: &str) -> Option<&'a str> {
        self.kv.get(k).copied()
    }
}

pub(super) fn fail(s: impl Into<String>) -> Failed {
    Failed::Local(Status::Error(s.into()))
}

pub(super) fn yes(b: bool) -> &'static str {
    if b {
        "yes"
    } else {
        "no"
    }
}

/// `global`, `mod`, `guild` or `game` (never served: refused).
fn room_of(word: Option<&str>) -> Result<u8, Failed> {
    match word.map(str::to_ascii_lowercase).as_deref() {
        Some("global") => Ok(room::GLOBAL),
        Some("mod") => Ok(room::MOD),
        Some("guild") => Ok(room::GUILD),
        Some("game") => Ok(room::GAME),
        w => Err(fail(format!("a room is global|mod|guild|game, not {w:?}"))),
    }
}

fn what_name(what: u8) -> &'static str {
    match what {
        line_what::SAID => "said",
        line_what::EMOTE => "emote",
        line_what::JOINED => "joined",
        line_what::LEFT => "left",
        line_what::REALM_NOTICE => "notice",
        line_what::GUILD_EVENT => "guild",
        _ => "other",
    }
}

pub(super) fn place_name(p: u8) -> &'static str {
    match p {
        place::OFFLINE => "offline",
        place::LOBBY => "lobby",
        place::IN_GAME => "game",
        place::HIDDEN => "hidden",
        _ => "other",
    }
}

fn code_name(c: ErrorCode) -> String {
    match c.name() {
        "Unknown" => c.0.to_string(),
        n => n.to_owned(),
    }
}

/// A `SocialDone` as the step's outcome.
pub(super) fn done_outcome(d: &Done, ms: u128) -> Outcome {
    let mut o = if d.is_ok() {
        Outcome::ok()
    } else {
        Outcome {
            status: Status::Refused {
                code: code_name(d.code),
                text: d.message(),
            },
            facts: BTreeMap::new(),
        }
    };
    o = o.fact("ms", ms).fact("wait_s", d.wait_s);
    if let Some(id) = d.id {
        o = o.fact("dm", id);
    }
    o
}

impl Bot {
    /// One chat verb (`say`, `whisper`, `heard-in`, `dm-unread`,
    /// `dm-history`, `users`).
    pub fn social(&mut self, verb: &str, args: &[String], wait: Duration) -> Outcome {
        let started = Instant::now();
        let a = Args::of(args);
        if let Err(f) = self.social_ready(wait) {
            return f.outcome(started.elapsed().as_millis());
        }
        let wait = wait.saturating_sub(started.elapsed());
        let lists = matches!(verb, "dm-unread" | "dm-history" | "users")
            && a.get("read").is_none()
            && a.get("watch").is_none();
        let key = format!("{verb} {}", args.join(" "));
        match &self.chat.asked {
            Some((k, at, o)) if lists && *k == key && at.elapsed() < LIST_AGAIN => {
                return o.clone();
            }
            _ => self.chat.asked = None,
        }
        let r = match verb {
            "say" => self.chat_say(&a, wait),
            "whisper" => self.chat_whisper(&a, wait),
            "heard-in" => self.chat_heard_in(&a),
            "dm-unread" => self.chat_dm_unread(wait),
            "dm-history" => self.chat_dm_history(&a, wait),
            "users" => self.chat_users(&a, wait),
            _ => Err(fail(format!("{verb} has no driver"))),
        };
        let o = r.unwrap_or_else(|f| f.outcome(started.elapsed().as_millis()));
        if lists {
            self.chat.asked = Some((key, started, o.clone()));
        }
        o
    }

    /// Takes the room lines and the live DM lines heard so far, so the
    /// load driver (`crate::load`) reads them as they come and the bot
    /// keeps none of them.
    pub fn take_heard(&mut self) -> (Vec<RoomLine>, Vec<DmLine>) {
        self.drain_events();
        let lines = std::mem::take(&mut self.chat.lines)
            .into_values()
            .flatten()
            .collect();
        (lines, std::mem::take(&mut self.chat.dms))
    }

    /// The session's hello, once: the state, then the rooms' backlogs
    /// (taken in as pushes).
    pub(super) fn social_ready(&mut self, wait: Duration) -> Result<(), Failed> {
        if self.chat.hello {
            return Ok(());
        }
        let got = self.social_send(Ask::Hello { rooms: ROOMS }, wait)?;
        match got.into_iter().last() {
            Some(FromRealm::State(s)) => {
                self.chat.hello = true;
                self.chat.hello_ms = s.now_ms;
                self.chat.state = Some(s);
                Ok(())
            }
            Some(FromRealm::Done(d)) => Err(Failed::Local(done_outcome(&d, 0).status)),
            _ => Err(fail("no SocialState")),
        }
    }

    /// Sends one request and reads its answers: every message with its
    /// request number, up to its last one (`SocialDone`, `SocialState` or
    /// the list's end). Pushes in between are kept.
    fn social_send(&mut self, ask: Ask, wait: Duration) -> Result<Vec<FromRealm>, Failed> {
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
                    let msg = FromRealm::parse(&b)
                        .map_err(|e| fail(format!("a malformed social message 0x{k:02X}: {e}")))?;
                    if msg.req() != Some(req) {
                        self.social_pushed(msg);
                        continue;
                    }
                    let last = matches!(
                        msg,
                        FromRealm::Done(_) | FromRealm::State(_) | FromRealm::ListEnd { .. }
                    );
                    got.push(msg);
                    if last {
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

    /// A social message that isn't the answer waited for: a push, or a
    /// late answer to an earlier request (dropped).
    pub(super) fn social_pushed(&mut self, msg: FromRealm) {
        match msg {
            FromRealm::State(s) if s.req == 0 => self.chat.state = Some(s),
            FromRealm::RoomLine(l) => self.chat.lines.entry(l.room).or_default().push(l),
            FromRealm::DmLine(l) if l.req == 0 => self.chat.dms.push(l),
            FromRealm::RoomPresence(p) => {
                if let Some(w) = self.chat.watch.as_mut().filter(|w| w.room == p.room) {
                    let name = p.who.char.to_ascii_lowercase();
                    match p.op {
                        presence_op::JOINED => {
                            w.joined += 1;
                            w.users.insert(name, p.place);
                        }
                        presence_op::LEFT => {
                            w.left += 1;
                            w.users.remove(&name);
                        }
                        _ => {
                            w.changed += 1;
                            w.users.insert(name, p.place);
                        }
                    }
                }
            }
            _ => {}
        }
    }

    /// The next request number of the social service (chat, guilds and
    /// friends share one counter, as the game's client).
    pub(super) fn social_next_req(&mut self) -> u32 {
        self.chat.req += 1;
        self.chat.req
    }

    /// The last `SocialState` (the privacy choices the realm holds).
    pub(super) fn social_state(&self) -> Option<&SocialState> {
        self.chat.state.as_ref()
    }

    /// The session ended: the next chat step says hello again.
    pub(super) fn social_reset(&mut self) {
        self.chat = Memo::default();
        self.guild = super::guild::Memo::default();
        self.friends = super::friends::Memo::default();
    }

    /// The answer's `SocialDone`, as the outcome.
    fn social_done(&mut self, ask: Ask, wait: Duration) -> Result<(Outcome, Done), Failed> {
        let started = Instant::now();
        let got = self.social_send(ask, wait)?;
        match got.last() {
            Some(FromRealm::Done(d)) => {
                Ok((done_outcome(d, started.elapsed().as_millis()), d.clone()))
            }
            _ => Err(fail("no SocialDone")),
        }
    }

    /// The answer's messages, or the refusal of its `SocialDone`.
    fn social_list(&mut self, ask: Ask, wait: Duration) -> Result<Vec<FromRealm>, Failed> {
        let got = self.social_send(ask, wait)?;
        if let Some(FromRealm::Done(d)) = got.last() {
            return Err(Failed::Local(done_outcome(d, 0).status));
        }
        Ok(got)
    }

    /// `say ROOM TEXT.. [emote=yes]`.
    fn chat_say(&mut self, a: &Args, wait: Duration) -> Result<Outcome, Failed> {
        let r = room_of(a.plain.first().copied())?;
        let text = a.plain[1..].join(" ");
        let ask = Ask::Say {
            room: r,
            emote: a.get("emote") == Some("yes"),
            text,
        };
        Ok(self.social_done(ask, wait)?.0)
    }

    /// `whisper NAME|*ACCOUNT TEXT..`: facts dm, stored.
    fn chat_whisper(&mut self, a: &Args, wait: Duration) -> Result<Outcome, Failed> {
        let Some(to) = a.plain.first() else {
            return Err(fail("whisper takes NAME|*ACCOUNT TEXT"));
        };
        let ask = Ask::Whisper {
            to: (*to).to_owned(),
            text: a.plain[1..].join(" "),
        };
        let (o, d) = self.social_done(ask, wait)?;
        Ok(o.fact(
            "stored",
            yes(d.flags & d2_net::msg::social::done_flags::STORED != 0),
        ))
    }

    /// `heard-in ROOM [TEXT..]`: the room's lines so far; with TEXT,
    /// whether (and how often) a line of exactly that text came.
    fn chat_heard_in(&mut self, a: &Args) -> Result<Outcome, Failed> {
        let r = room_of(a.plain.first().copied())?;
        self.drain_events();
        let lines = self
            .chat
            .lines
            .get(&r)
            .map(Vec::as_slice)
            .unwrap_or_default();
        let backlog = lines
            .iter()
            .filter(|l| l.at_ms < self.chat.hello_ms)
            .count();
        let mut o = Outcome::ok()
            .fact("lines", lines.len())
            .fact("backlog", backlog);
        if let Some(l) = lines.last() {
            o = o
                .fact("last", &l.text)
                .fact("from", &l.who.char)
                .fact("tag", l.who.tag.as_deref().unwrap_or(""))
                .fact("what", what_name(l.what));
        }
        let spoken = lines
            .iter()
            .rev()
            .find(|l| l.what == line_what::SAID || l.what == line_what::EMOTE);
        if let Some(l) = spoken {
            o = o.fact("said", &l.text).fact("said.from", &l.who.char);
        }
        // Every spoken line heard, and how many of them differ: a line
        // heard twice shows as fewer distinct than counted.
        let said: Vec<&str> = lines
            .iter()
            .filter(|l| l.what == line_what::SAID || l.what == line_what::EMOTE)
            .map(|l| l.text.as_str())
            .collect();
        let distinct: std::collections::BTreeSet<&str> = said.iter().copied().collect();
        o = o
            .fact("said.count", said.len())
            .fact("said.distinct", distinct.len());
        if a.plain.len() > 1 {
            let want = a.plain[1..].join(" ");
            let times = lines.iter().filter(|l| l.text == want).count();
            o = o
                .fact("has", if times > 0 { "yes" } else { "no" })
                .fact("times", times);
        }
        Ok(o)
    }

    /// `dm-unread`: the state's counts and the conversations' list.
    fn chat_dm_unread(&mut self, wait: Duration) -> Result<Outcome, Failed> {
        let got = self.social_list(Ask::DmConversations { page: 0 }, wait)?;
        self.drain_events();
        let st = self.chat.state.clone().unwrap_or_default();
        let incoming: Vec<&DmLine> = self.chat.dms.iter().filter(|l| !l.outgoing).collect();
        let mut o = Outcome::ok()
            .fact("unread", st.dm_unread)
            .fact("conversations", st.dm_conversations_unread)
            .fact("pushed", incoming.iter().filter(|l| !l.stored).count())
            .fact(
                "pushed.stored",
                incoming.iter().filter(|l| l.stored).count(),
            );
        for m in got {
            if let FromRealm::DmConversationRow { row, .. } = m {
                let k = row.peer_account.to_ascii_lowercase();
                o = o
                    .fact(format!("{k}.unread"), row.unread)
                    .fact(format!("{k}.where"), place_name(row.place))
                    .fact(format!("{k}.last"), &row.last_text);
            }
        }
        Ok(o)
    }

    /// `dm-history PEER [rows=N] [read=yes]`.
    fn chat_dm_history(&mut self, a: &Args, wait: Duration) -> Result<Outcome, Failed> {
        let Some(peer) = a.plain.first() else {
            return Err(fail("dm-history takes PEER"));
        };
        let rows = a
            .get("rows")
            .map_or(Ok(50), str::parse)
            .map_err(|_| fail("rows=N"))?;
        let got = self.social_list(
            Ask::DmHistory {
                peer: (*peer).to_owned(),
                before_dm: 0,
                rows,
            },
            wait,
        )?;
        let lines: Vec<DmLine> = got
            .into_iter()
            .filter_map(|m| match m {
                FromRealm::DmLine(l) => Some(l),
                _ => None,
            })
            .collect();
        let mut o = Outcome::ok().fact("count", lines.len());
        if let Some(f) = lines.first() {
            o = o.fact("first", &f.text);
        }
        if let Some(l) = lines.last() {
            o = o
                .fact("last", &l.text)
                .fact("last.stored", yes(l.stored))
                .fact("last.outgoing", yes(l.outgoing))
                .fact("last.char", &l.peer_char);
            if a.get("read") == Some("yes") {
                let ask = Ask::DmRead {
                    peer_account: l.peer_account.clone(),
                    up_to_dm: l.dm,
                };
                let (read, _) = self.social_done(ask, wait)?;
                if read.status != Status::Ok {
                    return Ok(read);
                }
            }
        }
        Ok(o)
    }

    /// `users ROOM [NAME..] [watch=yes|no]`: the room's user list (or the
    /// watched one as its pushes keep it); has.NAME for each NAME.
    fn chat_users(&mut self, a: &Args, wait: Duration) -> Result<Outcome, Failed> {
        let r = room_of(a.plain.first().copied())?;
        let watch = a.get("watch");
        let live = watch.is_none() && self.chat.watch.as_ref().is_some_and(|w| w.room == r);
        let users: BTreeMap<String, u8> = if live {
            self.drain_events();
            self.chat
                .watch
                .as_ref()
                .map(|w| w.users.clone())
                .unwrap_or_default()
        } else {
            let got = self.social_list(
                Ask::UserList {
                    room: r,
                    watch: watch == Some("yes"),
                },
                wait,
            )?;
            let mut order = Vec::new();
            let mut users = BTreeMap::new();
            for m in got {
                if let FromRealm::UserRow { who, place, .. } = m {
                    order.push(who.char.clone());
                    users.insert(who.char.to_ascii_lowercase(), place);
                }
            }
            let mut sorted = order.clone();
            sorted.sort_by_key(|n| n.to_ascii_lowercase());
            if sorted != order {
                return Ok(Outcome::error(format!("the rows aren't sorted: {order:?}")));
            }
            self.chat.watch = match watch {
                Some("yes") => Some(Watch {
                    room: r,
                    users: users.clone(),
                    ..Watch::default()
                }),
                Some("no") => None,
                _ => self.chat.watch.take(),
            };
            users
        };
        let mut o = Outcome::ok().fact("count", users.len());
        for name in &a.plain[1..] {
            let at = users.get(&name.to_ascii_lowercase());
            o = o.fact(format!("has.{name}"), yes(at.is_some())).fact(
                format!("{name}.where"),
                place_name(at.copied().unwrap_or(place::OFFLINE)),
            );
        }
        if let Some(w) = self.chat.watch.as_ref().filter(|w| w.room == r) {
            o = o
                .fact("joined", w.joined)
                .fact("left", w.left)
                .fact("changed", w.changed);
        }
        Ok(o)
    }
}
