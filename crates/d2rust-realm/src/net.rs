//! The network side: the QUIC endpoint's connections and their sessions
//! (contracts sections 3 and 6).
//!
//! Each connection's control stream opens with the handshake
//! ([`ClientHello`], checked against this server: protocol, game, build,
//! tick rate; then the session limits), answered with [`SessionWelcome`]
//! or [`Refused`]. Then it carries the session's requests:
//!
//! - kinds 0x20-0x3F: accounts and characters, answered here on the
//!   [`Accounts`] API (a new login to an account that has a session takes
//!   it over: the old one gets `SessionTaken`, its character leaves its
//!   game saved, and it is closed);
//! - kinds 0x50-0x57: the ladder ([`Ladder::request`]), once logged in;
//! - kinds 0x40-0x5F: the lobby ([`LobbyApi::handle`]), once a character
//!   is selected;
//! - kinds 0xA0-0xAF: the online shared stash ([`Realm::stash_request`]),
//!   each on its own task (a move waits for its game server);
//! - kinds 0x70-0x7F, 0xF0-0xF7 and 0xF8-0xFF: the realm's chat, guilds
//!   and friends ([`SocialService`]), in the session's order;
//! - anything else: `NotSupported`; a request that doesn't decode is
//!   `Malformed` and ends the connection.
//!
//! The main server runs no games: its game servers do, and a client plays
//! on a connection of its own to the game server (`JoinNode`). A game
//! stream (`Attach`) here is refused with `NotSupported`. What the game
//! servers' games report updates the sessions and the lobby
//! ([`Net::run_news`]).
//!
//! A lost connection of a logged-in session keeps the session for the
//! grace period, so a client that comes back with the session token in its
//! hello finds it as it was ([`ResumeState`]).
//!
//! The same UDP port takes the game servers' links (ALPN `d2rust-node/1`,
//! contracts 11.2): [`Net::run`] hands those to the [`Realm`].
//!
//! From the one-process server multiplayer v1 had, without its games.

use std::collections::BTreeMap;
use std::net::IpAddr;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use d2_net::msg::account::{AccountEvent, AccountRequest};
use d2_net::msg::ladder::ToRealm as LadderRequest;
use d2_net::msg::market::ToRealm;
use d2_net::msg::stash::{StashRequest, KINDS as STASH_KINDS};
use d2_net::msg::{guild as guild_msg, social as social_msg};
use d2_net::msg::{kind as msg_kind, ErrorCode};
use d2_net::wire::{ClientHello, Refused, ResumeState, ServerInfo, SessionWelcome};
use d2_net::ResumeToken;
use d2_quic::{FramedStream, QuicConn, QuicServer};
use tokio::sync::{mpsc, Mutex as AsyncMutex};

use crate::accounts::{Account, AccountId, Accounts, NewChar};
use crate::ladder::{Asker, Ladder};
use crate::log::ServerLog;
use crate::market::{MarketService, Who};
use crate::realm::games::GameNews;
use crate::realm::stash::StashUser;
use crate::realm::Realm;
use crate::session::{ControlOut, ControlTx, LobbyApi, SessionCtx, SessionId, Sessions};
use crate::social::{Member, SocialService};

/// How long a client has for its hello, and a game stream for its attach.
pub const HANDSHAKE_WAIT: Duration = Duration::from_secs(10);

/// How long a connection stays open after its last frame (a refusal, a
/// goodbye) so the frame arrives: closing a QUIC connection drops what it
/// hasn't delivered yet.
pub const CLOSE_GRACE: Duration = Duration::from_secs(1);

/// Sessions not logged in yet, per address (contracts section 5).
pub const UNAUTHENTICATED_PER_IP: usize = 4;

/// What the network side runs with.
#[derive(Clone, Debug)]
pub struct NetConfig {
    /// What a client's hello must match.
    pub server: ServerInfo,
    pub server_name: String,
    /// The most players of one game (for the welcome).
    pub max_players: u32,
    pub max_sessions: usize,
    /// How long a lost session is kept for its client to come back.
    pub grace: Duration,
}

type Ctx = Arc<AsyncMutex<SessionCtx>>;

/// The network side of the server.
pub struct Net<L: LobbyApi> {
    cfg: NetConfig,
    accounts: Accounts,
    lobby: Arc<L>,
    sessions: Sessions,
    log: Arc<ServerLog>,
    state: Mutex<State>,
    next_id: AtomicU64,
    /// The most frames one session's control stream had queued.
    out_peak: AtomicUsize,
    /// The main server's realm, for the online stash's requests.
    stash: Option<Arc<Realm>>,
    /// The market's requests (kinds 0xB0-0xCF).
    market: Option<Arc<dyn MarketService>>,
    /// The ladder's requests (kinds 0x50-0x57) and rows.
    ladder: Option<Arc<Ladder>>,
    /// The chat, guilds and friends (kinds 0x70-0x7F, 0xF0-0xFF).
    social: Option<Arc<dyn SocialService>>,
}

#[derive(Default)]
struct State {
    /// Every session's context, open or held.
    ctxs: BTreeMap<SessionId, Ctx>,
    /// Session tokens, for a hello's resume.
    tokens: BTreeMap<[u8; 16], SessionId>,
    /// Sessions whose connection was lost, with when they end.
    held: BTreeMap<SessionId, Instant>,
    /// Sessions not logged in, per address.
    unauthenticated: BTreeMap<IpAddr, usize>,
    /// The account's session: claimed atomically by a login, so two
    /// logins at once cannot both keep it.
    by_account: BTreeMap<AccountId, SessionId>,
}

/// What [`Net::stats`] reports.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NetStats {
    pub open: usize,
    pub ctxs: usize,
    pub held: usize,
    pub tokens: usize,
    pub by_account: usize,
    pub unauthenticated: usize,
    pub out_peak: usize,
}

/// A refused hello or attach.
type Refusal = Refused;

impl<L: LobbyApi> Net<L> {
    pub fn new(
        cfg: NetConfig,
        accounts: Accounts,
        lobby: Arc<L>,
        sessions: Sessions,
        log: Arc<ServerLog>,
    ) -> Self {
        Self {
            cfg,
            accounts,
            lobby,
            sessions,
            log,
            state: Mutex::new(State::default()),
            next_id: AtomicU64::new(1),
            out_peak: AtomicUsize::new(0),
            stash: None,
            market: None,
            ladder: None,
            social: None,
        }
    }

    /// The sessions it holds, for the `stats` line: open connections,
    /// contexts (open or held), held ones, resume tokens, logins per
    /// account, addresses with sessions not logged in, and the most frames
    /// one control stream had queued.
    pub fn stats(&self) -> NetStats {
        let st = self.state();
        NetStats {
            open: self.sessions.len(),
            ctxs: st.ctxs.len(),
            held: st.held.len(),
            tokens: st.tokens.len(),
            by_account: st.by_account.len(),
            unauthenticated: st.unauthenticated.values().sum(),
            out_peak: self.out_peak.load(Ordering::Relaxed),
        }
    }

    /// Serves the chat, guilds and friends with `social`.
    pub fn with_social(mut self, social: Arc<dyn SocialService>) -> Self {
        self.social = Some(social);
        self
    }

    /// The social service's view of a logged-in session.
    fn member(c: &SessionCtx) -> Option<Member> {
        let (account, name) = c.account.as_ref()?;
        Some(Member {
            session: c.id,
            account: *account,
            account_name: name.clone(),
            character: c.character.clone(),
            out: c.out.clone(),
        })
    }

    /// The session logged in, came back or changed its character: the
    /// social service's presence follows.
    fn social_seat(&self, c: &SessionCtx) {
        if let (Some(s), Some(m)) = (&self.social, Self::member(c)) {
            s.seat(&m);
        }
    }

    /// The game session `id` is in now, for the social service.
    fn social_place(&self, id: SessionId) {
        if let Some(s) = &self.social {
            s.place(id, self.lobby.session_game(id));
        }
    }

    /// Serves the ladder's requests with `ladder`, and writes its rows
    /// after saves and deletes.
    pub fn with_ladder(mut self, ladder: Arc<Ladder>) -> Self {
        self.ladder = Some(ladder);
        self
    }

    /// Serves the market's requests with `market`.
    pub fn with_market(mut self, market: Arc<dyn MarketService>) -> Self {
        self.market = Some(market);
        self
    }

    /// The market's view of a logged-in session.
    fn market_who(c: &SessionCtx) -> Option<Who> {
        let (account, _) = c.account.as_ref()?;
        Some(Who {
            session: c.id,
            account: *account,
            character: c.character.clone(),
            out: c.out.clone(),
        })
    }

    /// The session's character changed: its market seat follows (where
    /// its notices go).
    fn market_seat(&self, c: &SessionCtx) {
        if let (Some(m), Some(who)) = (&self.market, Self::market_who(c)) {
            m.seat(&who);
        }
        self.social_seat(c);
    }

    /// Serves the stash's requests with `realm`'s online stash.
    pub fn with_stash(mut self, realm: Arc<Realm>) -> Self {
        self.stash = Some(realm);
        self
    }

    fn state(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Open sessions (not counting held ones).
    pub fn session_count(&self) -> usize {
        self.sessions.len()
    }

    /// Accepts connections until the endpoint closes: players' here, game
    /// servers' to `realm`.
    pub async fn run(self: Arc<Self>, quic: Arc<QuicServer>, realm: Arc<Realm>) {
        while let Some((conn, control)) = quic.accept().await {
            if conn.is_player() {
                let me = self.clone();
                tokio::spawn(async move { me.serve(conn, control).await });
            } else {
                tokio::spawn(realm.clone().serve_node(conn, control));
            }
        }
    }

    /// Takes the game servers' news until the realm is gone: a stored save
    /// updates its character's summary in its sessions, then the lobby
    /// takes it.
    pub async fn run_news(self: Arc<Self>, mut rx: mpsc::UnboundedReceiver<GameNews>) {
        while let Some(news) = rx.recv().await {
            if let GameNews::Saved { account, summary } = &news {
                if let Some(l) = &self.ladder {
                    l.saved(*account, &summary.name);
                }
                let ctxs: Vec<Ctx> = self.state().ctxs.values().cloned().collect();
                for ctx in ctxs {
                    let mut c = ctx.lock().await;
                    let mine = c.account.as_ref().is_some_and(|(a, _)| a == account);
                    if let Some(s) = c
                        .character
                        .as_mut()
                        .filter(|s| mine && s.name.eq_ignore_ascii_case(&summary.name))
                    {
                        *s = summary.clone();
                    }
                }
            }
            self.lobby.on_news(news);
            if self.social.is_some() {
                let ids: Vec<SessionId> = self.state().ctxs.keys().copied().collect();
                for id in ids {
                    self.social_place(id);
                }
            }
        }
    }

    /// Ends the held sessions whose grace period ran out. The server calls
    /// it every second.
    pub async fn expire_held(&self) {
        let now = Instant::now();
        let over: Vec<SessionId> = self
            .state()
            .held
            .iter()
            .filter(|(_, &until)| until <= now)
            .map(|(&id, _)| id)
            .collect();
        for id in over {
            self.end_session(id, "its connection was lost").await;
        }
    }

    /// One connection: the handshake, then the session.
    async fn serve(self: Arc<Self>, conn: QuicConn, mut control: FramedStream) {
        let ip = conn.peer().ip();
        let hello = match tokio::time::timeout(HANDSHAKE_WAIT, control.recv_async()).await {
            Ok(Ok(body)) => body,
            _ => {
                conn.close(0, "no hello");
                return;
            }
        };
        let opened =
            match ClientHello::parse(&hello).and_then(|h| h.check(&self.cfg.server).map(|()| h)) {
                Ok(h) => self.open_session(ip, h.resume).await,
                Err(r) => Err(r),
            };
        let (ctx, rx, welcome) = match opened {
            Ok(x) => x,
            Err(r) => {
                let _ = control.send(r.body());
                control.close();
                close_later(conn, "refused");
                return;
            }
        };
        let id = ctx.lock().await.id;
        if control.send(welcome.body()).is_err() {
            self.lose_session(id).await;
            return;
        }
        let streams = {
            let me = self.clone();
            let (conn, ctx) = (conn.clone(), ctx.clone());
            tokio::spawn(async move { me.game_streams(conn, ctx).await })
        };
        let clean = self.session_loop(&ctx, &mut control, rx).await;
        streams.abort();
        if clean {
            control.close();
            close_later(conn, "bye");
            self.end_session(id, "logged out").await;
        } else {
            self.lose_session(id).await;
        }
    }

    /// Reads requests and writes answers until the session ends. True when
    /// it ended cleanly (logout, takeover, malformed request), false when
    /// the connection was lost.
    async fn session_loop(
        &self,
        ctx: &Ctx,
        control: &mut FramedStream,
        mut rx: mpsc::UnboundedReceiver<ControlOut>,
    ) -> bool {
        loop {
            tokio::select! {
                got = control.recv_async() => match got {
                    Ok(body) => self.dispatch(ctx, body).await,
                    Err(_) => return false,
                },
                out = rx.recv() => match out {
                    Some(ControlOut::Body(b)) => {
                        self.out_peak.fetch_max(rx.len() + 1, Ordering::Relaxed);
                        if control.send(b).is_err() {
                            return false;
                        }
                    }
                    Some(ControlOut::Close) | None => {
                        // Send what was queued before the close.
                        while let Ok(ControlOut::Body(b)) = rx.try_recv() {
                            let _ = control.send(b);
                        }
                        return true;
                    }
                },
            }
        }
    }

    /// A new session, or a held one taken back with its token.
    #[allow(clippy::type_complexity)]
    async fn open_session(
        &self,
        ip: IpAddr,
        resume: Option<ResumeToken>,
    ) -> Result<(Ctx, mpsc::UnboundedReceiver<ControlOut>, SessionWelcome), Refusal> {
        let token = ResumeToken::random();
        // A held session comes back with its state.
        let held = {
            let mut st = self.state();
            match resume.and_then(|t| st.tokens.get(&t.0).copied()) {
                Some(id) if st.held.remove(&id).is_some() => {
                    st.tokens.retain(|_, s| *s != id);
                    st.tokens.insert(token.0, id);
                    st.ctxs.get(&id).cloned().map(|ctx| (id, ctx))
                }
                _ => None,
            }
        };
        if let Some((id, ctx)) = held {
            {
                let (tx, rx) = ControlTx::new(id);
                self.sessions.insert(tx.clone());
                let mut c = ctx.lock().await;
                c.out = tx;
                c.ip = ip;
                let state = c.account.as_ref().map(|(_, name)| ResumeState {
                    account: name.clone(),
                    character: c.character.as_ref().map(|s| s.name.clone()),
                    game_id: self.lobby.session_game(id),
                });
                // Pushes go to the new connection from now on.
                self.social_seat(&c);
                drop(c);
                self.log
                    .line(&format!("session {id}: back from {ip} (resumed)"));
                return Ok((ctx, rx, self.welcome(token, state)));
            }
        }
        let mut st = self.state();
        if self.sessions.len() >= self.cfg.max_sessions {
            return Err(Refused::new(ErrorCode::SERVER_FULL));
        }
        let waiting = st.unauthenticated.get(&ip).copied().unwrap_or(0);
        if waiting >= UNAUTHENTICATED_PER_IP {
            return Err(Refused::with_text(
                ErrorCode::SERVER_FULL,
                "Too many connections from your address. Try again later.",
            ));
        }
        *st.unauthenticated.entry(ip).or_default() += 1;
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = ControlTx::new(id);
        let ctx = Arc::new(AsyncMutex::new(SessionCtx {
            id,
            ip,
            account: None,
            character: None,
            game: None,
            out: tx.clone(),
        }));
        st.ctxs.insert(id, ctx.clone());
        st.tokens.insert(token.0, id);
        drop(st);
        self.sessions.insert(tx);
        Ok((ctx, rx, self.welcome(token, None)))
    }

    fn welcome(&self, session: ResumeToken, resume: Option<ResumeState>) -> SessionWelcome {
        SessionWelcome {
            tick_rate: self.cfg.server.tick_rate,
            max_players: self.cfg.max_players,
            session,
            server_name: self.cfg.server_name.clone(),
            registration_open: self.accounts.config().registration_open,
            resume,
        }
    }

    /// A lost connection: a logged-in session is held for the grace
    /// period; any other ends now.
    async fn lose_session(&self, id: SessionId) {
        self.sessions.remove(id);
        let Some(ctx) = self.state().ctxs.get(&id).cloned() else {
            return;
        };
        let logged_in = ctx.lock().await.account.is_some();
        if logged_in && !self.cfg.grace.is_zero() {
            self.state()
                .held
                .insert(id, Instant::now() + self.cfg.grace);
        } else {
            self.end_session(id, "its connection was lost").await;
        }
    }

    /// Ends a session for good: its character leaves its game (saved), the
    /// lobby forgets it.
    async fn end_session(&self, id: SessionId, why: &str) {
        self.sessions.remove(id);
        let ctx = {
            let mut st = self.state();
            st.held.remove(&id);
            st.tokens.retain(|_, s| *s != id);
            st.ctxs.remove(&id)
        };
        let Some(ctx) = ctx else {
            return;
        };
        let c = ctx.lock().await;
        if c.account.is_none() {
            self.unauthenticated_done(c.ip);
        }
        if let Some((account, _)) = &c.account {
            let mut st = self.state();
            if st.by_account.get(account) == Some(&id) {
                st.by_account.remove(account);
            }
        }
        self.lobby.session_ended(&c);
        if let Some(m) = &self.market {
            m.leave(id);
        }
        if let Some(s) = &self.social {
            s.leave(id);
        }
        if let Some((_, name)) = &c.account {
            self.log
                .line(&format!("session {id} ({name}) ended: {why}"));
        }
    }

    fn unauthenticated_done(&self, ip: IpAddr) {
        let mut st = self.state();
        if let Some(n) = st.unauthenticated.get_mut(&ip) {
            *n = n.saturating_sub(1);
            if *n == 0 {
                st.unauthenticated.remove(&ip);
            }
        }
    }

    /// One request of a session.
    async fn dispatch(&self, ctx: &Ctx, body: Vec<u8>) {
        let kind = body[0];
        if msg_kind::ACCOUNT.contains(&kind) {
            self.account_request(ctx, kind, &body).await;
        } else if msg_kind::LADDER.contains(&kind) {
            self.ladder_request(ctx, kind, &body).await;
        } else if msg_kind::LOBBY.contains(&kind) {
            let mut c = ctx.lock().await;
            if c.account.is_none() {
                c.out.error(kind, ErrorCode::NOT_LOGGED_IN, 0, "");
            } else if c.character.is_none() {
                c.out.error(kind, ErrorCode::NO_CHAR_SELECTED, 0, "");
            } else if let Err(d) = self.lobby.handle(&mut c, &body).await {
                c.out.send(d.to_msg(kind).body());
            }
            let id = c.id;
            drop(c);
            // A create or a join seats it: its place follows.
            self.social_place(id);
        } else if msg_kind::SOCIAL.contains(&kind) {
            self.social_request(ctx, kind, &body).await;
        } else if msg_kind::GUILD.contains(&kind) || msg_kind::FRIENDS.contains(&kind) {
            self.guild_request(ctx, kind, &body).await;
        } else if STASH_KINDS.contains(&kind) {
            self.stash_request(ctx, kind, &body).await;
        } else if msg_kind::MARKET.contains(&kind) {
            self.market_request(ctx, kind, &body).await;
        } else {
            ctx.lock()
                .await
                .out
                .error(kind, ErrorCode::NOT_SUPPORTED, 0, "");
        }
    }

    /// Kinds 0x50-0x57: the ladder's. Answered here (a page is a few store
    /// reads); the session's lock is not held meanwhile.
    async fn ladder_request(&self, ctx: &Ctx, kind: u8, body: &[u8]) {
        let who = {
            let c = ctx.lock().await;
            let Some(ladder) = &self.ladder else {
                c.out.error(kind, ErrorCode::NOT_SUPPORTED, 0, "");
                return;
            };
            let Ok(msg) = LadderRequest::parse(body) else {
                c.out.error(kind, ErrorCode::MALFORMED, 0, "");
                c.out.close();
                return;
            };
            let Some((account, _)) = &c.account else {
                c.out.error(kind, ErrorCode::NOT_LOGGED_IN, 0, "");
                return;
            };
            let who = Asker {
                account: *account,
                character: c.character.as_ref().map(|s| s.name.clone()),
                out: c.out.clone(),
            };
            (ladder.clone(), who, msg)
        };
        let (ladder, who, msg) = who;
        let online = |account: i64, name: &str| self.plays(AccountId(account), name);
        ladder.request(&who, msg, &online).await;
    }

    /// Whether `account`'s character `name` plays now: in a game, or the
    /// character its session has selected. A session busy with a request
    /// counts as playing it.
    fn plays(&self, account: AccountId, name: &str) -> bool {
        if self.lobby.character_in_game(&name.to_ascii_lowercase()) {
            return true;
        }
        let ctx = {
            let st = self.state();
            st.by_account
                .get(&account)
                .and_then(|id| st.ctxs.get(id))
                .cloned()
        };
        ctx.is_some_and(|ctx| match ctx.try_lock() {
            Ok(c) => c
                .character
                .as_ref()
                .is_some_and(|s| s.name.eq_ignore_ascii_case(name)),
            Err(_) => true,
        })
    }

    /// Kinds 0xA0-0xAF: the online stash's, on a task of their own.
    async fn stash_request(&self, ctx: &Ctx, kind: u8, body: &[u8]) {
        let c = ctx.lock().await;
        let Ok(req) = StashRequest::parse(body) else {
            c.out.error(kind, ErrorCode::MALFORMED, 0, "");
            c.out.close();
            return;
        };
        let Some((account, _)) = &c.account else {
            c.out.error(kind, ErrorCode::NOT_LOGGED_IN, 0, "");
            return;
        };
        let Some(realm) = self.stash.clone().filter(|r| r.stash().is_some()) else {
            c.out.error(kind, ErrorCode::NOT_SUPPORTED, 0, "");
            return;
        };
        let who = StashUser {
            account: *account,
            character: c.character.clone(),
            out: c.out.clone(),
        };
        tokio::spawn(realm.stash_request(who, req));
    }

    /// Kinds 0xB0-0xCF: the market's, on a task of their own.
    async fn market_request(&self, ctx: &Ctx, kind: u8, body: &[u8]) {
        let c = ctx.lock().await;
        let Some(market) = self.market.clone() else {
            c.out.error(kind, ErrorCode::NOT_SUPPORTED, 0, "");
            return;
        };
        let Ok(msg) = ToRealm::parse(body) else {
            c.out.error(kind, ErrorCode::MALFORMED, 0, "");
            c.out.close();
            return;
        };
        let Some(who) = Self::market_who(&c) else {
            c.out.error(kind, ErrorCode::NOT_LOGGED_IN, 0, "");
            return;
        };
        market.request(who, msg);
    }

    /// Kinds 0x70-0x7F: the chat's, in the session's order (the session
    /// lock is not held while the request runs).
    async fn social_request(&self, ctx: &Ctx, kind: u8, body: &[u8]) {
        let Some((social, who)) = self.social_who(ctx, kind).await else {
            return;
        };
        let Ok(msg) = social_msg::ToRealm::parse(body) else {
            who.out.error(kind, ErrorCode::MALFORMED, 0, "");
            who.out.close();
            return;
        };
        social.social(who, msg).await;
    }

    /// Kinds 0xF0-0xFF: the guilds' and the friends'.
    async fn guild_request(&self, ctx: &Ctx, kind: u8, body: &[u8]) {
        let Some((social, who)) = self.social_who(ctx, kind).await else {
            return;
        };
        let Ok(msg) = guild_msg::ToRealm::parse(body) else {
            who.out.error(kind, ErrorCode::MALFORMED, 0, "");
            who.out.close();
            return;
        };
        social.guild(who, msg).await;
    }

    /// The social service and the session's member view, or the refusal
    /// sent (no service: `NotSupported`; not logged in).
    async fn social_who(&self, ctx: &Ctx, kind: u8) -> Option<(Arc<dyn SocialService>, Member)> {
        let c = ctx.lock().await;
        let Some(social) = self.social.clone() else {
            c.out.error(kind, ErrorCode::NOT_SUPPORTED, 0, "");
            return None;
        };
        let Some(who) = Self::member(&c) else {
            c.out.error(kind, ErrorCode::NOT_LOGGED_IN, 0, "");
            return None;
        };
        Some((social, who))
    }

    /// Kinds 0x20-0x3F on the accounts.
    async fn account_request(&self, ctx: &Ctx, kind: u8, body: &[u8]) {
        // The session lock is taken for the state read, and never held
        // across an account call or another session's lock: two logins
        // taking each other over must not deadlock.
        let (req, account, ip, out) = {
            let c = ctx.lock().await;
            let Ok(req) = AccountRequest::parse(body) else {
                c.out.error(kind, ErrorCode::MALFORMED, 0, "");
                c.out.close();
                return;
            };
            (
                req,
                c.account.as_ref().map(|(a, _)| *a),
                c.ip,
                c.out.clone(),
            )
        };
        if !req.before_login() && account.is_none() {
            out.error(kind, ErrorCode::NOT_LOGGED_IN, 0, "");
            return;
        }
        let answer: Result<AccountEvent, crate::accounts::Denied> = match req {
            AccountRequest::Register {
                account: name,
                password,
            } => {
                if account.is_some() {
                    Err(ErrorCode::OTHER.into())
                } else {
                    match self.accounts.register(&name, &password, ip).await {
                        Ok(a) => Ok(self.logged_in(ctx, a).await),
                        Err(d) => Err(d),
                    }
                }
            }
            AccountRequest::Login {
                account: name,
                password,
            } => {
                if account.is_some() {
                    Err(ErrorCode::OTHER.into())
                } else {
                    match self.accounts.login(&name, &password, ip).await {
                        Ok(a) => Ok(self.logged_in(ctx, a).await),
                        Err(d) => Err(d),
                    }
                }
            }
            AccountRequest::Logout => {
                out.close();
                return;
            }
            AccountRequest::CharList => {
                let a = account.expect("checked");
                Ok(AccountEvent::CharList(self.accounts.characters(a).await))
            }
            AccountRequest::CharCreate {
                name,
                class,
                hardcore,
                expansion,
                ladder,
            } => {
                let a = account.expect("checked");
                let new = NewChar {
                    name,
                    class,
                    hardcore,
                    expansion,
                    progression: 0,
                };
                // A ladder character goes in the open season.
                let season = match (&self.ladder, ladder) {
                    (_, false) => Ok(crate::accounts::ladder::NON_LADDER),
                    (Some(l), true) => match l.current_season().await {
                        Ok(Some(s)) => Ok(s),
                        Ok(None) => Err(ErrorCode::NO_SUCH_SEASON.into()),
                        Err(d) => Err(d),
                    },
                    (None, true) => Err(ErrorCode::NO_SUCH_SEASON.into()),
                };
                match season {
                    Ok(season) => self
                        .accounts
                        .create_character_in(a, new, season)
                        .await
                        .map(AccountEvent::CharCreated),
                    Err(d) => Err(d),
                }
            }
            AccountRequest::CharDelete { name, password } => {
                let a = account.expect("checked");
                if self.character_in_game(&name) {
                    Err(ErrorCode::CHAR_IN_GAME.into())
                } else {
                    match self.accounts.delete_character(a, &name, &password).await {
                        Ok(()) => {
                            if let Some(l) = &self.ladder {
                                l.deleted(&name).await;
                            }
                            let mut c = ctx.lock().await;
                            if c.character
                                .as_ref()
                                .is_some_and(|s| s.name.eq_ignore_ascii_case(&name))
                            {
                                c.character = None;
                                self.market_seat(&c);
                            }
                            Ok(AccountEvent::CharDeleted { name })
                        }
                        Err(d) => Err(d),
                    }
                }
            }
            AccountRequest::CharSelect { name } => {
                let a = account.expect("checked");
                let id = ctx.lock().await.id;
                let in_game = self.lobby.session_game(id).is_some();
                if in_game {
                    Err(ErrorCode::ALREADY_IN_GAME.into())
                } else if self.character_in_game(&name) {
                    Err(ErrorCode::CHAR_IN_GAME.into())
                } else {
                    match self.accounts.character(a, &name).await {
                        Ok(summary) => {
                            let mut c = ctx.lock().await;
                            c.character = Some(summary.clone());
                            self.market_seat(&c);
                            // The answer first, then what a session in the
                            // lobby gets.
                            c.out.send(AccountEvent::CharSelected(summary).body());
                            self.lobby.char_selected(&c);
                            return;
                        }
                        Err(d) => Err(d),
                    }
                }
            }
        };
        match answer {
            Ok(ev) => out.send(ev.body()),
            Err(d) => out.send(d.to_msg(kind).body()),
        }
    }

    fn character_in_game(&self, name: &str) -> bool {
        let key = name.to_ascii_lowercase();
        self.lobby.character_in_game(&key)
    }

    /// A login: the account's session is claimed atomically and the one it
    /// had is taken over, and this one counts as logged in. Sessions are
    /// locked one at a time (never while holding this session's lock), so
    /// two logins cannot deadlock and only one keeps the account.
    async fn logged_in(&self, ctx: &Ctx, a: Account) -> AccountEvent {
        let (id, ip) = {
            let c = ctx.lock().await;
            (c.id, c.ip)
        };
        self.unauthenticated_done(ip);
        let previous = {
            let mut st = self.state();
            st.by_account.insert(a.id, id)
        };
        if let Some(previous) = previous.filter(|&p| p != id) {
            let other = self.state().ctxs.get(&previous).cloned();
            if let Some(other) = other {
                self.lobby.takeover(a.id);
                let out = { other.lock().await.out.clone() };
                out.error(msg_kind_login(), ErrorCode::SESSION_TAKEN, 0, "");
                out.close();
                self.end_session(previous, "the account logged in elsewhere")
                    .await;
            }
        }
        self.log
            .line(&format!("session {id}: {} logged in", a.name));
        {
            let mut c = ctx.lock().await;
            c.account = Some((a.id, a.name.clone()));
            self.social_seat(&c);
        }
        AccountEvent::LoggedIn {
            account: a.name,
            max_characters: self.accounts.config().max_characters,
        }
    }

    /// The session's game streams: refused, the main server runs no games.
    async fn game_streams(self: Arc<Self>, conn: QuicConn, _ctx: Ctx) {
        while let Some(mut stream) = conn.accept_stream().await {
            tokio::spawn(async move {
                let _ = tokio::time::timeout(HANDSHAKE_WAIT, stream.recv_async()).await;
                let _ = stream.send(Refused::new(ErrorCode::NOT_SUPPORTED).body());
                stream.close();
            });
        }
    }
}

/// Closes `conn` after [`CLOSE_GRACE`], once its last frames are out.
fn close_later(conn: QuicConn, reason: &'static str) {
    tokio::spawn(async move {
        tokio::time::sleep(CLOSE_GRACE).await;
        conn.close(0, reason);
    });
}

/// The kind an unasked `SessionTaken` answers: the login that took the
/// account over.
fn msg_kind_login() -> u8 {
    AccountRequest::Login {
        account: String::new(),
        password: String::new(),
    }
    .kind()
}
