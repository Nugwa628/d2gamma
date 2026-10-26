//! The lobby on a [`Session`]: typed calls for the lobby's requests
//! (contracts section 3.6) and [`GameList`], the open-games list kept from
//! the snapshot and the updates. The game's online screens and the MP test
//! tool's bots both use these, so they speak the same protocol.

use std::collections::BTreeMap;
use std::time::Duration;

use d2_net::msg::kind as ranges;
use d2_net::msg::lobby::{kind, GameInfo, JoinNode, LobbyEvent, LobbyRequest};
use d2_net::msg::nodelist::{self, NodeList};
use d2_net::NetError;

use crate::connect::Session;
use crate::OnlineError;

/// What the server answered to a create or join: the seat is held;
/// [`Session::enter`] opens its game stream.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JoinOk {
    pub game_id: u32,
    pub difficulty: u8,
    pub to: JoinTo,
}

/// Where a held seat is entered.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum JoinTo {
    /// The dedicated server's `JoinOk`: a game stream on the session's own
    /// connection, attached with this ticket ([`Session::join`]).
    Ticket([u8; 16]),
    /// The main server's `JoinNode`: a connection to the game server the
    /// game runs on ([`Session::join_node`]).
    Node(JoinNode),
}

impl JoinOk {
    /// The answer in a `JoinOk` or `JoinNode` body; `None` for any other.
    pub fn parse(body: &[u8]) -> Option<Self> {
        match LobbyEvent::parse(body).ok()? {
            LobbyEvent::JoinOk {
                game_id,
                ticket,
                difficulty,
            } => Some(Self {
                game_id,
                difficulty,
                to: JoinTo::Ticket(ticket),
            }),
            LobbyEvent::JoinNode(j) => Some(Self {
                game_id: j.game_id,
                difficulty: j.difficulty,
                to: JoinTo::Node(j),
            }),
            _ => None,
        }
    }
}

/// A lobby message among the bodies [`Session::poll`] returns: `None` for a
/// body of another area, a game-server list ([`node_list_event`]) or the
/// ladder's (inside the lobby's range, [`crate::LadderClient`]).
pub fn lobby_event(body: &[u8]) -> Option<Result<LobbyEvent, NetError>> {
    let k = *body.first()?;
    (ranges::LOBBY.contains(&k)
        && !ranges::LADDER.contains(&k)
        && k != kind::LIST_GAMES
        && k != nodelist::kind::NODE_LIST)
        .then(|| LobbyEvent::parse(body))
}

/// The main server's game-server list among the bodies [`Session::poll`]
/// returns: `None` for any other body.
pub fn node_list_event(body: &[u8]) -> Option<Result<NodeList, NetError>> {
    (*body.first()? == nodelist::kind::NODE_LIST).then(|| NodeList::parse(body))
}

impl Session {
    fn lobby(&mut self, r: LobbyRequest) -> Result<(), OnlineError> {
        self.send(r.body())
    }

    fn join_answer(&mut self, r: LobbyRequest, wait: Duration) -> Result<JoinOk, OnlineError> {
        let body = self.request(r.body(), &[kind::JOIN_OK, kind::JOIN_NODE], wait)?;
        JoinOk::parse(&body).ok_or(OnlineError::Malformed)
    }

    /// Subscribe to the game list: a snapshot comes, then updates, as
    /// bodies from [`Session::poll`] ([`lobby_event`], [`GameList`]).
    pub fn list_games(&mut self) -> Result<(), OnlineError> {
        self.lobby(LobbyRequest::ListGames)
    }

    /// Create a game (empty `password`: public; `max_players` 0: the
    /// server's default) and hold the creator's seat. The main server
    /// places it; no description, no level range.
    pub fn create_game(
        &mut self,
        name: &str,
        password: &str,
        difficulty: u8,
        max_players: u8,
        wait: Duration,
    ) -> Result<JoinOk, OnlineError> {
        self.join_answer(
            LobbyRequest::CreateGame {
                name: name.into(),
                password: password.into(),
                difficulty,
                max_players,
                place: nodelist::Placement::Automatic,
                description: String::new(),
                level_min: 0,
                level_max: 0,
            },
            wait,
        )
    }

    /// Join an open game (empty `password` for a public one).
    pub fn join_game(
        &mut self,
        game_id: u32,
        password: &str,
        wait: Duration,
    ) -> Result<JoinOk, OnlineError> {
        self.join_answer(
            LobbyRequest::JoinGame {
                game_id,
                password: password.into(),
            },
            wait,
        )
    }

    /// Stop the list updates (into a game, or back to character select).
    pub fn leave_lobby(&mut self) -> Result<(), OnlineError> {
        self.lobby(LobbyRequest::LeaveLobby)
    }

    /// One chat line to the lobby (1-200 bytes).
    pub fn chat(&mut self, text: &str) -> Result<(), OnlineError> {
        self.lobby(LobbyRequest::Chat { text: text.into() })
    }
}

/// The open games as the lobby's messages describe them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GameList {
    games: BTreeMap<u32, GameInfo>,
    /// The snapshot's end arrived.
    pub complete: bool,
}

impl GameList {
    /// Apply one lobby message (others are ignored).
    pub fn apply(&mut self, ev: &LobbyEvent) {
        match ev {
            LobbyEvent::GameListBegin { .. } => {
                self.games.clear();
                self.complete = false;
            }
            LobbyEvent::GameEntry(g) => {
                self.games.insert(g.game_id, g.clone());
            }
            LobbyEvent::GameListEnd => self.complete = true,
            LobbyEvent::GameGone { game_id } => {
                self.games.remove(game_id);
            }
            LobbyEvent::JoinOk { .. } | LobbyEvent::JoinNode(_) | LobbyEvent::ChatLine { .. } => {}
        }
    }

    /// The games, by id (creation order).
    pub fn games(&self) -> impl Iterator<Item = &GameInfo> {
        self.games.values()
    }

    pub fn get(&self, id: u32) -> Option<&GameInfo> {
        self.games.get(&id)
    }

    /// A game by name, any case.
    pub fn by_name(&self, name: &str) -> Option<&GameInfo> {
        self.games
            .values()
            .find(|g| g.name.eq_ignore_ascii_case(name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn g(id: u32, name: &str) -> GameInfo {
        GameInfo {
            game_id: id,
            name: name.into(),
            difficulty: 0,
            players: 1,
            max_players: 8,
            locked: false,
            hardcore: false,
            expansion: true,
            joinable: 0,
            ..GameInfo::default()
        }
    }

    #[test]
    fn list_follows_snapshot_and_updates() {
        let mut l = GameList::default();
        for ev in [
            LobbyEvent::GameListBegin { count: 2 },
            LobbyEvent::GameEntry(g(1, "a")),
            LobbyEvent::GameEntry(g(2, "b")),
            LobbyEvent::GameListEnd,
        ] {
            l.apply(&ev);
        }
        assert!(l.complete);
        assert_eq!(l.games().count(), 2);
        l.apply(&LobbyEvent::GameEntry(GameInfo {
            players: 5,
            ..g(2, "b")
        }));
        assert_eq!(l.get(2).unwrap().players, 5);
        l.apply(&LobbyEvent::GameGone { game_id: 1 });
        assert!(l.get(1).is_none());
        assert_eq!(l.by_name("B").unwrap().game_id, 2);
        l.apply(&LobbyEvent::GameListBegin { count: 0 });
        assert!(!l.complete);
        assert_eq!(l.games().count(), 0);
    }

    #[test]
    fn lobby_bodies_are_recognised() {
        let b = LobbyEvent::GameGone { game_id: 3 }.body();
        assert_eq!(
            lobby_event(&b).unwrap().unwrap(),
            LobbyEvent::GameGone { game_id: 3 }
        );
        assert!(lobby_event(&[0x22, 0]).is_none());
        assert!(lobby_event(&[]).is_none());
        assert!(lobby_event(&[0x5F]).unwrap().is_err());
    }

    #[test]
    fn the_server_list_is_its_own_event() {
        let b = NodeList::default().body();
        assert!(lobby_event(&b).is_none());
        assert_eq!(node_list_event(&b).unwrap().unwrap(), NodeList::default());
        assert!(node_list_event(&LobbyEvent::GameListEnd.body()).is_none());
        assert!(node_list_event(&[nodelist::kind::NODE_LIST, 1])
            .unwrap()
            .is_err());
    }
}
