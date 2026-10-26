//! The online screens' link to a real server (the `online` feature): the
//! screens' [`Request`]s become control-stream messages on the shared
//! [`OnlineSession`], and what the server sends becomes [`Event`]s.
//!
//! Connecting waits for the network, so it runs on a worker thread; its
//! answer arrives as an event. Every other request only queues its body,
//! and [`OnlineLink::poll`] reads what has arrived without waiting, so the
//! front end's 40 ms tick never blocks. The session stays in the
//! [`OnlineSession`] after the lobby's `JoinOk`: the game joins through it
//! and the player comes back to the lobby on the same session.
//!
//! When the main server lists its game servers (`NodeList`), a
//! [`Pinger`] measures the pings to them in the background (QUIC
//! handshakes, `d2_quic::ping`), and each new measurement reaches the
//! screens as [`Event::Pings`]. The main server itself is pinged from
//! the connection on (as [`REALM_PING`], with the address and pinned
//! certificate of the launcher's list), list or not: a player-hosted
//! game's ping is that round trip plus the host's, as the realm reports
//! it. `CreateGame` carries where the new game runs, its description and
//! its level range.
//!
//! The realm lobby's requests (chat rooms, DMs, user lists, guilds,
//! friends, the ladder: `docs/plans/server/lobby-contract.md`) are
//! numbered per session (`u32 req`, from 1); their answers and list rows
//! carry the number, so `d2_online`'s social and ladder clients put each
//! list together while pushes (`req` 0) arrive in between, and the link
//! hands the screens one event per answer ([`realm`]).
//!
//! The lobby's market window (plan L18) asks the realm's market on the
//! same session: the link stays its one reader, so the market's bodies
//! (and the `ERROR`s answering its requests) go to the market's queue of
//! a [`RealmInbox`], and what the market queued in its [`RealmOutbox`]
//! goes out on the next poll ([`OnlineLink::market`]).

mod realm;

#[cfg(test)]
mod social_tests;

use std::collections::{HashMap, VecDeque};
use std::sync::mpsc;

use d2_net::msg::account::{AccountEvent, AccountRequest, CharSummary};
use d2_net::msg::lobby::{LobbyEvent, LobbyRequest};
use d2_net::msg::{kind as ranges, ErrorCode, ErrorMsg};
use d2_net::Timing;
use d2_online::{
    node_list_event, OnlineError, PingTarget, Pinger, QuicProbe, ServerSpec, ServerTrust, Session,
};

use super::link::{
    node_code, CharInfo, Event, Failure, GameInfo, Joined, OnlineLink, PingTable, Placement,
    Request,
};
use super::{Online, OnlineHandle, ServerEntry, REALM_PING};
use crate::app::online::{client_info, OnlineSession, ServerList};
use crate::app::realm_pump::{owner_of, Owner, RealmInbox, RealmOutbox};
use crate::ui::front_end::Registry;
use realm::RealmLobby;

/// What the screens show for a failure the session reports.
pub fn failure(e: &OnlineError) -> Failure {
    Failure {
        code: e.code().unwrap_or(ErrorCode(0)),
        wait_s: e.wait_s(),
        text: e.message(),
    }
}

fn char_info(c: &CharSummary) -> CharInfo {
    CharInfo {
        name: c.name.clone(),
        class: c.class,
        level: c.level,
        status: c.status,
        difficulties: c.difficulties,
        look: c.look.clone(),
        last_played: c.last_played,
        season: c.season,
    }
}

fn game_info(g: &d2_net::msg::lobby::GameInfo) -> GameInfo {
    GameInfo {
        id: g.game_id,
        name: g.name.clone(),
        difficulty: g.difficulty,
        players: g.players,
        max_players: g.max_players,
        locked: g.locked,
        hardcore: g.hardcore,
        expansion: g.expansion,
        joinable: g.joinable,
        server: g.server.clone(),
        player_hosted: g.player_hosted,
        node_id: g.node_id.clone(),
        host_rtt_ms: g.host_rtt_ms,
        description: g.description.clone(),
        level_min: g.level_min,
        level_max: g.level_max,
        created_ms: g.created_ms,
    }
}

/// The screens' entries for the launcher's server list.
pub fn entries(list: &ServerList) -> Vec<ServerEntry> {
    list.servers
        .iter()
        .map(|s| ServerEntry {
            id: s.id.clone(),
            name: s.name.clone(),
            address: s.address.clone(),
        })
        .collect()
}

/// The main server's own ping target, when its certificate is pinned.
fn realm_target(spec: &ServerSpec) -> Option<PingTarget> {
    match spec.trust {
        ServerTrust::Pinned(cert_sha256) => Some(PingTarget {
            node: REALM_PING.into(),
            addr: spec.address.clone(),
            cert_sha256,
        }),
        ServerTrust::Any => None,
    }
}

/// The link over the shared session.
pub struct NetLink {
    session: OnlineSession,
    list: ServerList,
    /// The connect worker's answer, while one runs.
    connecting: Option<mpsc::Receiver<Event>>,
    out: VecDeque<Event>,
    /// Game names by id, from the list, for the join's loading screen.
    names: HashMap<u32, String>,
    /// The pings to the listed game servers, once there is a list, and the
    /// measurement the screens saw last.
    pinger: Option<Pinger>,
    pings_seen: u64,
    /// The main server's ping target, once connected.
    realm_ping: Option<PingTarget>,
    /// The realm lobby's requests in flight and lists being put together.
    realm: RealmLobby,
    /// The lobby market's queues, once the lobby made its market.
    market: Option<(RealmInbox, RealmOutbox)>,
}

impl NetLink {
    pub fn new(session: OnlineSession, list: ServerList) -> Self {
        NetLink {
            session,
            list,
            connecting: None,
            out: VecDeque::new(),
            names: HashMap::new(),
            pinger: None,
            pings_seen: 0,
            realm_ping: None,
            realm: RealmLobby::new(),
            market: None,
        }
    }

    /// Sends a realm-lobby request with the session's next number.
    fn send_numbered(&mut self, r: Request) {
        if let Some(body) = self.realm.number(r) {
            self.send_body(body);
        }
    }

    /// The game servers to ping, and the main server beside them.
    fn with_realm(&self, mut targets: Vec<PingTarget>) -> Vec<PingTarget> {
        targets.extend(self.realm_ping.clone());
        targets
    }

    /// The connection is up: the main server's ping starts at once (a
    /// realm without game servers sends no list, and its player-hosted
    /// games still need it); the game servers join it once listed.
    fn connected(&mut self) {
        let realm = self.with_realm(Vec::new());
        self.ping(realm);
    }

    /// Pings the listed servers (from now on, in the background).
    fn ping(&mut self, targets: Vec<PingTarget>) {
        if self.pinger.is_none() {
            if targets.is_empty() {
                return;
            }
            // Without a socket there are no pings: Automatic then takes
            // the main server's suggestion.
            self.pinger = QuicProbe::new().and_then(Pinger::start).ok();
            self.pings_seen = 0;
        }
        if let Some(p) = &self.pinger {
            p.set_targets(targets);
        }
    }

    /// The pings, when they changed since the screens saw them last.
    fn new_pings(&mut self) -> Option<Event> {
        let p = self.pinger.as_ref()?;
        let version = p.version();
        if version == self.pings_seen {
            return None;
        }
        self.pings_seen = version;
        let table: PingTable = p
            .pings()
            .iter()
            .map(|(node, ping)| (node.to_string(), ping.ms))
            .collect();
        Some(Event::Pings(table))
    }

    fn lost(&mut self, text: &str) {
        self.out.push_back(Event::Disconnected(Failure {
            code: ErrorCode(0),
            wait_s: 0,
            text: text.into(),
        }));
    }

    fn send_body(&mut self, body: Vec<u8>) {
        let r = match self.session.0.lock() {
            Ok(mut slot) => match slot.as_mut() {
                Some(s) => s.send(body).map_err(|e| failure(&e)),
                None => Err(Failure {
                    code: ErrorCode(0),
                    wait_s: 0,
                    text: "Not connected to the server.".into(),
                }),
            },
            Err(_) => Err(Failure {
                code: ErrorCode(0),
                wait_s: 0,
                text: "The online session is not available.".into(),
            }),
        };
        if let Err(f) = r {
            self.drop_session();
            self.out.push_back(Event::Disconnected(f));
        }
    }

    fn drop_session(&mut self) {
        self.pinger = None;
        self.realm_ping = None;
        // A new session numbers its requests from 1 again.
        self.realm = RealmLobby::new();
        // The market starts over with the next lobby.
        self.market = None;
        if let Ok(mut slot) = self.session.0.lock() {
            if let Some(s) = slot.take() {
                s.close();
            }
        }
    }

    fn connect(&mut self, index: usize) {
        if self.connecting.is_some() {
            return;
        }
        let Some(spec) = self.list.servers.get(index).map(|s| s.spec()) else {
            self.lost(crate::app::online::NO_LIST);
            return;
        };
        // A new connection replaces the old one.
        self.drop_session();
        self.realm_ping = realm_target(&spec);
        let (tx, rx) = mpsc::channel();
        let session = self.session.clone();
        std::thread::Builder::new()
            .name("online-connect".into())
            .spawn(move || {
                let ev = match Session::connect(&spec, &client_info(), Timing::default()) {
                    Ok(s) => {
                        let w = s.welcome();
                        let ev = Event::Connected {
                            server_name: w.server_name.clone(),
                            registration_open: w.registration_open,
                        };
                        if let Ok(mut slot) = session.0.lock() {
                            *slot = Some(s);
                        }
                        ev
                    }
                    Err(e) => Event::Disconnected(failure(&e)),
                };
                let _ = tx.send(ev);
            })
            .expect("a thread for connecting");
        self.connecting = Some(rx);
    }

    /// One S→C body as the screens' event (`None`: nothing they show).
    fn event(&mut self, body: &[u8]) -> Option<Event> {
        let k = *body.first()?;
        // The realm lobby's bodies, and the ERRORs answering its requests.
        if let Some(ev) = self.realm.feed(body) {
            return ev;
        }
        if k == d2_net::msg::kind::ERROR {
            let e = ErrorMsg::parse(body).ok()?;
            return Some(Event::Error(Failure {
                code: e.code,
                wait_s: e.wait_s,
                text: e.text,
            }));
        }
        if ranges::ACCOUNT.contains(&k) {
            return Some(match AccountEvent::parse(body).ok()? {
                AccountEvent::LoggedIn {
                    account,
                    max_characters,
                } => Event::LoggedIn {
                    account,
                    max_characters,
                },
                AccountEvent::CharList(l) => Event::Chars(l.iter().map(char_info).collect()),
                AccountEvent::CharCreated(c) => Event::CharCreated(char_info(&c)),
                AccountEvent::CharDeleted { name } => Event::CharDeleted(name),
                AccountEvent::CharSelected(c) => Event::CharSelected(char_info(&c)),
            });
        }
        if let Some(list) = node_list_event(body) {
            let list = list.ok()?.0;
            let targets = self.with_realm(list.iter().map(PingTarget::of).collect());
            self.ping(targets);
            return Some(Event::Nodes(list));
        }
        if ranges::LOBBY.contains(&k) {
            return Some(match LobbyEvent::parse(body).ok()? {
                LobbyEvent::GameListBegin { .. } => {
                    self.names.clear();
                    Event::GameListBegin
                }
                LobbyEvent::GameEntry(g) => {
                    self.names.insert(g.game_id, g.name.clone());
                    Event::Game(game_info(&g))
                }
                LobbyEvent::GameListEnd => Event::GameListEnd,
                LobbyEvent::GameGone { game_id } => Event::GameGone(game_id),
                LobbyEvent::JoinOk {
                    game_id,
                    ticket,
                    difficulty,
                } => Event::JoinOk(Joined {
                    game_id,
                    ticket,
                    difficulty,
                    name: self.names.get(&game_id).cloned().unwrap_or_default(),
                    node: None,
                }),
                LobbyEvent::ChatLine { from, text, .. } => Event::ChatLine { from, text },
                // A game on a game server: the game starts as for `JoinOk`
                // and connects there.
                LobbyEvent::JoinNode(j) => Event::JoinOk(Joined {
                    game_id: j.game_id,
                    ticket: [0; 16],
                    difficulty: j.difficulty,
                    name: self.names.get(&j.game_id).cloned().unwrap_or_default(),
                    node: Some(j),
                }),
            });
        }
        None
    }
}

impl OnlineLink for NetLink {
    fn send(&mut self, r: Request) {
        let body = match r {
            Request::Connect { index } => return self.connect(index),
            Request::Logout => {
                self.send_body(AccountRequest::Logout.body());
                self.drop_session();
                // The screens asked for it: no event.
                self.out.retain(|e| !matches!(e, Event::Disconnected(_)));
                return;
            }
            Request::Register { account, password } => {
                AccountRequest::Register { account, password }.body()
            }
            Request::Login { account, password } => {
                AccountRequest::Login { account, password }.body()
            }
            Request::CharList => AccountRequest::CharList.body(),
            Request::CharCreate {
                name,
                class,
                hardcore,
                expansion,
                ladder,
            } => AccountRequest::CharCreate {
                name,
                class,
                hardcore,
                expansion,
                ladder,
            }
            .body(),
            Request::CharDelete { name, password } => {
                AccountRequest::CharDelete { name, password }.body()
            }
            Request::CharSelect { name } => AccountRequest::CharSelect { name }.body(),
            Request::ListGames => LobbyRequest::ListGames.body(),
            Request::CreateGame {
                place: Placement::HostOnMyPc,
                ..
            } => {
                // No game hosting in this build's lobby.
                self.out.push_back(Event::Error(Failure {
                    code: node_code::HOSTING_OFF,
                    wait_s: 0,
                    text: String::new(),
                }));
                return;
            }
            Request::CreateGame {
                name,
                password,
                difficulty,
                place,
                description,
                level_min,
                level_max,
            } => LobbyRequest::CreateGame {
                name,
                password,
                difficulty,
                max_players: 0,
                place,
                description,
                level_min,
                level_max,
            }
            .body(),
            Request::JoinGame { game_id, password } => {
                LobbyRequest::JoinGame { game_id, password }.body()
            }
            Request::LeaveLobby => LobbyRequest::LeaveLobby.body(),
            Request::Chat { text } => LobbyRequest::Chat { text }.body(),
            r => return self.send_numbered(r),
        };
        self.send_body(body);
    }

    fn market(
        &mut self,
        tips: Option<crate::ui::item_tip::ItemTips>,
    ) -> Option<Box<dyn crate::market::source::MarketSource>> {
        let (inbox, outbox) = (RealmInbox::default(), RealmOutbox::default());
        self.market = Some((inbox.clone(), outbox.clone()));
        let src = crate::market::realm::RealmSource::on_reader(inbox, outbox, tips, None);
        Some(Box::new(src))
    }

    fn poll(&mut self) -> Option<Event> {
        if let Some(e) = self.out.pop_front() {
            return Some(e);
        }
        if let Some(rx) = &self.connecting {
            match rx.try_recv() {
                Ok(ev) => {
                    self.connecting = None;
                    match ev {
                        Event::Connected { .. } => self.connected(),
                        _ => self.realm_ping = None,
                    }
                    return Some(ev);
                }
                Err(mpsc::TryRecvError::Empty) => return None,
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.connecting = None;
                    return Some(Event::Disconnected(Failure {
                        code: ErrorCode(0),
                        wait_s: 0,
                        text: String::new(),
                    }));
                }
            }
        }
        if let Some(ev) = self.new_pings() {
            return Some(ev);
        }
        // What the market queued goes out first.
        let queued: Vec<Vec<u8>> = match &self.market {
            Some((_, outbox)) => std::iter::from_fn(|| outbox.pop()).collect(),
            None => Vec::new(),
        };
        for body in queued {
            self.send_body(body);
        }
        loop {
            let polled = match self.session.0.lock() {
                Ok(mut slot) => slot.as_mut().map(|s| s.poll()),
                Err(_) => None,
            };
            match polled {
                None | Some(Ok(None)) => return None,
                Some(Ok(Some(body))) => {
                    // The market's bodies are the market's (none without one).
                    if owner_of(&body) == Some(Owner::Market) {
                        if let Some((inbox, _)) = &self.market {
                            inbox.route(body);
                        }
                        continue;
                    }
                    if let Some(ev) = self.event(&body) {
                        return Some(ev);
                    }
                }
                Some(Err(e)) => {
                    self.drop_session();
                    return Some(Event::Disconnected(failure(&e)));
                }
            }
        }
    }
}

/// Registers the online screens on the real server: the launcher's list
/// (or the reason there is none, shown on the login screen) and the shared
/// session. Returns the screens' handle for the host's hooks
/// ([`Online::take_joined`], [`Online::lost`], [`Online::select_server`]).
pub fn register(reg: &mut Registry, session: &OnlineSession) -> OnlineHandle {
    let (list, problem) = match ServerList::load() {
        Ok(l) => (l, None),
        // A damaged list says so; a missing one is the login screen's own
        // text.
        Err(e) if e != crate::app::online::NO_LIST => (ServerList::default(), Some(e)),
        Err(_) => (ServerList::default(), None),
    };
    let servers = entries(&list);
    let online = Online::new(Box::new(NetLink::new(session.clone(), list)), servers);
    online.borrow_mut().state.notice = problem;
    super::register_with(reg, online.clone());
    online
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::front_end::screens::online::NodeInfo;
    use d2_net::msg::ErrorCode;

    fn link() -> NetLink {
        NetLink::new(OnlineSession::default(), ServerList::default())
    }

    /// The server's bodies become the screens' events: errors keep their
    /// code, wait and words; a join names the game from the list.
    #[test]
    fn bodies_become_events() {
        let mut l = link();
        let mut e = ErrorMsg::new(0x46, ErrorCode::WRONG_PASSWORD);
        e.wait_s = 3;
        assert_eq!(
            l.event(&e.body()),
            Some(Event::Error(Failure {
                code: ErrorCode::WRONG_PASSWORD,
                wait_s: 3,
                text: ErrorCode::WRONG_PASSWORD.text().into(),
            }))
        );
        let summary = CharSummary {
            name: "Werwer".into(),
            class: 4,
            level: 30,
            status: 0x0520,
            difficulties: 0b011,
            look: vec![1; 32],
            last_played: 9,
            season: 0,
        };
        assert_eq!(
            l.event(&AccountEvent::CharSelected(summary.clone()).body()),
            Some(Event::CharSelected(char_info(&summary)))
        );
        let g = d2_net::msg::lobby::GameInfo {
            game_id: 5,
            name: "cows".into(),
            difficulty: 1,
            players: 2,
            max_players: 8,
            locked: true,
            hardcore: false,
            expansion: true,
            joinable: 0,
            ..d2_net::msg::lobby::GameInfo::default()
        };
        assert_eq!(
            l.event(&LobbyEvent::GameEntry(g.clone()).body()),
            Some(Event::Game(game_info(&g)))
        );
        let join = LobbyEvent::JoinOk {
            game_id: 5,
            ticket: [4; 16],
            difficulty: 1,
        };
        assert_eq!(
            l.event(&join.body()),
            Some(Event::JoinOk(Joined {
                game_id: 5,
                ticket: [4; 16],
                difficulty: 1,
                name: "cows".into(),
                node: None,
            }))
        );
        assert_eq!(
            l.event(
                &LobbyEvent::ChatLine {
                    from: "A".into(),
                    text: "hi".into(),
                    at: 1
                }
                .body()
            ),
            Some(Event::ChatLine {
                from: "A".into(),
                text: "hi".into()
            })
        );
        // Bodies of other areas and broken ones show nothing.
        assert_eq!(l.event(&[0x60, 1]), None);
        assert_eq!(l.event(&[0x42]), None);
    }

    /// Without a connection a request fails at once with a reason, and a
    /// server the list doesn't have says where the list comes from.
    #[test]
    fn requests_without_a_session_fail_with_a_reason() {
        let mut l = link();
        l.send(Request::CharList);
        assert!(
            matches!(l.poll(), Some(Event::Disconnected(f)) if f.text.contains("Not connected"))
        );
        l.send(Request::Connect { index: 0 });
        assert!(matches!(
            l.poll(),
            Some(Event::Disconnected(f)) if f.text == crate::app::online::NO_LIST
        ));
        assert_eq!(l.poll(), None);
    }

    /// The main server is pinged beside its game servers when the list
    /// pins its certificate, from the connection on; a session without one
    /// pings only the game servers.
    #[test]
    fn the_main_server_is_pinged_beside_the_game_servers() {
        let spec = |trust| ServerSpec {
            id: "eu".into(),
            name: "D2Gamma Europe".into(),
            game: "d2rust".into(),
            address: "127.0.0.1:6112".into(),
            trust,
            version: String::new(),
        };
        let realm = realm_target(&spec(ServerTrust::Pinned([9; 32]))).unwrap();
        assert_eq!(
            (realm.node.as_str(), realm.addr.as_str(), realm.cert_sha256),
            (REALM_PING, "127.0.0.1:6112", [9; 32])
        );
        assert_eq!(realm_target(&spec(ServerTrust::Any)), None);

        let node = PingTarget {
            node: "eu-1".into(),
            addr: "127.0.0.1:6113".into(),
            cert_sha256: [3; 32],
        };
        let mut l = link();
        assert_eq!(l.with_realm(vec![node.clone()]), vec![node.clone()]);
        l.realm_ping = Some(realm.clone());
        assert_eq!(
            l.with_realm(vec![node.clone()]),
            [node.clone(), realm.clone()]
        );
        // A realm without game servers sends no list: its own ping starts
        // with the connection (a loopback port that never answers here).
        let silent = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        l.realm_ping = Some(PingTarget {
            addr: silent.local_addr().unwrap().to_string(),
            ..realm
        });
        assert!(l.pinger.is_none());
        l.connected();
        assert!(l.pinger.is_some());
        l.drop_session();
        assert_eq!(l.realm_ping, None);
        assert!(l.pinger.is_none());
        // Without a pinned certificate there is nothing to ping.
        l.connected();
        assert!(l.pinger.is_none());
    }

    #[test]
    fn session_failures_keep_code_wait_and_words() {
        let f = failure(&OnlineError::Server {
            request_kind: 0x21,
            code: ErrorCode::RATE_LIMITED,
            wait_s: 30,
            text: "slow down".into(),
        });
        assert_eq!((f.code, f.wait_s), (ErrorCode::RATE_LIMITED, 30));
        assert!(f.text.contains("30 seconds"));
        assert_eq!(failure(&OnlineError::TimedOut).code, ErrorCode(0));
    }

    /// The main server's game servers become the screens' list and start
    /// the pings (one per listed server, a loopback port that never
    /// answers here); hosting a game is refused in this build.
    #[test]
    fn the_server_list_starts_the_pings_and_hosting_is_refused() {
        use d2_net::msg::nodelist::NodeList;
        let mut l = link();
        let silent = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        let node = NodeInfo {
            node: "hel-1".into(),
            name: "D2RUST Helsinki".into(),
            location: "Helsinki, Finland".into(),
            ping_addr: silent.local_addr().unwrap().to_string(),
            cert_sha256: [3; 32],
            load_pct: 10,
            state: Default::default(),
            suggested: true,
        };
        let body = NodeList(vec![node.clone()]).body();
        assert_eq!(l.event(&body), Some(Event::Nodes(vec![node])));
        assert!(l.pinger.is_some());
        // An empty list keeps no pinger to start with.
        let mut l2 = link();
        assert_eq!(
            l2.event(&NodeList::default().body()),
            Some(Event::Nodes(vec![]))
        );
        assert!(l2.pinger.is_none());
        l.send(Request::CreateGame {
            name: "cows".into(),
            password: String::new(),
            difficulty: 0,
            place: Placement::HostOnMyPc,
            description: String::new(),
            level_min: 0,
            level_max: 0,
        });
        assert!(matches!(
            l.poll(),
            Some(Event::Error(f)) if f.code == node_code::HOSTING_OFF
        ));
        // Logging out stops the pings.
        l.send(Request::Logout);
        assert!(l.pinger.is_none());
    }
}
