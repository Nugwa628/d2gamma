//! The fake's accounts, characters and game list: registration and login,
//! the character list, and the lobby's games with the contract's join
//! rules (every open game is listed, the ones the selected character
//! can't join with the reason).

use d2_formats::d2s::status;

use super::super::link::{
    lobby_code, node_code, CharInfo, ErrorCode, Event, Failure, GameInfo, Joined, NodeInfo,
    NodeState, PingTable, Placement, Request,
};
use super::{character, fail, FakeServer, Game};

/// The round trip the fake gives a player host to the realm.
const HOST_RTT_MS: u16 = 42;

/// Game descriptions: at most 40 bytes (lobby contract section 7).
const MAX_DESCRIPTION: usize = 40;

/// Account names: 2-15 ASCII letters, digits, `-`, `_`, starting with a
/// letter (contract section 4).
pub fn account_name_ok(n: &str) -> bool {
    (2..=15).contains(&n.len())
        && n.starts_with(|c: char| c.is_ascii_alphabetic())
        && n.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// Game names: 1-15 letters, digits, space, `-`, `_` (contract section 6).
pub fn game_name_ok(n: &str) -> bool {
    (1..=15).contains(&n.chars().count())
        && n.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == ' ' || c == '-' || c == '_')
        && !n.trim().is_empty()
}

/// Why character `c` can't join game `g` (0: it can), in the lobby
/// contract's order: dead hardcore, kind, difficulty, level range, full.
pub fn join_rule(c: &CharInfo, g: &GameInfo) -> ErrorCode {
    let hc = c.status & status::HARDCORE != 0;
    let exp = c.status & status::EXPANSION != 0;
    if c.status & (status::HARDCORE | status::DEAD) == (status::HARDCORE | status::DEAD) {
        ErrorCode::DEAD_HARDCORE
    } else if hc != g.hardcore || exp != g.expansion {
        ErrorCode::WRONG_GAME_KIND
    } else if !status::difficulty_open(c.difficulties, g.difficulty) {
        ErrorCode::DIFFICULTY_LOCKED
    } else if !in_range(c.level, g.level_min, g.level_max) {
        lobby_code::LEVEL_RANGE
    } else if g.players >= g.max_players {
        ErrorCode::GAME_FULL
    } else {
        ErrorCode(0)
    }
}

/// `level` inside a game's range (0: no bound).
fn in_range(level: u8, min: u8, max: u8) -> bool {
    (min == 0 || level >= min) && (max == 0 || level <= max)
}

impl FakeServer {
    /// A game somebody else opened.
    pub fn add_game(&mut self, info: GameInfo, password: &str) -> u32 {
        let id = self.next_game;
        self.next_game += 1;
        self.games.push(Game {
            info: GameInfo { id, ..info },
            password: password.into(),
        });
        id
    }

    /// A game closes (the lobby gets `GameGone`).
    pub fn close_game(&mut self, id: u32) {
        self.games.retain(|g| g.info.id != id);
        if self.in_lobby {
            self.out.push_back(Event::GameGone(id));
        }
    }

    /// Somebody chats in the old one-room lobby chat.
    pub fn chat_from(&mut self, from: &str, text: &str) {
        if self.in_lobby {
            self.out.push_back(Event::ChatLine {
                from: from.into(),
                text: text.into(),
            });
        }
    }

    /// The game servers change (sent at once to a selected character).
    pub fn set_nodes(&mut self, nodes: Vec<NodeInfo>, pings: PingTable) {
        self.nodes = nodes;
        self.pings = pings;
        if self.selected.is_some() {
            self.send_nodes();
        }
    }

    fn send_nodes(&mut self) {
        if !self.nodes.is_empty() {
            self.out.push_back(Event::Nodes(self.nodes.clone()));
            self.out.push_back(Event::Pings(self.pings.clone()));
        }
    }

    /// Where a new game goes: the server's name, its id (empty for a
    /// player host) and whether a player hosts it, or why it can't.
    fn place(&self, place: &Placement, host: &str) -> Result<(String, String, bool), ErrorCode> {
        let named = |id: &str| self.nodes.iter().find(|n| n.node == id);
        let on = |n: &NodeInfo| (n.name.clone(), n.node.clone(), false);
        match place {
            Placement::HostOnMyPc if self.hosting => Ok((host.to_string(), String::new(), true)),
            Placement::HostOnMyPc => Err(node_code::HOSTING_OFF),
            Placement::Official(id) => match named(id).map(|n| (n, n.state)) {
                Some((n, NodeState::Open)) => Ok(on(n)),
                Some((_, NodeState::Full)) => Err(node_code::NODE_FULL),
                Some((_, NodeState::Draining)) => Err(node_code::NODE_DRAINING),
                _ => Err(node_code::NODE_UNAVAILABLE),
            },
            _ if self.nodes.is_empty() => Ok(Default::default()),
            _ => {
                // The client's pick when it still takes games, else the
                // first open one.
                let pick = place.node().and_then(named).filter(|n| n.open());
                match pick.or_else(|| self.nodes.iter().find(|n| n.open())) {
                    Some(n) => Ok(on(n)),
                    None => Err(node_code::NODE_UNAVAILABLE),
                }
            }
        }
    }

    /// The connection drops.
    pub fn drop_connection(&mut self, text: &str) {
        self.connected = false;
        self.me = None;
        self.in_lobby = false;
        self.out.push_back(Event::Disconnected(Failure {
            code: ErrorCode(0),
            wait_s: 0,
            text: text.into(),
        }));
    }

    /// What the selected character sees for a game.
    fn view(&self, g: &Game) -> GameInfo {
        let mut info = g.info.clone();
        info.joinable = self.selected.as_ref().map_or(0, |c| join_rule(c, &info).0);
        info
    }

    /// A game's `joinable` for the selected character (0 when there is no
    /// such game).
    pub(super) fn joinable(&self, game_id: u32) -> u16 {
        self.games
            .iter()
            .find(|g| g.info.id == game_id)
            .map_or(0, |g| self.view(g).joinable)
    }

    /// Why the form's create fields are refused (`None`: they pass).
    fn create_rule(&self, c: &CharInfo, description: &str, min: u8, max: u8) -> Option<ErrorCode> {
        if description.len() > MAX_DESCRIPTION || description.chars().any(char::is_control) {
            Some(lobby_code::BAD_TEXT)
        } else if (min != 0 && max != 0 && min > max) || !in_range(c.level, min, max) {
            Some(lobby_code::LEVEL_RANGE)
        } else {
            None
        }
    }

    /// Accounts, characters and the game list.
    pub(super) fn answer_lobby(&mut self, r: Request) {
        let me = self.me;
        match r {
            Request::Connect { .. } => {
                self.connected = true;
                self.out.push_back(Event::Connected {
                    server_name: self.server_name.clone(),
                    registration_open: self.registration_open,
                });
            }
            Request::Register { account, password } => {
                if !self.registration_open {
                    self.out
                        .push_back(fail(ErrorCode::REGISTRATION_CLOSED, "registration closed"));
                } else if !account_name_ok(&account) {
                    self.out.push_back(fail(ErrorCode::BAD_ACCOUNT_NAME, ""));
                } else if !(8..=64).contains(&password.len()) {
                    self.out
                        .push_back(fail(ErrorCode::BAD_PASSWORD, "bad password"));
                } else if self
                    .accounts
                    .iter()
                    .any(|a| a.name.eq_ignore_ascii_case(&account))
                {
                    self.out
                        .push_back(fail(ErrorCode::ACCOUNT_NAME_TAKEN, "taken"));
                } else {
                    self.add_account(&account, &password, Vec::new());
                    self.me = Some(self.accounts.len() - 1);
                    self.out.push_back(Event::LoggedIn {
                        account,
                        max_characters: self.max_characters,
                    });
                }
            }
            Request::Login { account, password } => {
                match self
                    .accounts
                    .iter()
                    .position(|a| a.name.eq_ignore_ascii_case(&account) && a.password == password)
                {
                    Some(i) => {
                        self.me = Some(i);
                        self.out.push_back(Event::LoggedIn {
                            account: self.accounts[i].name.clone(),
                            max_characters: self.max_characters,
                        });
                    }
                    None => self
                        .out
                        .push_back(fail(ErrorCode::WRONG_LOGIN, "wrong login")),
                }
            }
            Request::Logout => {
                self.connected = false;
                self.me = None;
                self.selected = None;
                self.in_lobby = false;
                self.social.session_ends();
            }
            _ if me.is_none() => self.out.push_back(fail(ErrorCode::NOT_LOGGED_IN, "")),
            Request::CharList => {
                let a = &self.accounts[me.unwrap_or(0)];
                self.out.push_back(Event::Chars(a.chars.clone()));
            }
            Request::CharCreate {
                name,
                class,
                hardcore,
                expansion,
                ladder,
            } => {
                let season = self
                    .ladder
                    .seasons
                    .iter()
                    .find(|s| s.current && s.season != 0)
                    .map(|s| s.season);
                let taken = self
                    .accounts
                    .iter()
                    .flat_map(|a| &a.chars)
                    .any(|c| c.name.eq_ignore_ascii_case(&name));
                let a = &mut self.accounts[me.unwrap_or(0)];
                if taken {
                    self.out.push_back(fail(ErrorCode::CHAR_NAME_TAKEN, ""));
                } else if a.chars.len() >= usize::from(self.max_characters) {
                    self.out.push_back(fail(ErrorCode::CHAR_LIMIT, ""));
                } else if ladder && season.is_none() {
                    self.out.push_back(fail(lobby_code::NO_SUCH_SEASON, ""));
                } else {
                    let mut flags = 0;
                    if hardcore {
                        flags |= status::HARDCORE;
                    }
                    if expansion {
                        flags |= status::EXPANSION;
                    }
                    let mut c = character(&name, class, 1, 0, flags);
                    if ladder {
                        c.season = season.unwrap_or(0);
                    }
                    a.chars.insert(0, c.clone());
                    self.out.push_back(Event::CharCreated(c));
                }
            }
            Request::CharDelete { name, password } => {
                let a = &mut self.accounts[me.unwrap_or(0)];
                if a.password != password {
                    self.out.push_back(fail(ErrorCode::WRONG_LOGIN, ""));
                } else if let Some(i) = a
                    .chars
                    .iter()
                    .position(|c| c.name.eq_ignore_ascii_case(&name))
                {
                    let c = a.chars.remove(i);
                    self.out.push_back(Event::CharDeleted(c.name));
                } else {
                    self.out.push_back(fail(ErrorCode::UNKNOWN_CHAR, ""));
                }
            }
            Request::CharSelect { name } => {
                let a = &self.accounts[me.unwrap_or(0)];
                match a.chars.iter().find(|c| c.name.eq_ignore_ascii_case(&name)) {
                    Some(c) => {
                        self.selected = Some(c.clone());
                        self.out.push_back(Event::CharSelected(c.clone()));
                        self.send_nodes();
                    }
                    None => self.out.push_back(fail(ErrorCode::UNKNOWN_CHAR, "")),
                }
            }
            _ if self.selected.is_none() => {
                self.out.push_back(fail(ErrorCode::NO_CHAR_SELECTED, ""))
            }
            Request::ListGames => {
                self.in_lobby = true;
                self.out.push_back(Event::GameListBegin);
                let views: Vec<GameInfo> = self.games.iter().map(|g| self.view(g)).collect();
                for v in views {
                    self.out.push_back(Event::Game(v));
                }
                self.out.push_back(Event::GameListEnd);
            }
            Request::LeaveLobby => self.in_lobby = false,
            Request::CreateGame {
                name,
                password,
                difficulty,
                place,
                description,
                level_min,
                level_max,
            } => {
                let c = self
                    .selected
                    .clone()
                    .unwrap_or_else(|| character("", 0, 1, 0, 0));
                let fields = self.create_rule(&c, &description, level_min, level_max);
                if c.status & (status::HARDCORE | status::DEAD) == (status::HARDCORE | status::DEAD)
                {
                    self.out.push_back(fail(ErrorCode::DEAD_HARDCORE, ""));
                } else if !game_name_ok(&name) {
                    self.out.push_back(fail(ErrorCode::BAD_GAME_NAME, ""));
                } else if password.len() > 15 {
                    self.out.push_back(fail(
                        ErrorCode::OTHER,
                        "Game passwords are at most 15 characters.",
                    ));
                } else if !status::difficulty_open(c.difficulties, difficulty) {
                    self.out.push_back(fail(ErrorCode::DIFFICULTY_LOCKED, ""));
                } else if let Some(code) = fields {
                    self.out.push_back(fail(code, ""));
                } else if self
                    .games
                    .iter()
                    .any(|g| g.info.name.eq_ignore_ascii_case(&name))
                {
                    self.out.push_back(fail(ErrorCode::GAME_NAME_TAKEN, ""));
                } else {
                    match self.place(&place, &c.name) {
                        Err(code) => self.out.push_back(fail(code, "")),
                        Ok((server, node_id, player_hosted)) => {
                            let info = GameInfo {
                                id: 0,
                                name: name.clone(),
                                difficulty,
                                players: 1,
                                max_players: 8,
                                locked: !password.is_empty(),
                                hardcore: c.status & status::HARDCORE != 0,
                                expansion: c.status & status::EXPANSION != 0,
                                joinable: 0,
                                server,
                                player_hosted,
                                node_id,
                                host_rtt_ms: if player_hosted { HOST_RTT_MS } else { 0 },
                                description,
                                level_min,
                                level_max,
                                created_ms: self.now_ms,
                            };
                            let id = self.add_game(info, &password);
                            self.in_lobby = false;
                            self.out.push_back(Event::JoinOk(Joined {
                                game_id: id,
                                ticket: [7; 16],
                                difficulty,
                                name,
                                node: None,
                            }));
                        }
                    }
                }
            }
            Request::JoinGame { game_id, password } => {
                let c = self
                    .selected
                    .clone()
                    .unwrap_or_else(|| character("", 0, 1, 0, 0));
                match self.games.iter().find(|g| g.info.id == game_id) {
                    None => self.out.push_back(fail(ErrorCode::GAME_NOT_FOUND, "")),
                    Some(g) => {
                        let rule = join_rule(&c, &g.info);
                        if rule != ErrorCode(0) {
                            self.out.push_back(fail(rule, ""));
                        } else if g.info.locked && password.is_empty() {
                            self.out.push_back(fail(ErrorCode::PASSWORD_NEEDED, ""));
                        } else if g.info.locked && g.password != password {
                            self.out.push_back(fail(ErrorCode::WRONG_PASSWORD, ""));
                        } else {
                            let j = Joined {
                                game_id,
                                ticket: [9; 16],
                                difficulty: g.info.difficulty,
                                name: g.info.name.clone(),
                                node: None,
                            };
                            self.in_lobby = false;
                            self.out.push_back(Event::JoinOk(j));
                        }
                    }
                }
            }
            Request::Chat { text } => {
                if self.chat_off {
                    self.out
                        .push_back(fail(ErrorCode::NOT_SUPPORTED, "chat is not built"));
                } else {
                    let from = self
                        .selected
                        .as_ref()
                        .map(|c| c.name.clone())
                        .unwrap_or_default();
                    self.out.push_back(Event::ChatLine { from, text });
                }
            }
            // The realm lobby's requests are `answer_realm_lobby`'s.
            _ => {}
        }
    }
}
