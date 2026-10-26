//! The realm's chat and social service (`docs/plans/server/lobby-contract.md`
//! sections 3-5, 9 and 11): what the client's social, guild and friends
//! messages (kinds 0x70-0x7F, 0xF0-0xF7, 0xF8-0xFF) get on a player's
//! session.
//!
//! - [`SocialService`]: the seam the network side calls ([`Social`] is the
//!   realm's). Every request is answered on the session's control stream:
//!   the data, or `SocialDone` with a code. A realm without the service
//!   answers `NotSupported` and the client hides the tab.
//! - [`presence`]: who is where, for the chat, the user lists and the
//!   friends; [`rooms`]: the rooms, their lines and user lists; the DMs
//!   (`dms.rs`); [`limits`]: the rate limits (contract 9).
//! - [`guilds`]: the guilds (lobby plan L5b); [`friends`]: the friends,
//!   ignore and privacy messages and the friends' notices (L5a).
//! - [`SocialConfig`]: the config keys (contract 11).
//!
//! Store calls go through [`Accounts::store`] (off the network threads);
//! pushes go to every session of an account ([`Presence::send_to`]).

mod dms;
pub mod friends;
pub mod guilds;
pub mod limits;
pub mod presence;
pub mod rooms;

use std::future::Future;
use std::ops::RangeInclusive;
use std::pin::Pin;
use std::str::FromStr;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Instant;

use d2_accounts::social::{Refusal, ShowWhere, Whispers};
use d2_net::msg::account::CharSummary;
use d2_net::msg::lobby::MAX_CHAT;
use d2_net::msg::social::{
    line_what, room, Ask, ChatLimits as StateLimits, Done, FromRealm, GuildBrief, ListHead,
    SocialState, Who,
};
use d2_net::msg::{guild, social, ErrorCode};

use crate::accounts::{AccountId, Accounts};
use crate::denied::Denied;
use crate::log::ServerLog;
use crate::session::{ControlTx, SessionId};
use limits::ChatLimits;
pub use presence::{Here, Presence};
use rooms::{AccountView, RoomKey, Rooms};

/// A request's work, run by the network side on the session's task.
pub type Job = Pin<Box<dyn Future<Output = ()> + Send>>;

/// The session a social request comes from (logged in).
#[derive(Clone, Debug)]
pub struct Member {
    pub session: SessionId,
    pub account: AccountId,
    /// The account's stored name.
    pub account_name: String,
    /// The selected character.
    pub character: Option<CharSummary>,
    pub out: ControlTx,
}

/// The seam between the network side and the social service.
pub trait SocialService: Send + Sync {
    /// A social request (kinds 0x70-0x7F), answered on `who`'s control
    /// stream. The session's requests run one after another, in order.
    fn social(self: Arc<Self>, who: Member, msg: social::ToRealm) -> Job;

    /// A guild or friends request (kinds 0xF0-0xFF).
    fn guild(self: Arc<Self>, who: Member, msg: guild::ToRealm) -> Job;

    /// The session logged in, came back on a new connection or changed its
    /// character.
    fn seat(&self, who: &Member);

    /// The game the session is in or holds a seat in (`None`: the lobby).
    fn place(&self, session: SessionId, game: Option<u32>);

    /// The session ended.
    fn leave(&self, session: SessionId);

    /// What it serves, for the ready line's `caps`: `social`, `guilds`,
    /// `friends`.
    fn caps(&self) -> Vec<&'static str>;
}

/// The social config keys (contract 11; the rate limits of section 9).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SocialConfig {
    /// `Say` and `Whisper` lines per window, all rooms (`chat_lines`).
    pub chat_lines: u8,
    pub chat_window_s: u16,
    /// `Say` lines in Global per window, inside the limit above.
    pub global_lines: u8,
    pub global_window_s: u16,
    /// Rooms of at most this many members show joined and left lines.
    pub chat_join_lines_max: u16,
    pub dm_keep_days: u16,
    /// Undelivered DM lines one account may have waiting.
    pub dm_waiting_max: u16,
    pub guild_max_members: u16,
    pub friends_max: u16,
    pub ignore_max: u16,
}

impl Default for SocialConfig {
    fn default() -> Self {
        Self {
            chat_lines: 5,
            chat_window_s: 10,
            global_lines: 2,
            global_window_s: 30,
            chat_join_lines_max: 50,
            dm_keep_days: 30,
            dm_waiting_max: 200,
            guild_max_members: d2_accounts::social::GUILD_MAX_MEMBERS,
            friends_max: d2_accounts::social::FRIENDS_MAX,
            ignore_max: d2_accounts::social::IGNORE_MAX,
        }
    }
}

fn number<T>(value: &str, range: RangeInclusive<T>) -> Result<T, String>
where
    T: FromStr + PartialOrd + std::fmt::Display,
{
    value
        .parse::<T>()
        .ok()
        .filter(|v| range.contains(v))
        .ok_or_else(|| format!("a number in {}..={}", range.start(), range.end()))
}

impl SocialConfig {
    /// The keys it reads.
    pub const KEYS: &'static [&'static str] = &[
        "chat_lines",
        "chat_window_s",
        "global_lines",
        "global_window_s",
        "chat_join_lines_max",
        "dm_keep_days",
        "dm_waiting_max",
        "guild_max_members",
        "friends_max",
        "ignore_max",
    ];

    /// Sets `key` (one of [`SocialConfig::KEYS`]) from its config value.
    pub fn set(&mut self, key: &str, value: &str) -> Result<(), String> {
        match key {
            "chat_lines" => self.chat_lines = number(value, 1..=100)?,
            "chat_window_s" => self.chat_window_s = number(value, 1..=3600)?,
            "global_lines" => self.global_lines = number(value, 1..=100)?,
            "global_window_s" => self.global_window_s = number(value, 1..=3600)?,
            "chat_join_lines_max" => self.chat_join_lines_max = number(value, 0..=10_000)?,
            "dm_keep_days" => self.dm_keep_days = number(value, 1..=3650)?,
            "dm_waiting_max" => self.dm_waiting_max = number(value, 1..=10_000)?,
            "guild_max_members" => self.guild_max_members = number(value, 2..=1000)?,
            "friends_max" => self.friends_max = number(value, 1..=1000)?,
            "ignore_max" => self.ignore_max = number(value, 1..=1000)?,
            _ => return Err(format!("not a social key: {key}")),
        }
        Ok(())
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

/// The social clock: the system's, moved forward on a test server
/// (`social-clock`), so DMs and invites expire without waiting days.
#[derive(Debug, Default)]
pub struct Clock {
    offset_ms: AtomicI64,
}

impl Clock {
    pub fn now(&self) -> i64 {
        now_ms() + self.offset_ms.load(Ordering::Relaxed)
    }

    /// Moves the clock `ms` forward; the new offset.
    pub fn advance(&self, ms: i64) -> i64 {
        self.offset_ms.fetch_add(ms, Ordering::Relaxed) + ms
    }
}

/// The realm's social service.
pub struct Social {
    cfg: SocialConfig,
    accounts: Accounts,
    /// The realm's mod game (`Hello.game`) and its label ("D2Gamma").
    mod_id: String,
    mod_label: String,
    presence: Presence,
    clock: Clock,
    /// `limits = test`: every window divided by 60.
    test_windows: bool,
    rooms: Mutex<Rooms>,
    limits: Mutex<ChatLimits>,
    guild_limits: Mutex<guilds::GuildLimits>,
    friends: friends::Friends,
    log: Arc<ServerLog>,
}

impl std::fmt::Debug for Social {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Social")
            .field("mod_id", &self.mod_id)
            .field("sessions", &self.presence.len())
            .finish_non_exhaustive()
    }
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// A line's text as the realm takes it: trimmed, 1 to [`MAX_CHAT`] bytes,
/// no control characters (`BadText`).
pub fn line_text(text: &str) -> Result<String, Denied> {
    let t = text.trim();
    if t.is_empty() || t.len() > MAX_CHAT || t.chars().any(char::is_control) {
        return Err(Refusal::BadText.into());
    }
    Ok(t.to_owned())
}

/// Milliseconds as the wire's `u64`.
fn wire_ms(ms: i64) -> u64 {
    u64::try_from(ms).unwrap_or(0)
}

/// Whole seconds from `now_ms` to `until_ms`, at least 1.
fn wait_s(now_ms: i64, until_ms: i64) -> u32 {
    let ms = (until_ms - now_ms).max(1);
    u32::try_from((ms + 999) / 1000).unwrap_or(u32::MAX)
}

/// What a `SocialState` says but the granted rooms and the `req`.
#[derive(Clone, Debug)]
struct StateBase {
    view: AccountView,
    dm_unread: (u16, u16),
    friends_online: u16,
    guild_invites: u16,
    muted_until_ms: Option<i64>,
}

impl Social {
    /// The service of the realm serving `mod_id` (shown as `mod_label`).
    pub fn new(
        cfg: SocialConfig,
        accounts: Accounts,
        mod_id: &str,
        mod_label: &str,
        test_windows: bool,
        log: Arc<ServerLog>,
    ) -> Arc<Self> {
        Arc::new_cyclic(|me| Self {
            cfg,
            accounts,
            mod_id: mod_id.to_owned(),
            mod_label: mod_label.to_owned(),
            presence: Presence::default(),
            clock: Clock::default(),
            test_windows,
            rooms: Mutex::new(Rooms::new(cfg.chat_join_lines_max)),
            limits: Mutex::new(ChatLimits::new(&cfg, test_windows)),
            guild_limits: Mutex::new(guilds::GuildLimits::new(test_windows)),
            friends: friends::Friends::new(test_windows, me.clone()),
            log,
        })
    }

    pub fn config(&self) -> &SocialConfig {
        &self.cfg
    }

    pub fn accounts(&self) -> &Accounts {
        &self.accounts
    }

    pub fn presence(&self) -> &Presence {
        &self.presence
    }

    pub fn clock(&self) -> &Clock {
        &self.clock
    }

    /// The friends' part: the lobby's games for it
    /// ([`friends::Friends::set_games`]).
    pub fn friends(&self) -> &friends::Friends {
        &self.friends
    }

    pub fn mod_id(&self) -> &str {
        &self.mod_id
    }

    pub fn mod_label(&self) -> &str {
        &self.mod_label
    }

    /// Whether the rate-limit windows are the test ones (divided by 60).
    pub fn test_windows(&self) -> bool {
        self.test_windows
    }

    pub fn log_line(&self, line: &str) {
        self.log.line(&format!("social: {line}"));
    }

    /// Answers request `req` of kind `kind` with `SocialDone`: done, or
    /// the refusal with its code, wait and words.
    pub fn done(out: &ControlTx, req: u32, kind: u8, r: Result<(), Denied>) {
        out.send(social::FromRealm::Done(done_of(req, kind, r)).body());
    }

    /// The account's sessions that sent a `SocialHello` (the ones that get
    /// the pushes).
    pub fn hello_sessions(&self, account: AccountId) -> Vec<Here> {
        let rooms = lock(&self.rooms);
        self.presence
            .sessions_of(account)
            .into_iter()
            .filter(|h| rooms.said_hello(h.session))
            .collect()
    }

    /// A list request: the shared list limit (contract 9).
    pub fn list_limit(&self, account: AccountId) -> Result<(), Denied> {
        lock(&self.limits).list(account.0, Instant::now())
    }

    /// Refused with `Muted` while the account is muted.
    async fn check_mute(&self, account: AccountId, now_ms: i64) -> Result<(), Denied> {
        let a = account.0;
        let mute = self
            .accounts
            .store(move |db| Ok(db.mutes().get(a, now_ms)?))
            .await?;
        match mute {
            Some(m) => Err(Denied::wait(ErrorCode::MUTED, wait_s(now_ms, m.until_ms))),
            None => Ok(()),
        }
    }

    /// What the rooms and the state need of the account, from the store.
    async fn load(&self, account: AccountId) -> Result<StateBase, Denied> {
        let a = account.0;
        let now = self.clock.now();
        let (guild, ignores, privacy, unread, friends, invites, mute) = self
            .accounts
            .store(move |db| {
                let guild = match db.guilds().membership(a)? {
                    Some((id, rank)) => db.guilds().get(id)?.map(|g| GuildBrief {
                        guild: u64::try_from(g.id).unwrap_or(0),
                        tag: g.tag,
                        name: g.name,
                        my_rank: rank.as_u8(),
                    }),
                    None => None,
                };
                let friends: Vec<i64> = db
                    .friends()
                    .list_friends(a)?
                    .into_iter()
                    .map(|f| f.account)
                    .collect();
                Ok((
                    guild,
                    db.friends().ignored_ids(a)?,
                    db.prefs().get(a)?,
                    db.dms().unread(a)?,
                    friends,
                    db.guilds().invites(a, now)?.len(),
                    db.mutes().get(a, now)?,
                ))
            })
            .await?;
        let online = friends
            .into_iter()
            .filter(|f| self.presence.online(AccountId(*f)))
            .count();
        Ok(StateBase {
            view: AccountView {
                guild,
                ignores: ignores.into_iter().collect(),
                whispers: match privacy.whispers {
                    Whispers::Everyone => social::whispers::EVERYONE,
                    Whispers::FriendsAndGuild => social::whispers::FRIENDS_AND_GUILD,
                    Whispers::Nobody => social::whispers::NOBODY,
                },
                show_where: match privacy.show_where {
                    ShowWhere::MutualFriends => social::show_where::MUTUAL_FRIENDS,
                    ShowWhere::Nobody => social::show_where::NOBODY,
                },
            },
            dm_unread: unread,
            friends_online: u16::try_from(online).unwrap_or(u16::MAX),
            guild_invites: u16::try_from(invites).unwrap_or(u16::MAX),
            muted_until_ms: mute.map(|m| m.until_ms),
        })
    }

    /// The `SocialState` of `base` for a session granted `rooms`.
    fn state(&self, base: &StateBase, req: u32, rooms: u16) -> Vec<u8> {
        FromRealm::State(SocialState {
            req,
            rooms,
            mod_id: self.mod_id.clone(),
            mod_label: self.mod_label.clone(),
            guild: base.view.guild.clone(),
            dm_unread: base.dm_unread.0,
            dm_conversations_unread: base.dm_unread.1,
            friends_online: base.friends_online,
            guild_invites: base.guild_invites,
            whispers: base.view.whispers,
            show_where: base.view.show_where,
            limits: StateLimits {
                say_lines: self.cfg.chat_lines,
                say_window_s: self.cfg.chat_window_s,
                global_lines: self.cfg.global_lines,
                global_window_s: self.cfg.global_window_s,
            },
            muted_until_ms: base.muted_until_ms.map(wire_ms),
            now_ms: wire_ms(self.clock.now()),
        })
        .body()
    }

    /// Reads the account again from the store (its guild, ignores,
    /// privacy, counts, mute): its sessions' rooms follow and each session
    /// with a hello gets its `SocialState` (`req` 0). The friends and the
    /// guilds call it after they changed the store.
    pub async fn reload(&self, account: AccountId) {
        if self.hello_sessions(account).is_empty() {
            return;
        }
        let base = match self.load(account).await {
            Ok(b) => b,
            Err(d) => return self.log_line(&format!("{account}: reload: {}", d.text)),
        };
        let now = wire_ms(self.clock.now());
        let mut rooms = lock(&self.rooms);
        rooms.set_view(&self.presence, account.0, base.view.clone(), now);
        for h in self.presence.sessions_of(account) {
            if rooms.said_hello(h.session) {
                h.out.send(self.state(&base, 0, rooms.granted(h.session)));
            }
        }
    }

    /// The account's guild changed (joined, left, kicked, disbanded,
    /// rank): its sessions' Guild room and `SocialState` follow. Called by
    /// the guilds after the store changed.
    pub async fn guild_changed(&self, account: AccountId) {
        self.reload(account).await;
    }

    /// A guild event line (`what` 5: joined, left, kicked, promoted,
    /// demoted, motd) in guild `guild`'s room, about `who`.
    pub fn guild_event(&self, guild: u64, who: Who, text: String) {
        let now = wire_ms(self.clock.now());
        lock(&self.rooms).add_line(
            &self.presence,
            RoomKey::Guild(guild),
            line_what::GUILD_EVENT,
            who,
            text,
            now,
            None,
        );
    }

    /// `SocialHello`: the state, the backlog, the rooms, then the DMs that
    /// waited.
    async fn hello(&self, who: &Member, req: u32, asked: u16) -> Result<(), Denied> {
        let base = self.load(who.account).await?;
        let Some(here) = self.presence.get(who.session) else {
            return Ok(());
        };
        let granted = Rooms::grant(asked, &base.view);
        let state = self.state(&base, req, granted);
        let now = wire_ms(self.clock.now());
        lock(&self.rooms).hello(&self.presence, &here, asked, base.view, state, now);
        self.deliver_waiting(who.account).await;
        Ok(())
    }

    /// `Say` in room `number`.
    async fn say(&self, who: &Member, number: u8, emote: bool, text: &str) -> Result<(), Denied> {
        let here = self
            .presence
            .get(who.session)
            .ok_or(ErrorCode::NOT_LOGGED_IN)?;
        if here.character.is_none() {
            return Err(ErrorCode::NO_CHAR_SELECTED.into());
        }
        let account = who.account.0;
        let key = {
            let rooms = lock(&self.rooms);
            match rooms.joined_key(who.session, account, number) {
                Some(k) => k,
                None if number == room::GUILD
                    && rooms.view(account).is_some_and(|v| v.guild.is_none()) =>
                {
                    return Err(ErrorCode::NOT_IN_GUILD.into())
                }
                None => return Err(ErrorCode::ROOM_UNAVAILABLE.into()),
            }
        };
        let text = line_text(text)?;
        let now = self.clock.now();
        self.check_mute(who.account, now).await?;
        lock(&self.limits).say(account, number, &text, Instant::now())?;
        let here = self.presence.get(who.session).unwrap_or(here);
        lock(&self.rooms).say(&self.presence, &here, key, emote, text, wire_ms(now));
        Ok(())
    }

    /// The old lobby chat (0x49) of a session: said in its Mod room, by
    /// the room's rules, whether or not it sent a hello.
    pub async fn legacy_say(&self, session: SessionId, text: &str) -> Result<(), Denied> {
        let here = self.presence.get(session).ok_or(ErrorCode::NOT_LOGGED_IN)?;
        if here.character.is_none() {
            return Err(ErrorCode::NO_CHAR_SELECTED.into());
        }
        let text = line_text(text)?;
        let now = self.clock.now();
        self.check_mute(here.account, now).await?;
        lock(&self.limits).say(here.account.0, room::MOD, &text, Instant::now())?;
        lock(&self.rooms).say(
            &self.presence,
            &here,
            RoomKey::Mod,
            false,
            text,
            wire_ms(now),
        );
        Ok(())
    }

    /// `UserList` of room `number`; `watch` keeps it live.
    async fn user_list(
        &self,
        who: &Member,
        req: u32,
        number: u8,
        watch: bool,
    ) -> Result<(), Denied> {
        self.list_limit(who.account)?;
        let mut rooms = lock(&self.rooms);
        let key = rooms
            .joined_key(who.session, who.account.0, number)
            .ok_or(ErrorCode::ROOM_UNAVAILABLE)?;
        if watch {
            rooms.watch(who.session, Some(key));
        } else if rooms.watched(who.session) == Some(key) {
            rooms.watch(who.session, None);
        }
        let rows = rooms.users(key);
        drop(rooms);
        who.out.send(
            FromRealm::ListBegin(ListHead {
                req,
                what: social::kind::USER_LIST,
                total: u32::try_from(rows.len()).unwrap_or(u32::MAX),
                page: 0,
                rows: u16::try_from(rows.len()).unwrap_or(u16::MAX),
                now_ms: wire_ms(self.clock.now()),
            })
            .body(),
        );
        for (w, place) in rows {
            who.out
                .send(FromRealm::UserRow { req, who: w, place }.body());
        }
        who.out.send(FromRealm::ListEnd { req }.body());
        Ok(())
    }

    /// The console's `notice TEXT`: a realm notice line in Global.
    pub fn notice(&self, text: &str) -> Result<String, String> {
        let text = line_text(text).map_err(|d| d.text)?;
        let now = wire_ms(self.clock.now());
        lock(&self.rooms).add_line(
            &self.presence,
            RoomKey::Global,
            line_what::REALM_NOTICE,
            Who {
                class: social::NO_CLASS,
                ..Who::default()
            },
            text,
            now,
            None,
        );
        self.log_line("notice sent to Global");
        Ok("notice sent".into())
    }

    /// The console's `mute ACCOUNT MINUTES [REASON]`.
    pub async fn mute(&self, account: &str, minutes: u32, reason: &str) -> Result<String, String> {
        let id = self
            .accounts
            .account_id(account)?
            .ok_or_else(|| format!("no account named `{account}`"))?;
        let until = self.clock.now() + i64::from(minutes) * 60_000;
        let reason = reason.to_owned();
        self.accounts
            .store(move |db| Ok(db.mutes().mute(id.0, until, &reason)?))
            .await
            .map_err(|d| d.text)?;
        self.log_line(&format!("{id}: muted for {minutes} min"));
        self.reload(id).await;
        Ok(format!("{account} muted for {minutes} min"))
    }

    /// The console's `unmute ACCOUNT`.
    pub async fn unmute(&self, account: &str) -> Result<String, String> {
        let id = self
            .accounts
            .account_id(account)?
            .ok_or_else(|| format!("no account named `{account}`"))?;
        let was = self
            .accounts
            .store(move |db| Ok(db.mutes().unmute(id.0)?))
            .await
            .map_err(|d| d.text)?;
        if !was {
            return Err(format!("{account} is not muted"));
        }
        self.log_line(&format!("{id}: unmuted"));
        self.reload(id).await;
        Ok(format!("{account} unmuted"))
    }

    /// What the service holds in memory, for the realm's `stats` line:
    /// the sessions it knows, the rooms' and the limits' counts.
    pub fn stats(&self) -> String {
        format!(
            "social={} {} {}",
            self.presence.len(),
            lock(&self.rooms).stats(),
            lock(&self.limits).stats()
        )
    }

    /// The hourly sweep: DMs past `dm_keep_days`, expired mutes and guild
    /// invites, the repeat rule's old texts. How many DM lines went.
    pub async fn sweep(&self) -> Result<usize, String> {
        let now = self.clock.now();
        let keep = self.cfg.dm_keep_days;
        lock(&self.limits).prune(Instant::now());
        let (dms, mutes, invites) = self
            .accounts
            .store(move |db| {
                Ok((
                    db.dms().sweep(now, keep)?,
                    db.mutes().sweep(now)?,
                    db.guilds().sweep_invites(now)?,
                ))
            })
            .await
            .map_err(|d| d.text)?;
        if dms + mutes + invites > 0 {
            self.log_line(&format!(
                "swept {dms} DM lines, {mutes} mutes, {invites} guild invites"
            ));
        }
        Ok(dms)
    }

    /// The console's `social-clock +N`: the clock forward, then the
    /// sweep. The answer's text.
    pub async fn advance_clock(&self, ms: i64) -> Result<String, String> {
        let offset = self.clock.advance(ms);
        let dms = self.sweep().await?;
        Ok(format!(
            "social clock +{} s; swept {dms} DM lines",
            offset / 1000
        ))
    }

    async fn handle(&self, who: Member, msg: social::ToRealm) {
        let (req, kind) = (msg.req, msg.kind());
        let r = match msg.ask {
            Ask::Hello { rooms } => self.hello(&who, req, rooms).await,
            Ask::Say { room, emote, text } => {
                let r = self.say(&who, room, emote, &text).await;
                return Self::done(&who.out, req, kind, r);
            }
            Ask::Whisper { to, text } => {
                let done = match self.whisper(&who, req, &to, &text).await {
                    Ok(d) => d,
                    Err(e) => done_of(req, kind, Err(e)),
                };
                return who.out.send(FromRealm::Done(done).body());
            }
            Ask::DmConversations { page } => self.conversations(&who, req, page).await,
            Ask::DmHistory {
                peer,
                before_dm,
                rows,
            } => self.history(&who, req, &peer, before_dm, rows).await,
            Ask::DmRead {
                peer_account,
                up_to_dm,
            } => {
                let r = self.dm_read(&who, &peer_account, up_to_dm).await;
                return Self::done(&who.out, req, kind, r);
            }
            Ask::UserList { room, watch } => self.user_list(&who, req, room, watch).await,
        };
        // The others answer with their data; a refusal with `SocialDone`.
        if let Err(e) = r {
            Self::done(&who.out, req, kind, Err(e));
        }
    }
}

/// `SocialDone` for request `req` of kind `kind`.
pub fn done_of(req: u32, kind: u8, r: Result<(), Denied>) -> Done {
    match r {
        Ok(()) => Done::ok(req, kind),
        Err(d) => {
            let mut done = Done::refused(req, kind, d.code);
            done.wait_s = d.wait_s;
            done.text = d.text;
            done
        }
    }
}

impl SocialService for Social {
    fn social(self: Arc<Self>, who: Member, msg: social::ToRealm) -> Job {
        Box::pin(async move { self.handle(who, msg).await })
    }

    fn guild(self: Arc<Self>, who: Member, msg: guild::ToRealm) -> Job {
        Box::pin(async move {
            let kind = msg.kind();
            if d2_net::msg::kind::FRIENDS.contains(&kind) {
                friends::handle(&self, &who, msg).await;
            } else {
                guilds::handle(&self, &who, msg).await;
            }
        })
    }

    fn seat(&self, who: &Member) {
        if let Some(ch) = self.presence.seat(who) {
            let now = wire_ms(self.clock.now());
            lock(&self.rooms).changed(&self.presence, who.account.0, now);
            friends::presence_changed(self, &ch);
        }
    }

    fn place(&self, session: SessionId, game: Option<u32>) {
        if let Some(ch) = self.presence.set_game(session, game) {
            let now = wire_ms(self.clock.now());
            lock(&self.rooms).changed(&self.presence, ch.here.account.0, now);
            friends::presence_changed(self, &ch);
        }
    }

    fn leave(&self, session: SessionId) {
        if let Some(ch) = self.presence.leave(session) {
            let now = wire_ms(self.clock.now());
            lock(&self.rooms).leave(&self.presence, session, ch.here.account.0, now);
            friends::presence_changed(self, &ch);
        }
    }

    fn caps(&self) -> Vec<&'static str> {
        let mut caps = vec!["social"];
        caps.extend(friends::CAPS);
        caps.extend(guilds::CAPS);
        caps
    }
}

#[cfg(test)]
mod tests;
