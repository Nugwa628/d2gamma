//! The online screens: login and account creation, the account's character
//! list, character creation, and the lobby (game list, create game, join
//! game, password prompt, chat). The flow (`flow::next`):
//!
//! ```text
//! main menu "ONLINE" / `play --online` -> login -> characters -> lobby -> game
//!                                          |          |  ^        |
//!                                          |          v  |        +-- create-game panel, password prompt
//!                                          |        create        (pop-ups of the lobby)
//! ```
//!
//! Back always goes one step up: lobby -> characters -> login (logs out) ->
//! main menu. A failed request shows the error's text in the game's message
//! box and the screen stays where it was; a lost connection goes back to
//! login with the reason.
//!
//! All screens share one [`OnlineHandle`]: the connection ([`OnlineLink`])
//! and what the server told so far ([`OnlineState`]). The online character
//! never touches the save folder; the create screen runs with its own sink.

pub mod backdrop;
pub mod chars;
pub mod chat_pane;
pub mod create;
pub mod fake;
pub mod friends_panel;
pub mod guild_panel;
pub mod ladder_panel;
pub mod layout;
pub mod lobby;
pub mod login;
pub mod market_host;
pub mod server_drop;
pub mod text_fit;
pub mod widgets;

pub mod link;
#[cfg(feature = "online")]
pub mod net_link;

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

pub use link::{
    failure_text, CharInfo, ErrorCode, Event, Failure, GameInfo, Joined, NoLink, NodeInfo,
    NodeState, OnlineLink, PingTable, Placement, Request, ServerEntry, PLAYER_HOSTED_NOTE,
};

use crate::ui::front_end::{Action, Registry, ScreenId};

pub const ONLINE_LOGIN: ScreenId = ScreenId("online_login");
pub const ONLINE_CHARS: ScreenId = ScreenId("online_chars");
pub const ONLINE_CREATE: ScreenId = ScreenId("online_create");
pub const ONLINE_LOBBY: ScreenId = ScreenId("online_lobby");

/// The chat lines the lobby keeps (no history on the server).
pub const CHAT_KEEP: usize = 64;

/// The switch for hosting a game on the player's own PC. Off: the create
/// form has no **Host on my PC** row, so no hosting is offered anywhere
/// (player hosting is not designed yet; the code behind it stays).
pub const PLAYER_HOSTING: bool = false;

/// The realm's own entry in [`PingTable`]: the link pings the realm too, so
/// a player-hosted game's ping can be estimated (lobby contract section 7).
pub const REALM_PING: &str = "@realm";

/// What the server has told the client so far.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OnlineState {
    /// The servers of `servers.toml` (empty: the file is missing).
    pub servers: Vec<ServerEntry>,
    /// The server the login screen uses.
    pub server: usize,
    /// A handshake is done.
    pub connected: bool,
    pub server_name: String,
    pub registration_open: bool,
    /// Logged in as (the stored spelling).
    pub account: Option<String>,
    pub max_characters: u8,
    /// The account's characters, as the server last listed them.
    pub chars: Vec<CharInfo>,
    /// The character the lobby runs with (`CharSelected`).
    pub selected: Option<CharInfo>,
    /// The open games, every kind (the ones the character can't join
    /// with the reason).
    pub games: Vec<GameInfo>,
    /// The main server's official game servers (empty: a server without
    /// game servers, v1).
    pub nodes: Vec<NodeInfo>,
    /// The client's pings to them.
    pub pings: PingTable,
    /// Lobby chat, oldest first: (from, text).
    pub chat: VecDeque<(String, String)>,
    /// The server answered `Chat` with `NotSupported`: the chat box hides.
    pub chat_off: bool,
    /// The game the server let the character into; the host takes it
    /// ([`Online::take_joined`]) when the front end ends in a game load.
    pub joined: Option<Joined>,
    /// A message the next online screen shows when it opens (the reason a
    /// connection was lost or a game closed).
    pub notice: Option<String>,
}

impl OnlineState {
    /// The ping of official server `node` (see [`PingTable`]).
    pub fn ping(&self, node: &str) -> Option<Option<u32>> {
        self.pings.get(node).copied()
    }

    /// The official server a listed game runs on (by its id, else by its
    /// name; `None` for a player-hosted game or a v1 server).
    pub fn node_of(&self, g: &GameInfo) -> Option<&NodeInfo> {
        if g.player_hosted {
            return None;
        }
        if !g.node_id.is_empty() {
            return self.nodes.iter().find(|n| n.node == g.node_id);
        }
        if g.server.is_empty() {
            return None;
        }
        self.nodes.iter().find(|n| n.name == g.server)
    }

    /// A player-hosted game's estimated ping: the client's round trip to
    /// the realm plus the host's (`None` while either is unknown). The
    /// realm never lists a host's address, so the client can't ping it.
    pub fn host_ping(&self, g: &GameInfo) -> Option<u32> {
        let realm = self.ping(REALM_PING).flatten()?;
        (g.host_rtt_ms > 0).then(|| realm + u32::from(g.host_rtt_ms))
    }

    /// The automatic choice for a new game with the pings so far.
    pub fn auto_node(&self) -> Option<&NodeInfo> {
        d2_net::msg::nodelist::pick_auto(&self.nodes, &|id| self.ping(id).flatten())
    }
}

/// The connection and the state, shared by every online screen.
pub struct Online {
    pub link: Box<dyn OnlineLink>,
    pub state: OnlineState,
    /// The config folder the lobby saves its panel layouts in
    /// (`layouts.toml`); `None`: they are not saved (tests, captures).
    pub layouts_dir: Option<std::path::PathBuf>,
    /// What the lobby's market window needs from the game's files (plan
    /// L18), set by the host; `None`: the window draws without them.
    pub market_parts: Option<market_host::PartsLoader>,
}

pub type OnlineHandle = Rc<RefCell<Online>>;

impl Online {
    pub fn new(link: Box<dyn OnlineLink>, servers: Vec<ServerEntry>) -> OnlineHandle {
        Rc::new(RefCell::new(Online {
            link,
            state: OnlineState {
                servers,
                ..OnlineState::default()
            },
            layouts_dir: None,
            market_parts: None,
        }))
    }

    pub fn send(&mut self, r: Request) {
        self.link.send(r);
    }

    /// Reads every event that arrived, applies the ones that only update the
    /// state (list entries, chat, the character list), and returns them all
    /// in order for the current screen to react to.
    pub fn take_events(&mut self) -> Vec<Event> {
        let mut out = Vec::new();
        while let Some(ev) = self.link.poll() {
            self.apply(&ev);
            out.push(ev);
        }
        out
    }

    fn apply(&mut self, ev: &Event) {
        let s = &mut self.state;
        match ev {
            Event::Connected {
                server_name,
                registration_open,
            } => {
                s.connected = true;
                s.server_name = server_name.clone();
                s.registration_open = *registration_open;
            }
            Event::LoggedIn {
                account,
                max_characters,
            } => {
                s.account = Some(account.clone());
                s.max_characters = *max_characters;
            }
            Event::Chars(list) => s.chars = list.clone(),
            Event::CharCreated(c) => {
                s.chars.retain(|o| !o.name.eq_ignore_ascii_case(&c.name));
                s.chars.insert(0, c.clone());
            }
            Event::CharDeleted(name) => s.chars.retain(|o| !o.name.eq_ignore_ascii_case(name)),
            Event::CharSelected(c) => {
                s.selected = Some(c.clone());
                s.games.clear();
            }
            Event::GameListBegin => s.games.clear(),
            Event::Game(g) => match s.games.iter_mut().find(|o| o.id == g.id) {
                Some(o) => *o = g.clone(),
                None => s.games.push(g.clone()),
            },
            Event::GameListEnd => {}
            Event::GameGone(id) => s.games.retain(|g| g.id != *id),
            Event::Nodes(list) => {
                s.pings
                    .retain(|id, _| id == REALM_PING || list.iter().any(|n| n.node == *id));
                s.nodes = list.clone();
            }
            Event::Pings(t) => s.pings = t.clone(),
            Event::JoinOk(j) => s.joined = Some(j.clone()),
            Event::ChatLine { from, text } => {
                s.chat.push_back((from.clone(), text.clone()));
                while s.chat.len() > CHAT_KEEP {
                    s.chat.pop_front();
                }
            }
            Event::Error(f) if f.code == link::ErrorCode::NOT_SUPPORTED => s.chat_off = true,
            Event::Error(_) => {}
            Event::Disconnected(f) => {
                let servers = std::mem::take(&mut s.servers);
                let server = s.server;
                *s = OnlineState {
                    servers,
                    server,
                    notice: Some(failure_text(f)),
                    ..OnlineState::default()
                };
            }
            // The realm lobby's chat, guilds, friends and ladder: the
            // screens that show them keep their own state.
            _ => {}
        }
    }

    /// The game the lobby joined, once (the host starts the game stream
    /// with its ticket).
    pub fn take_joined(&mut self) -> Option<Joined> {
        self.state.joined.take()
    }

    /// The host lost the connection (during a game, or before the front
    /// end opened): the state starts over and the login screen shows
    /// `text`.
    pub fn lost(&mut self, text: &str) {
        self.apply(&Event::Disconnected(Failure {
            code: ErrorCode(0),
            wait_s: 0,
            text: text.to_string(),
        }));
    }

    /// The login screen starts on the server with id `id` (`play --online
    /// --server ID`); an unknown id keeps the first.
    pub fn select_server(&mut self, id: &str) {
        if let Some(i) = self.state.servers.iter().position(|s| s.id == id) {
            self.state.server = i;
        }
    }
}

/// The UI element name of an online screen's own control (the ids of
/// `screens::element_ids`); `None` for anything else, the select and create
/// screens' controls under the online list and create included.
pub fn control_name(screen: ScreenId, action: Action) -> Option<String> {
    let Action::Custom(a) = action else {
        return None;
    };
    let name = match screen {
        s if s == ONLINE_LOGIN => {
            use login::act::*;
            match a {
                LOGIN => "login".into(),
                NEW_ACCOUNT => "new_account".into(),
                SERVER => "server".into(),
                BACK => "back".into(),
                MESSAGE_OK => "message_ok".into(),
                CANCEL => "cancel".into(),
                f if (FIELD..FIELD + 3).contains(&f) => format!("field({})", f - FIELD),
                _ => return None,
            }
        }
        s if s == ONLINE_CHARS => {
            use chars::act::*;
            match a {
                MESSAGE_OK => "message_ok".into(),
                CANCEL => "cancel".into(),
                DELETE_OK => "delete_ok".into(),
                DELETE_CANCEL => "delete_cancel".into(),
                FIELD => "field(0)".into(),
                _ => return None,
            }
        }
        s if s == ONLINE_CREATE => {
            use create::act::*;
            match a {
                MESSAGE_OK => "message_ok".into(),
                CANCEL => "cancel".into(),
                LADDER => "ladder".into(),
                _ => return None,
            }
        }
        s if s == ONLINE_LOBBY => {
            use lobby::act::*;
            match a {
                BACK => "back".into(),
                PRIMARY => "primary".into(),
                TAB_CREATE => "tab_create".into(),
                TAB_JOIN => "tab_join".into(),
                TAB_LADDER => "tab_ladder".into(),
                TAB_FRIENDS => "tab_friends".into(),
                MARKET => "market".into(),
                LOCK => "lock_panels".into(),
                RESET => "reset_panels".into(),
                PICKER => "background".into(),
                HD_TEXT => "hd_text".into(),
                MESSAGE_OK => "message_ok".into(),
                CANCEL => "cancel".into(),
                PROMPT_OK => "prompt_ok".into(),
                PROMPT_CANCEL => "prompt_cancel".into(),
                SCROLL_UP => "scroll_up".into(),
                SCROLL_DOWN => "scroll_down".into(),
                SERVER_AUTO => "server.auto".into(),
                SERVER_HOST => "server.host".into(),
                SERVER_BOX => "server".into(),
                SERVER_CLOSE => "server.close".into(),
                s if (SERVER..SERVER + lobby::SERVER_ROWS as u32).contains(&s) => {
                    format!("server({})", s - SERVER)
                }
                f if (FIELD..FIELD + 5).contains(&f) => format!("field({})", f - FIELD),
                p if (PANEL..PANEL_END).contains(&p) => format!("panel({})", p - PANEL),
                d if (DIFFICULTY..DIFFICULTY + 3).contains(&d) => format!(
                    "difficulty.{}",
                    link::difficulty_name((d - DIFFICULTY) as u8).to_ascii_lowercase()
                ),
                _ => return None,
            }
        }
        _ => return None,
    };
    Some(name)
}

/// Register the online screens on `online`.
pub fn register_with(reg: &mut Registry, online: OnlineHandle) {
    login::register(reg, online.clone());
    chars::register(reg, online.clone());
    create::register(reg, online.clone());
    lobby::register(reg, online);
}

/// Register the online screens with no link: logging in says online play is
/// not in this build. The host replaces them with [`register_with`].
pub fn register(reg: &mut Registry) {
    register_with(reg, Online::new(Box::<NoLink>::default(), Vec::new()));
}
