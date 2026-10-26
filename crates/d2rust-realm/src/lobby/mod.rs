//! The lobby: the list of open games, creating and joining them with the
//! password, difficulty and level-range rules, and the lobby chat
//! (multiplayer contracts sections 3.6, 5 and 6; the game list v2 of
//! `docs/plans/server/lobby-contract.md` section 7).
//!
//! - [`state`]: every rule, with no I/O and a clock the caller passes
//!   (from the one-process server multiplayer v1 had).
//! - [`Lobby`]: the rules on the main server. It answers the lobby's
//!   control messages ([`LobbyApi::handle`]) and keeps every listening
//!   session's game list current: every open game, the ones the
//!   character can't join with the reason, each with its server, its
//!   description and level range. The selected character is the session's
//!   stored summary, never the client's word.
//!
//! The main server runs no games itself; its game servers do (plan N4,
//! contracts 11.3-11.5). Every session that selected a character gets the
//! list of linked game servers (`NodeList`, [`crate::realm::listing`]) at
//! once and on each `ListGames`, and again whenever a server links, is
//! lost or its load or state changes (at most every 5 s,
//! [`Lobby::push_nodes`]). `CreateGame` that passes the rules starts the
//! game where its `Placement` says: the server the creator chose (refused
//! when it is not linked, full or draining), or for Automatic the
//! client's pick by ping, else the least loaded open server (none:
//! `NodeUnavailable`); then the creator's seat, as for `JoinGame`: a seat
//! lease, the character's record offered to the game server, and the
//! client's `JoinNode` with its ticket. The client takes the seat on the
//! game server; what the game server then reports (seat taken or refused,
//! players, left, closed, its link lost) keeps the list and the seats
//! current ([`LobbyApi::on_news`]).
//!
//! A takeover (the account logged in elsewhere) ends its seats' leases at
//! once and kicks its characters; any other end of a session kicks them
//! too, and they leave after their last save.
//!
//! With the chat rooms ([`Lobby::with_social`]), the old lobby chat line
//! (0x49) is said in the session's Mod room by the rooms' rules (lobby
//! contract 7.1).

pub mod state;

use std::collections::{BTreeSet, HashMap};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use d2_link::{Lease, NodeId, PlayerId};
use d2_net::msg::account::CharSummary;
use d2_net::msg::guild::FriendGame;
use d2_net::msg::lobby::{reach, GameInfo, LobbyEvent, LobbyRequest};
use d2_net::msg::node::{NewCharInfo, NodeGameSpec};
use d2_net::msg::nodelist::Placement;
use d2_net::msg::ErrorCode;
use d2_quic::parse_fingerprint;
use d2_tickets::SeatGrant;

use crate::accounts::{unix_now, AccountId, LimitMode};
use crate::config::RealmConfig;
use crate::denied::Denied;
use crate::realm::games::{random_u64, GameNews, SeatOffer};
use crate::realm::listing::{display_name, player_addr, Pushes};
use crate::realm::nodes::Linked;
use crate::realm::saves::{LeaseEnd, LeaseGrant};
use crate::realm::tickets::NodeContact;
use crate::realm::Realm;
use crate::session::{ControlTx, LobbyApi, SessionCtx, SessionId, Sessions};
use crate::social::friends::FriendGames;
use crate::social::Social;

pub use state::{
    GameServer, Limits, LobbyChar, LobbyState, NewGame, OpenGame, Refusal, Ticket, TICKET_TTL,
};

/// The lobby's view of a session's selected character.
pub fn lobby_char(c: &CharSummary) -> LobbyChar {
    LobbyChar {
        name: c.name.clone(),
        level: c.level,
        status: c.status,
        difficulties: c.difficulties,
    }
}

/// The lobby's limits from the main server's config: `max_games` and the
/// players per game.
pub fn limits(cfg: &RealmConfig) -> Limits {
    let players = u8::try_from(cfg.max_players).unwrap_or(u8::MAX);
    Limits {
        max_games: cfg.max_games,
        default_players: players,
        players_cap: players,
        test_windows: cfg.accounts.limits == LimitMode::Test,
    }
}

impl From<Refusal> for Denied {
    fn from(r: Refusal) -> Self {
        Denied::wait(r.code, r.wait_s)
    }
}

/// A seat the lobby handed out: held until its player leaves or it is
/// refused.
#[derive(Clone, Debug)]
struct HeldSeat {
    game: u32,
    node: String,
    session: SessionId,
    character: String,
    /// The client took it on the game server.
    taken: bool,
}

/// The lobby on the main server.
pub struct Lobby {
    state: Mutex<LobbyState>,
    sessions: Sessions,
    realm: Arc<Realm>,
    /// Each open game's game server.
    nodes: Mutex<HashMap<u32, String>>,
    /// The seats handed out, by lease.
    seats: Mutex<HashMap<Lease, HeldSeat>>,
    /// The chat rooms, which take the old chat line.
    social: Option<Arc<Social>>,
    /// The sessions that get the game-server list: those that selected a
    /// character.
    node_watch: Mutex<BTreeSet<SessionId>>,
    /// When the list goes out to them again.
    node_push: Mutex<Pushes>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Unix milliseconds now, for a game's creation time in the list.
fn unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

/// A seat's key in the lobby state's tickets: its lease (the client's
/// ticket is the game server's business).
fn seat_key(lease: Lease) -> [u8; 16] {
    let mut k = [0u8; 16];
    k[..8].copy_from_slice(&lease.0.to_le_bytes());
    k
}

/// How clients reach a linked game server: its listed public address, or
/// the address its link comes from.
fn contact(node: &Linked) -> Result<NodeContact, ErrorCode> {
    Ok(NodeContact {
        addr: player_addr(&node.entry, node.addr),
        cert_sha256: parse_fingerprint(&node.entry.cert_sha256).ok_or(ErrorCode::OTHER)?,
        player_hosted: false,
        reach: reach::DIRECT,
    })
}

impl Lobby {
    pub fn new(limits: Limits, sessions: Sessions, realm: Arc<Realm>) -> Self {
        Lobby {
            state: Mutex::new(LobbyState::new(limits)),
            sessions,
            realm,
            nodes: Mutex::new(HashMap::new()),
            seats: Mutex::new(HashMap::new()),
            social: None,
            node_watch: Mutex::new(BTreeSet::new()),
            node_push: Mutex::new(Pushes::default()),
        }
    }

    /// The old chat line (0x49) goes to `social`'s Mod room.
    pub fn with_social(mut self, social: Arc<Social>) -> Self {
        self.social = Some(social);
        self
    }

    /// The open games (for the status file and tests).
    pub fn games(&self) -> Vec<OpenGame> {
        lock(&self.state).games().cloned().collect()
    }

    /// Sends the game-server list to one session, which gets every change
    /// from now on.
    fn send_nodes(&self, out: &ControlTx) {
        let list = self.realm.node_list();
        lock(&self.node_watch).insert(out.id());
        lock(&self.node_push).sent_one(&list);
        out.send(list.body());
    }

    /// Sends the game-server list to every session that has it, when it
    /// changed since the last time and that was at least 5 s ago
    /// ([`Pushes::due`]).
    pub fn push_nodes(&self, now: Instant) {
        let list = self.realm.node_list();
        if !lock(&self.node_push).due(&list, now) {
            return;
        }
        let body = list.body();
        let watching: Vec<SessionId> = lock(&self.node_watch).iter().copied().collect();
        for id in watching {
            if !self.sessions.send(id, body.clone()) {
                lock(&self.node_watch).remove(&id);
            }
        }
    }

    /// Pushes the game-server list's changes ([`Lobby::push_nodes`]),
    /// checking every second, for as long as the main server runs.
    pub async fn run_node_list(self: Arc<Self>) {
        let mut every = tokio::time::interval(std::time::Duration::from_secs(1));
        loop {
            every.tick().await;
            self.push_nodes(Instant::now());
        }
    }

    /// Every listener's new view of game `id`: its entry, or `GameGone`.
    fn broadcast(&self, id: u32) {
        let views = lock(&self.state).views_of(id);
        for (session, view) in views {
            let ev = match view {
                Some(g) => LobbyEvent::GameEntry(g),
                None => LobbyEvent::GameGone { game_id: id },
            };
            self.sessions.send(session, ev.body());
        }
    }

    /// `CreateGame`: the rules, then the game on the game server `place`
    /// says (listed once it runs), then the creator's seat.
    async fn create(
        &self,
        s: &mut SessionCtx,
        c: &LobbyChar,
        new: NewGame<'_>,
        place: &Placement,
        now: Instant,
    ) -> Result<(), Denied> {
        let (g, node) = {
            let mut st = lock(&self.state);
            let mut g = st.check_create(s.id, c, new, now)?;
            let node = self.realm.place(place)?;
            g.server = GameServer::official(&node.entry.id, &display_name(&node.entry));
            g.created_ms = unix_ms();
            // Opened now, so no other create takes its name meanwhile.
            st.opened(g.clone());
            (g, node)
        };
        lock(&self.nodes).insert(g.id, node.entry.id.clone());
        let spec = NodeGameSpec {
            game: g.id,
            name: g.name.clone(),
            difficulty: g.difficulty,
            hardcore: g.hardcore,
            expansion: g.expansion,
            max_players: g.max_players,
        };
        if let Err(code) = self.realm.start_game(&node, spec).await {
            self.forget_game(g.id);
            return Err(code.into());
        }
        self.realm.log_line(&format!(
            "game {} \"{}\" runs on node {}",
            g.id, g.name, node.entry.id
        ));
        self.broadcast(g.id);
        self.seat(s, c, g.id, &node, now).await
    }

    /// `JoinGame`: the rules, then the seat.
    async fn join(
        &self,
        s: &mut SessionCtx,
        c: &LobbyChar,
        game_id: u32,
        password: &str,
        now: Instant,
    ) -> Result<(), Denied> {
        lock(&self.state).check_join(s.id, c, game_id, password, now)?;
        let node = lock(&self.nodes)
            .get(&game_id)
            .cloned()
            .and_then(|n| self.realm.nodes.get(&n))
            .ok_or(ErrorCode::GAME_NOT_FOUND)?;
        self.seat(s, c, game_id, &node, now).await
    }

    /// Holds a seat in `game` on `node` for the session's character: its
    /// lease, its offer (with the stored record) to the game server, and
    /// the client's `JoinNode`.
    async fn seat(
        &self,
        s: &mut SessionCtx,
        c: &LobbyChar,
        game: u32,
        node: &Linked,
        now: Instant,
    ) -> Result<(), Denied> {
        let (account, _) = s.account.clone().ok_or(ErrorCode::NOT_LOGGED_IN)?;
        let cs = self.realm.saves.seat(account, &c.name).await?;
        let character = cs.summary.name.clone();
        let difficulty = lock(&self.state)
            .game(game)
            .map(|g| g.difficulty)
            .ok_or(ErrorCode::GAME_NOT_FOUND)?;
        let lease = self.realm.saves.leases().grant(LeaseGrant {
            account,
            character: character.clone(),
            game,
            node: node.entry.id.clone(),
        })?;
        let issued = lock(&self.state).issue(s.id, c, game, seat_key(lease), now);
        if let Err(r) = issued {
            self.realm.saves.end_any(lease, LeaseEnd::Refused);
            return Err(r.into());
        }
        lock(&self.seats).insert(
            lease,
            HeldSeat {
                game,
                node: node.entry.id.clone(),
                session: s.id,
                character: character.clone(),
                taken: false,
            },
        );
        // The player's id in the game: opaque, never the account's.
        let player = random_u64() >> 1;
        let offer = SeatOffer {
            game,
            lease,
            player,
            character: character.clone(),
            new: cs.new.map(|n| NewCharInfo {
                name: n.name,
                class: n.class,
                hardcore: n.hardcore,
                expansion: n.expansion,
                progression: n.progression,
            }),
            record: cs.record,
        };
        let grant = SeatGrant {
            realm: self.realm.versions.game.clone(),
            game,
            node: NodeId(node.entry.id.clone()),
            lease,
            player: PlayerId(player),
            character,
            difficulty,
            build: self.realm.versions.version.clone(),
        };
        let joined = match self.realm.offer_seat(node, offer).await {
            Ok(()) => contact(node).and_then(|to| {
                self.realm
                    .tickets
                    .join_node(&grant, &to, SystemTime::now())
                    .map_err(|e| {
                        self.realm.log_line(&format!("{lease}: no ticket: {e}"));
                        ErrorCode::OTHER
                    })
            }),
            Err(code) => Err(code),
        };
        match joined {
            Ok(j) => {
                s.game = Some(game);
                s.out.send(LobbyEvent::JoinNode(j).body());
                self.broadcast(game);
                Ok(())
            }
            Err(code) => {
                self.end_seat(lease, LeaseEnd::Refused);
                self.realm.kick(&node.entry.id, game, lease);
                Err(code.into())
            }
        }
    }

    /// Ends seat `lease` here: its lease (if still live), its hold.
    fn end_seat(&self, lease: Lease, why: LeaseEnd) -> Option<HeldSeat> {
        let seat = lock(&self.seats).remove(&lease)?;
        self.realm.saves.end_any(lease, why);
        lock(&self.state).unseat(&seat_key(lease), &seat.character);
        self.broadcast(seat.game);
        Some(seat)
    }

    /// Game `game` is over: it leaves the list, its seats go.
    fn forget_game(&self, game: u32) {
        lock(&self.state).closed(game);
        lock(&self.nodes).remove(&game);
        lock(&self.seats).retain(|_, s| s.game != game);
        self.broadcast(game);
    }

    fn list(&self, s: &SessionCtx, c: &LobbyChar) {
        let list: Vec<GameInfo> = lock(&self.state).subscribe(s.id, c);
        let count = u16::try_from(list.len()).unwrap_or(u16::MAX);
        s.out.send(LobbyEvent::GameListBegin { count }.body());
        for g in list {
            s.out.send(LobbyEvent::GameEntry(g).body());
        }
        s.out.send(LobbyEvent::GameListEnd.body());
    }

    async fn chat(
        &self,
        s: &SessionCtx,
        c: &LobbyChar,
        text: String,
        now: Instant,
    ) -> Result<(), Denied> {
        if let Some(social) = &self.social {
            return social.legacy_say(s.id, &text).await;
        }
        let listeners = {
            let mut st = lock(&self.state);
            st.check_chat(s.id, now)?;
            st.listeners()
        };
        let body = LobbyEvent::ChatLine {
            from: c.name.clone(),
            text,
            at: u64::try_from(unix_now()).unwrap_or(0),
        }
        .body();
        for id in listeners {
            self.sessions.send(id, body.clone());
        }
        Ok(())
    }
}

/// The friends' view of the open games: a friend's game with the asker's
/// join rule (lobby contract 5).
impl FriendGames for Lobby {
    fn friend_game(&self, game: u32, asker: Option<&CharSummary>) -> Option<FriendGame> {
        let st = lock(&self.state);
        let g = st.game(game)?;
        Some(FriendGame {
            game_id: g.id,
            game_name: g.name.clone(),
            joinable: asker.map_or(ErrorCode::NO_CHAR_SELECTED.0, |c| {
                st.joinable(&lobby_char(c), g)
            }),
        })
    }
}

impl LobbyApi for Lobby {
    async fn handle(&self, s: &mut SessionCtx, body: &[u8]) -> Result<(), Denied> {
        let req = LobbyRequest::parse(body).map_err(|_| ErrorCode::MALFORMED)?;
        let c = lobby_char(s.character.as_ref().ok_or(ErrorCode::NO_CHAR_SELECTED)?);
        let now = Instant::now();
        match req {
            LobbyRequest::ListGames => {
                // Entering the lobby (again, after a game: the client read
                // no lobby messages while it played).
                self.send_nodes(&s.out);
                self.list(s, &c);
            }
            LobbyRequest::LeaveLobby => lock(&self.state).unsubscribe(s.id),
            LobbyRequest::Chat { text } => self.chat(s, &c, text, now).await?,
            LobbyRequest::CreateGame {
                name,
                password,
                difficulty,
                max_players,
                description,
                level_min,
                level_max,
                place,
            } => {
                let new = NewGame {
                    name: &name,
                    password: &password,
                    difficulty,
                    max_players,
                    description: &description,
                    level_min,
                    level_max,
                };
                self.create(s, &c, new, &place, now).await?;
            }
            LobbyRequest::JoinGame { game_id, password } => {
                self.join(s, &c, game_id, &password, now).await?;
            }
        }
        Ok(())
    }

    fn char_selected(&self, s: &SessionCtx) {
        self.send_nodes(&s.out);
    }

    fn session_ended(&self, s: &SessionCtx) {
        lock(&self.state).session_ended(s.id);
        lock(&self.node_watch).remove(&s.id);
        let mine: Vec<(Lease, HeldSeat)> = lock(&self.seats)
            .iter()
            .filter(|(_, h)| h.session == s.id)
            .map(|(l, h)| (*l, h.clone()))
            .collect();
        for (lease, h) in mine {
            // A seat not taken yet ends now; a taken one ends when its
            // character, kicked, has been saved and left.
            if !h.taken {
                self.end_seat(lease, LeaseEnd::Closed);
            }
            self.realm.kick(&h.node, h.game, lease);
        }
    }

    fn takeover(&self, account: AccountId) {
        for (lease, g) in self.realm.saves.end_account(account, LeaseEnd::Takeover) {
            self.end_seat(lease, LeaseEnd::Takeover);
            self.realm.kick(&g.node, g.game, lease);
        }
    }

    fn on_news(&self, news: GameNews) {
        match news {
            GameNews::SeatTaken { lease, .. } => {
                if let Some(h) = lock(&self.seats).get_mut(&lease) {
                    h.taken = true;
                }
                lock(&self.state).seated(&seat_key(lease));
            }
            GameNews::SeatRefused { lease, .. } => {
                self.end_seat(lease, LeaseEnd::Refused);
            }
            GameNews::Left { lease, .. } => {
                self.end_seat(lease, LeaseEnd::Left);
            }
            GameNews::Players { game, players } => {
                lock(&self.state).players(game, players);
                self.broadcast(game);
            }
            GameNews::Closed { game } => self.forget_game(game),
            GameNews::NodeLost { node } => {
                let games: Vec<u32> = lock(&self.nodes)
                    .iter()
                    .filter(|(_, n)| **n == node)
                    .map(|(g, _)| *g)
                    .collect();
                for g in games {
                    self.forget_game(g);
                }
            }
            GameNews::Saved { .. } => {}
        }
    }

    fn session_game(&self, session: SessionId) -> Option<u32> {
        lock(&self.state).session_game(session)
    }

    fn character_in_game(&self, name_key: &str) -> bool {
        lock(&self.state).character_in_game(name_key)
    }

    fn caps(&self) -> &'static [&'static str] {
        &["lobby", "chat", "node-choice"]
    }
}
