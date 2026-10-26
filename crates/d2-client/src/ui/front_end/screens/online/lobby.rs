//! The lobby: on the 16:9 canvas the host gives it ([`Screen::wide`]) a
//! still picture behind its panels (`app::lobby_picture`: the main menu's
//! art on its blurred cover);
//! on an 800 × 600 canvas (tests and tools) the menus' lobby art
//! (`realmbckg`). The layout is the classic one (plan `lobby-ui.md` L10),
//! spread over the 16:9 width: the title and the account in the middle,
//! the left group (the chat, BACK, LOCK and RESET) at the left edge, the
//! right group (MARKET, the character, the panel and JOIN) at the right
//! edge, each panel on a dark backing in its gold frame. Chat in the large
//! pane on the left with
//! its input line below it; on the right the selected character (name,
//! level and class, which difficulties it unlocked, the guild's tag and the
//! ladder rank), the buttons CREATE / GAMES / LADDER / FRIENDS, and the
//! panel under them: the game list, the create-game form, the ladder or the
//! friends. MARKET sits top right. The bottom slots hold BACK (or CANCEL in
//! the form) and JOIN (CREATE in the form).
//!
//! - **Game list**: name (with `PW` right after it for a password game),
//!   difficulty, players / most players and, with game servers, the server
//!   with its ping (`EU N1 18 ms`; `Host Rhaego ~64 ms` for a player-hosted
//!   game: an estimate, the client's round trip to the realm plus the
//!   host's). Every open game is listed; the ones the character can't join
//!   are grey, and the lines under the list say why (lobby contract
//!   section 7: "full", "hardcore game", "Hell not unlocked", ...). The
//!   first line under the list tells the selected game: its reason, or its
//!   description, level range, server and password. Click selects, a
//!   double click or JOIN joins; Up / Down / wheel move and scroll.
//! - **Join**: a password game opens the password prompt first; a blank
//!   password is refused before anything is sent.
//! - **Create**: name, password (blank = a public game), an optional
//!   description and level range (`40-70`, `40-`, `-70`; the character's
//!   own level must be inside) and difficulty. Only the difficulties the
//!   character unlocked can be picked (the same rule the server applies,
//!   `d2_formats::d2s::status::difficulties`).
//! - **Game server** (a main server with game servers, multiplayer v2):
//!   the form's SERVER row is a drop-down ([`super::server_drop`], owner
//!   2026-10-24): **Automatic (lowest ping)** (the default: the lowest
//!   ping, within 15 ms the less loaded server; `nodelist::pick_auto`)
//!   and each official server by its short region name ("Finland", "US
//!   East") with its ping; a full or closing server is grey and can't be
//!   chosen; the box shows the choice and its ping. **Host on my PC** only
//!   with the hosting switch on ([`super::PLAYER_HOSTING`], off). The chat
//!   stays in its pane.
//! - **Ladder / Friends**: the panels of [`super::ladder_panel`] and
//!   [`super::friends_panel`] (with its guild tab, [`super::guild_panel`])
//!   through [`RightPanel`].
//! - **Market**: MARKET and the market key (Ctrl+I) open the market window
//!   over the lobby ([`super::market_host`], plan L18) once the realm's
//!   market answered; greyed until then. Its tip shows under it while the
//!   mouse is on it: how it opens, or why it can't. While the window is
//!   open it takes the input; Esc and BACK close it, and the lobby is as
//!   it was.
//! - **Chat**: the chat module in the left pane (`chat_pane`): tabs, lines,
//!   the "In chat" column and the input line. Enter sends the typed line
//!   (JOIN's Enter sends chat when a line is typed); a click on a tab opens
//!   it; Page Up / Page Down scroll the lines. A server without chat says so
//!   in the input line.
//! - **Panels**: the chat and the right panel move by their title strips
//!   and resize by their edges ([`super::layout`]); LOCK PANELS and RESET
//!   PANELS sit in the bottom row. The right panel's content is laid out
//!   at its classic place (grown by the panel's extra size) and moved with
//!   the panel.
//!
//! - **Background and HD Text** (16:9): the picker at the top left
//!   chooses the background, the HD Text switch beside it draws every text in
//!   the outline font; both are saved in the settings at once
//!   ([`Screen::settings`]).
//!
//! Every failure shows its text in the message box; the screen stays as it
//! was. JoinOk ends the front end in an online game load.

use std::ops::Range;

use d2_formats::d2s::status;

use crate::rules::camera::FrameSize;
use crate::ui::float::{LayoutKey, Scene};
use crate::ui::front_end::control::vk;
use crate::ui::front_end::flow::Trigger;
use crate::ui::front_end::screens::char_select::class_name;
use crate::ui::front_end::{Action, Control, ControlKind, DrawItem, FrontCtx, Registry, Screen};
use crate::ui::geom::{Point, Rect};
use crate::ui::ui_ids::UiElement;

use super::backdrop::{Backdrop, Picker};
use super::chat_pane::ChatPane;
use super::fake::game_name_ok;
use super::friends_panel::FriendsPanel;
use super::ladder_panel::LadderPanel;
use super::layout::{self, LobbyLayout};
use super::link::{
    difficulty_name, failure_text, lobby_code, ping_text, CharInfo, ErrorCode, Event, Failure,
    GameInfo, LadderAt, LadderQuery, Placement, Request, PLAYER_HOSTED_NOTE,
};
use super::market_host::MarketHost;
use super::server_drop::{self, Drop};
use super::widgets::*;
use super::{OnlineHandle, OnlineState, ONLINE_LOBBY};

/// The lobby art (800 × 600, 4 × 3 tiles).
pub const BG: &str = r"FrontEnd\realmbckg";

/// Custom action ids.
pub mod act {
    pub const BACK: u32 = 1;
    pub const PRIMARY: u32 = 2;
    pub const TAB_CREATE: u32 = 3;
    pub const TAB_JOIN: u32 = 4;
    pub const MESSAGE_OK: u32 = 5;
    pub const CANCEL: u32 = 6;
    pub const PROMPT_OK: u32 = 7;
    pub const PROMPT_CANCEL: u32 = 8;
    pub const SCROLL_UP: u32 = 9;
    pub const SCROLL_DOWN: u32 = 10;
    pub const TAB_LADDER: u32 = 11;
    pub const TAB_FRIENDS: u32 = 12;
    pub const MARKET: u32 = 13;
    pub const LOCK: u32 = 14;
    pub const RESET: u32 = 15;
    /// The background picker's field (opens and closes its list).
    pub const PICKER: u32 = 16;
    pub const HD_TEXT: u32 = 17;
    /// The form's fields: name, password; [`FIELD`] + 2 is the password
    /// prompt's; + 3 description, + 4 level range.
    pub const FIELD: u32 = 20;
    pub const DIFFICULTY: u32 = 30;
    /// The SERVER drop-down's entries: Automatic, Host on my PC, the
    /// official servers (`SERVER + i`); its box (opens and closes the
    /// list) and the click outside the open list (and Esc).
    pub const SERVER_AUTO: u32 = 40;
    pub const SERVER_HOST: u32 = 41;
    pub const SERVER_BOX: u32 = 42;
    pub const SERVER_CLOSE: u32 = 43;
    /// A grey entry (full, closing): its click keeps the list open.
    pub const SERVER_GREY: u32 = 44;
    pub const SERVER: u32 = 50;
    /// The open right panel's own actions: `PANEL..PANEL_END`.
    pub const PANEL: u32 = 100;
    pub const PANEL_END: u32 = 200;
}

/// The right panel (inside the art's right frame): the list, the form, the
/// ladder or the friends.
pub const PANEL: Rect = Rect {
    x: 450,
    y: 316,
    w: 312,
    h: 184,
};
/// The four buttons over the panel: CREATE, GAMES, LADDER, FRIENDS.
const BUTTONS_X: i32 = 446;
const BUTTON_W: u16 = 80;
const BUTTONS_Y: i32 = 305;
/// MARKET, top right (x, bottom y; width, height), and the baseline of
/// its tip under it.
const MARKET: (i32, i32) = (630, 112);
const MARKET_W: u16 = 128;
const MARKET_H: u16 = 35;
const MARKET_TIP_Y: i32 = 126;
/// The tip's right edge (the screen's, less a margin).
const MARKET_TIP_RIGHT: i32 = 794;
/// The group of the floating right panel's controls ([`Control::raised`]);
/// the chat has none.
const RAISED_RIGHT: u8 = 1;
/// The Ctrl key's virtual-key code.
const VK_CONTROL: u16 = 17;
/// The character's lines: x, the first line's bottom, a line's height.
const INFO: (i32, i32, i32) = (450, 200, 18);
/// List rows: a header, then [`ROWS`] rows of [`ROW_H`].
pub const ROWS: usize = 7;
const ROW_H: i32 = 20;
const LIST_TOP: i32 = 338;
/// A row's frame and click area start this far above the row's text box
/// (the text sits at the top of its box).
const ROW_LIFT: i32 = 4;
/// A row's text baseline below its frame's top.
const ROW_BASE: i32 = 16;
/// The list's columns: name (with the `PW` mark), difficulty, players,
/// server and ping; the right edge (left of the scroll bar).
pub const COL_NAME: i32 = 454;
pub const COL_DIFF: i32 = 548;
pub const COL_PLAYERS: i32 = 592;
pub const COL_SERVER: i32 = 620;
pub const LIST_RIGHT: i32 = 746;
/// The right edge without the scroll bar (every game on one page).
pub const LIST_RIGHT_FULL: i32 = 756;
/// The small lines under the list (baselines): the selected game (up to
/// two lines), then the greyed games' reasons; the last is under the
/// panel, above the player-hosted note.
const UNDER_Y: [i32; 3] = [486, 497, 516];
/// The lines of [`UNDER_Y`] inside the frame.
const UNDER_IN_FRAME: usize = 2;
/// A greyed row's shade: black through the lightest blend (its white text
/// dimmed by a quarter, still legible).
const GREY_ROW_MODE: u8 = 0;
/// The room between the list's columns.
const LIST_GUTTER: i32 = 6;
/// The player-hosted note under the panel.
const PH_NOTE_Y: i32 = 536;
/// Official servers in the SERVER drop-down (after Automatic).
pub const SERVER_ROWS: usize = 4;
/// The form: the fields' x, their cel bottoms (name, password,
/// description, levels; a row is 28 px), the SERVER box's (with game
/// servers) and the difficulty label's bottom (lower with the SERVER row).
const FORM_X: i32 = 590;
const FORM_Y: [i32; 4] = [344, 372, 400, 428];
const FORM_SERVER_Y: i32 = 456;
const FORM_DIFF_Y: i32 = 456;
const FORM_DIFF_Y_SERVERS: i32 = 476;
/// Most characters of a game description (lobby contract section 7).
pub const DESCRIPTION_MAX: usize = 40;
/// The password prompt's field.
const PROMPT: (i32, i32) = (315, 300);
/// Double click.
const DOUBLE_MS: u64 = 500;
/// On the 16:9 canvas: how far the middle (the title, the account, the
/// pop-ups and the market window) and the right group (MARKET, the
/// character, the right panel and JOIN) sit right of their 800-column
/// places; the left group stays. Both edges keep the left group's margin.
pub const WIDE_MID: i32 = (FrameSize::WIDE.width - 800) / 2;
pub const WIDE_RIGHT: i32 = FrameSize::WIDE.width - 800 - 14;
/// The character's dark plate (16:9), at its 800-column place.
const INFO_PLATE: Rect = Rect::new(446, 172, 320, 76);
/// The HD Text switch right of the picker, a text box as the picker's
/// field: its left, its cel bottom.
const HD_SWITCH: (i32, i32) = (
    super::backdrop::FIELD.0 + super::backdrop::BOX_W as i32 + 8,
    super::backdrop::FIELD.1,
);

/// Where a new game runs, as the form's SERVER drop-down has it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Choice {
    #[default]
    Auto,
    /// An official server, by id.
    Server(String),
    Host,
}

/// The right panel's views.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum View {
    List,
    Create,
    Ladder,
    Friends,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Phase {
    Idle,
    /// A create or join is out.
    Waiting(String),
    /// The password prompt for joining this game.
    Password(u32),
}

/// What a right panel asks of the lobby.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PanelOut {
    /// Build the screen again.
    Rebuild,
    /// Start a whisper to this character in the chat's input line.
    Whisper(String),
    /// Join this game (a friend's): the lobby's own join, with the password
    /// prompt for a password game.
    Join(u32),
    /// Show this in the message box.
    Message(String),
}

/// A view of the right panel that another module fills: the ladder
/// (`ladder_panel`, plan L11), the friends and the guild (`friends_panel`,
/// `guild_panel`, plan L12). The lobby hands it the panel's rect
/// ([`PANEL`]), every link event, and the input that lands on it while it
/// is open; its own actions are `act::PANEL..act::PANEL_END`.
pub trait RightPanel {
    /// The panel opens (its button), or the lobby is entered afresh with
    /// it open.
    fn open(&mut self) {}
    /// Every event of the lobby's link, in order, open or not.
    fn feed(&mut self, _ev: &Event) {}
    /// Its controls in `area` (labels, buttons, fields).
    fn controls(&mut self, area: Rect, v: &mut Vec<Control>);
    /// Its measured items over the controls (rows cut to fit, frames);
    /// `adv` measures text in a font.
    fn overlay(
        &mut self,
        _area: Rect,
        _adv: &dyn Fn(u16, &[u16]) -> i32,
        _now_ms: u64,
    ) -> Vec<DrawItem> {
        Vec::new()
    }
    /// Text controls it fills each frame (typed fields, carets).
    fn sync(&mut self, _controls: &mut [Control], _now_ms: u64) {}
    /// One of its actions.
    fn action(&mut self, _id: u32) -> Option<PanelOut> {
        None
    }
    /// JOIN (or Enter) while it is open, when [`RightPanel::enter`] did not
    /// take it; `chat_typed`: the chat's input line has text. `None`: the
    /// lobby's own (send the chat line, else join the list's selected game).
    fn primary(&mut self, _chat_typed: bool) -> Option<PanelOut> {
        None
    }
    /// A press at `p` inside the panel.
    fn mouse(&mut self, _p: Point, _now_ms: u64) -> Option<PanelOut> {
        None
    }
    fn wheel(&mut self, _delta: i32) -> Option<PanelOut> {
        None
    }
    /// A typed character; `true` when the panel took it (else it goes to
    /// the chat's input line).
    fn char(&mut self, _c: char) -> bool {
        false
    }
    /// A key; `true` when the panel took it.
    fn key(&mut self, _k: u16) -> bool {
        false
    }
    /// Enter (JOIN's key) while it is open: `Some` when the panel took it
    /// (a typed name box of its own), else it goes to the chat or JOIN.
    fn enter(&mut self) -> Option<PanelOut> {
        None
    }
    /// A press outside the panel: its typed box loses the focus.
    fn blur(&mut self) {}
    /// Its rows and buttons as UI elements (`online_lobby.ladder_row(Kaern)`).
    fn ui_elements(&self) -> Vec<UiElement> {
        Vec::new()
    }
}

/// The lobby's state, open for tests.
pub struct Lobby {
    online: OnlineHandle,
    pub view: View,
    phase: Phase,
    message: Option<String>,
    /// The selected game's id.
    pub selected: Option<u32>,
    /// The first list row shown.
    pub first: usize,
    pub chat: ChatPane,
    pub ladder: LadderPanel,
    pub friends: FriendsPanel,
    /// The market window over the lobby.
    pub market: MarketHost,
    /// The pointer, for the MARKET tip.
    pointer: Point,
    /// The market as last built: it can open, it is shown.
    market_seen: (bool, bool),
    /// The parts of the built controls a floating panel above them covers
    /// (control index, part).
    covers: Vec<(usize, Rect)>,
    /// How many of each field's last characters fit its box, as the last
    /// frame measured them ([`field_fit`]; the fields in `sync`'s order).
    field_fits: [usize; 5],
    game_name: Field,
    game_password: Field,
    join_password: Field,
    description: Field,
    levels: Field,
    /// The form's focus: 0 name, 1 password, 2 description, 3 levels.
    focus: usize,
    pub difficulty: u8,
    /// The form's game server.
    pub place: Choice,
    /// The SERVER drop-down: open, the entry the keys are on.
    pub drop: Drop,
    last_click: Option<(u64, u32)>,
    now_ms: u64,
    rebuild: bool,
    /// Games and chat as last built (a change rebuilds).
    seen: (usize, usize),
    listed: bool,
    /// Text controls of the fields in the built list: game name, game
    /// password, join password, description, level range.
    text_ix: [Option<usize>; 5],
    /// Where the chat and the right panel are.
    pub layout: LobbyLayout,
    /// Laid out on the 16:9 canvas ([`Screen::wide`]): the picture behind
    /// it, no lobby art.
    pub wide: bool,
    /// The background picker (16:9).
    pub picker: Picker,
    /// HD Text and the background as the settings have them, and the
    /// player's change not yet handed to them.
    shown: (bool, Backdrop),
    edit: Option<(bool, Backdrop)>,
    /// The settings changed what the lobby shows: it builds again.
    restyled: bool,
}

/// The form's field of each focus (0-3) as `act::FIELD` offsets.
const FORM_FIELDS: [u32; 4] = [0, 1, 3, 4];

impl Lobby {
    pub fn new(online: OnlineHandle) -> Self {
        let chat = ChatPane::new(online.clone());
        Lobby::with_chat(online, chat)
    }

    /// The lobby with its own chat (tests and captures).
    pub fn with_chat(online: OnlineHandle, chat: ChatPane) -> Self {
        Lobby {
            ladder: LadderPanel::new(online.clone()),
            friends: FriendsPanel::new(online.clone()),
            market: MarketHost::new(),
            pointer: Point::new(0, 0),
            market_seen: (false, false),
            covers: Vec::new(),
            field_fits: [usize::MAX; 5],
            online,
            view: View::List,
            phase: Phase::Idle,
            message: None,
            selected: None,
            first: 0,
            chat,
            game_name: Field::new(15),
            game_password: Field::new(15),
            join_password: Field::password(15),
            description: Field::new(DESCRIPTION_MAX),
            levels: Field::new(5),
            focus: 0,
            difficulty: 0,
            place: Choice::Auto,
            drop: Drop::default(),
            last_click: None,
            now_ms: 0,
            rebuild: false,
            seen: (0, 0),
            listed: false,
            text_ix: [None; 5],
            layout: LobbyLayout::new(),
            wide: false,
            picker: Picker::default(),
            shown: (false, Backdrop::default()),
            edit: None,
            restyled: false,
        }
    }

    /// How far the middle and the right group sit right of their
    /// 800-column places ([`WIDE_MID`], [`WIDE_RIGHT`]; none at 4:3).
    pub fn shifts(&self) -> (i32, i32) {
        if self.wide {
            (WIDE_MID, WIDE_RIGHT)
        } else {
            (0, 0)
        }
    }

    /// HD Text and the background the lobby shows (the settings', or the
    /// player's change on its way to them).
    pub fn styled(&self) -> (bool, Backdrop) {
        self.edit.unwrap_or(self.shown)
    }

    /// MARKET's rectangle where it is drawn.
    pub fn market_at(&self) -> Rect {
        let mut r = Self::market_rect();
        r.x += self.shifts().1;
        r
    }

    fn again(&mut self) -> Option<Trigger> {
        self.rebuild = true;
        Some(Trigger::Rebuild)
    }

    /// The list's rows: [`ROWS`], more in a taller right panel.
    fn rows(&self) -> usize {
        ROWS + (self.layout.right_grow().1.max(0) / ROW_H) as usize
    }

    /// The right panel's area at its classic place, grown with the panel.
    fn area(&self) -> Rect {
        let (dw, dh) = self.layout.right_grow();
        Rect::new(
            PANEL.x,
            PANEL.y,
            (i32::from(PANEL.w) + dw) as u16,
            (i32::from(PANEL.h) + dh) as u16,
        )
    }

    /// The config folder of the panel layouts, and the realm and account
    /// the lobby's is saved under.
    fn layout_key(&self) -> (Option<std::path::PathBuf>, Option<LayoutKey>) {
        let o = self.online.borrow();
        let key = o.state.account.as_deref().and_then(|a| {
            let realm = o.state.servers.get(o.state.server)?;
            Some(LayoutKey::new(&realm.id, a, Scene::Lobby))
        });
        (o.layouts_dir.clone(), key)
    }

    /// A layout file error, told in the chat.
    fn layout_note(&mut self, note: Option<String>) {
        if let Some(n) = note {
            self.chat.model.note(n);
        }
    }

    fn modal_up(&self) -> bool {
        self.message.is_some() || self.phase != Phase::Idle
    }

    fn character(&self) -> Option<CharInfo> {
        self.online.borrow().state.selected.clone()
    }

    /// The games in list order (the server's order: oldest first).
    fn games(&self) -> Vec<GameInfo> {
        self.online.borrow().state.games.clone()
    }

    fn selected_game(&self) -> Option<GameInfo> {
        let id = self.selected?;
        self.games().into_iter().find(|g| g.id == id)
    }

    fn mask(&self) -> u8 {
        self.character()
            .map_or(status::NORMAL_BIT, |c| c.difficulties)
    }

    /// The open panel of another module.
    fn panel(&mut self) -> Option<&mut dyn RightPanel> {
        match self.view {
            View::Ladder => Some(&mut self.ladder),
            View::Friends => Some(&mut self.friends),
            View::List | View::Create => None,
        }
    }

    fn fail(&mut self, code: ErrorCode) -> Option<Trigger> {
        self.message = Some(failure_text(&Failure {
            code,
            wait_s: 0,
            text: String::new(),
        }));
        self.again()
    }

    /// JOIN on the selected game: the client's own checks first (the
    /// server's rules, so the player hears the reason at once), then the
    /// password prompt for a password game, then the request.
    fn join(&mut self) -> Option<Trigger> {
        let Some(g) = self.selected_game() else {
            self.message = Some("Choose a game in the list first.".into());
            return self.again();
        };
        if g.joinable != 0 {
            return self.fail(ErrorCode(g.joinable));
        }
        if g.locked {
            self.join_password.text.clear();
            self.phase = Phase::Password(g.id);
            return self.again();
        }
        self.send_join(g.id, String::new())
    }

    fn send_join(&mut self, game_id: u32, password: String) -> Option<Trigger> {
        let name = self
            .games()
            .into_iter()
            .find(|g| g.id == game_id)
            .map(|g| g.name)
            .unwrap_or_default();
        self.online
            .borrow_mut()
            .send(Request::JoinGame { game_id, password });
        self.phase = Phase::Waiting(format!("Joining {name}..."));
        self.again()
    }

    /// CREATE in the form.
    fn create(&mut self) -> Option<Trigger> {
        let name = self.game_name.text.trim().to_string();
        if name.is_empty() {
            self.message = Some("Type a name for the game.".into());
            return self.again();
        }
        if !game_name_ok(&name) {
            return self.fail(ErrorCode::BAD_GAME_NAME);
        }
        if !status::difficulty_open(self.mask(), self.difficulty) {
            return self.fail(ErrorCode::DIFFICULTY_LOCKED);
        }
        let level = self.character().map_or(1, |c| c.level);
        let (level_min, level_max) = match parse_levels(&self.levels.text, level) {
            Ok(r) => r,
            Err(text) => {
                self.message = Some(text);
                return self.again();
            }
        };
        let (place, wait) = self.placement(&name);
        self.online.borrow_mut().send(Request::CreateGame {
            name,
            password: self.game_password.text.clone(),
            difficulty: self.difficulty,
            place,
            description: self.description.text.trim().to_string(),
            level_min,
            level_max,
        });
        self.phase = Phase::Waiting(wait);
        self.again()
    }

    /// Where the form's game runs, and the wait box's text.
    fn placement(&self, name: &str) -> (Placement, String) {
        let st = &self.online.borrow().state;
        let on = |server: &str| format!("Creating {name} on {server}...");
        if st.nodes.is_empty() {
            return (Placement::Automatic, format!("Creating {name}..."));
        }
        match &self.place {
            Choice::Auto => match st.auto_node() {
                Some(n) => (Placement::AutomaticPick(n.node.clone()), on(&n.name)),
                None => (Placement::Automatic, format!("Creating {name}...")),
            },
            Choice::Server(id) => {
                let shown = st
                    .nodes
                    .iter()
                    .find(|n| n.node == *id)
                    .map_or(id.as_str(), |n| n.name.as_str());
                (Placement::Official(id.clone()), on(shown))
            }
            Choice::Host => (
                Placement::HostOnMyPc,
                format!("Starting {name} on your PC..."),
            ),
        }
    }

    /// A choice the list no longer has goes back to Automatic.
    fn keep_place(&mut self) {
        if let Choice::Server(id) = &self.place {
            let st = &self.online.borrow().state;
            if !st.nodes.iter().any(|n| n.node == *id && n.open()) {
                self.place = Choice::Auto;
            }
        }
    }

    fn send_chat(&mut self) -> Option<Trigger> {
        self.chat.submit();
        None
    }

    /// Typed characters go to the chat's input line (the form's fields
    /// take them while it is open).
    fn chat_typing(&self) -> bool {
        self.view != View::Create
    }

    /// The form has the SERVER row: the main server lists game servers.
    fn has_servers(&self) -> bool {
        !self.online.borrow().state.nodes.is_empty()
    }

    /// The SERVER box at the form's classic place.
    fn server_box(&self) -> Rect {
        server_drop::box_rect(FORM_X, FORM_SERVER_Y)
    }

    /// The SERVER box where it is drawn (with the right panel).
    fn server_box_now(&self) -> Rect {
        let b = self.server_box();
        let (dx, dy) = self.layout.right_offset();
        Rect::new(b.x + dx, b.y + dy, b.w, b.h)
    }

    /// The drop-down's entries now.
    fn drop_entries(&self) -> Vec<server_drop::Entry> {
        server_drop::entries(&self.online.borrow().state)
    }

    /// The drop-down's entry `action` chosen (when it can be): the list
    /// closes.
    fn choose(&mut self, action: u32) -> Option<Trigger> {
        if let Some(e) = self
            .drop_entries()
            .into_iter()
            .find(|e| e.action == action && e.enabled)
        {
            self.place = e.choice;
        }
        self.drop.close();
        self.again()
    }

    /// Asks the realm for the character's rank in its own ladder (its
    /// season, hardcore or not, expansion or classic, all classes): the
    /// ladder panel asks, as it tells its answers from its pages'.
    fn ask_rank(&mut self) {
        // A non-ladder character has none.
        if let Some(c) = self.character().filter(|c| c.season != 0) {
            self.ladder.ask_rank(&c);
        }
    }

    /// Move the selection by `d` rows (keys), keeping it in view.
    fn step(&mut self, d: i32) -> Option<Trigger> {
        let games = self.games();
        if games.is_empty() {
            return None;
        }
        let at = self
            .selected
            .and_then(|id| games.iter().position(|g| g.id == id))
            .map_or(0, |i| {
                (i as i32 + d).clamp(0, games.len() as i32 - 1) as usize
            });
        self.selected = Some(games[at].id);
        let rows = self.rows();
        if at < self.first {
            self.first = at;
        } else if at >= self.first + rows {
            self.first = at + 1 - rows;
        }
        self.again()
    }

    fn scroll(&mut self, d: i32) -> Option<Trigger> {
        let n = self.games().len();
        let max = n.saturating_sub(self.rows());
        self.first = (self.first as i32 + d).clamp(0, max as i32) as usize;
        self.again()
    }

    /// The top of list row `r` (its frame and click area).
    fn row_top(r: usize) -> i32 {
        LIST_TOP - ROW_LIFT + ROW_H * r as i32
    }

    /// The list row under `p`.
    fn row_at(&self, p: Point) -> Option<usize> {
        let top = Self::row_top(0);
        let area = self.area();
        if p.x < area.x || p.x >= area.x + area.w as i32 - 14 || p.y < top {
            return None;
        }
        let r = ((p.y - top) / ROW_H) as usize;
        (r < self.rows()).then_some(self.first + r)
    }

    fn info_lines(&self) -> Vec<(String, i32)> {
        let Some(c) = self.character() else {
            return Vec::new();
        };
        let mut kind = String::new();
        if c.status & status::HARDCORE != 0 {
            kind.push_str("Hardcore ");
        }
        if c.status & status::EXPANSION != 0 {
            kind.push_str("Expansion ");
        }
        let mut lines = vec![
            (c.name.clone(), GOLD),
            (
                format!(
                    "Level {} {} {}",
                    c.level,
                    class_name(c.class),
                    kind.trim_end()
                )
                .trim_end()
                .to_string(),
                WHITE,
            ),
        ];
        let open: Vec<&str> = (0..3)
            .filter(|&d| status::difficulty_open(c.difficulties, d))
            .map(difficulty_name)
            .collect();
        lines.push((format!("Difficulties: {}", open.join(", ")), WHITE));
        let tag = self
            .chat
            .model
            .state()
            .and_then(|s| s.guild.as_ref())
            .map(|g| g.tag.clone());
        if let Some(line) = standing_line(tag.as_deref(), self.ladder.rank, c.season) {
            lines.push((line, GREEN));
        }
        lines
    }

    /// The four buttons over the panel; the open view's is off.
    fn button_controls(&self, v: &mut Vec<Control>) {
        let buttons = [
            ("CREATE", act::TAB_CREATE, View::Create),
            ("GAMES", act::TAB_JOIN, View::List),
            ("LADDER", act::TAB_LADDER, View::Ladder),
            ("FRIENDS", act::TAB_FRIENDS, View::Friends),
        ];
        for (i, (text, id, view)) in buttons.into_iter().enumerate() {
            let x = BUTTONS_X + i32::from(BUTTON_W) * i as i32;
            let mut b = narrow_button(x, BUTTONS_Y, BUTTON_W, text, id);
            // The open view's button: down, its label gold.
            b.enabled = self.view != view;
            if !b.enabled {
                b.color = GOLD;
            }
            v.push(b);
        }
    }

    fn list_controls(&self, v: &mut Vec<Control>) {
        let games = self.games();
        let (rows, area) = (self.rows(), self.area());
        let (dw, dh) = self.layout.right_grow();
        if games.is_empty() {
            let text = if self.listed {
                "No open games. Create one!"
            } else {
                "Getting the game list..."
            };
            v.push(centred(area.x, LIST_TOP + 60, area.w, 20, FONT16, text));
        }
        if games.len() > rows {
            let cel = |frame: u32, top: i32, id: Option<u32>| {
                let mut c =
                    Control::new(ControlKind::Image, 748 + dw, top + 14, 12, 14).with_art(SCROLL);
                c.cel = Some(frame);
                if let Some(id) = id {
                    c.action = Action::Custom(id);
                }
                c
            };
            let range = (games.len() - rows) as i32;
            v.push(cel(0, LIST_TOP, Some(act::SCROLL_UP)));
            v.push(cel(
                1,
                LIST_TOP + ROW_H * rows as i32 - 14,
                Some(act::SCROLL_DOWN),
            ));
            let track = ROW_H * rows as i32 - 14 * 3;
            v.push(cel(
                4,
                LIST_TOP + 14 + track * self.first as i32 / range,
                None,
            ));
        }
        let st = self.online.borrow();
        if !st.state.nodes.is_empty() && games.iter().any(|g| g.player_hosted) {
            v.push(small(
                area.x,
                PH_NOTE_Y + dh,
                area.w,
                format!("Host: {PLAYER_HOSTED_NOTE}"),
                GOLD,
            ));
        }
    }

    /// The list's header, rows and the lines under it, measured with
    /// `adv` so every cell stays in its column.
    fn list_overlay(&self, adv: &dyn Fn(u16, &[u16]) -> i32) -> Vec<DrawItem> {
        let width = |font: u16| move |s: &str| adv(font, &s.encode_utf16().collect::<Vec<u16>>());
        let w16 = width(FONT16);
        let w8 = width(FONT8);
        let st = self.online.borrow().state.clone();
        let games = self.games();
        let servers = list_has_servers(&st, &games);
        let (rows, area) = (self.rows(), self.area());
        // A wider panel widens the name column.
        let (dw, dh) = self.layout.right_grow();
        // Each column a gutter past the widest cell before it.
        let col_diff = COL_DIFF + dw;
        let widest = |head: &str, cell: &dyn Fn(&GameInfo) -> String| {
            games
                .iter()
                .map(|g| w16(&cell(g)))
                .fold(w16(head), i32::max)
        };
        let diff_w = widest("DIFF", &|g| difficulty_short(g.difficulty).to_string());
        let col_players = (COL_PLAYERS + dw).max(col_diff + diff_w + LIST_GUTTER);
        let players_w = widest("PL", &|g| format!("{}/{}", g.players, g.max_players));
        let col_server = (COL_SERVER + dw).max(col_players + players_w + LIST_GUTTER);
        let right = if games.len() > rows {
            LIST_RIGHT
        } else {
            LIST_RIGHT_FULL
        } + dw;
        let mut out = Vec::new();
        let head = LIST_TOP - 6;
        let mut heads = vec![(COL_NAME, "NAME"), (col_diff, "DIFF"), (col_players, "PL")];
        if servers {
            heads.push((col_server, "SERVER"));
        }
        for (x, h) in heads {
            out.push(text_item(FONT16, x, head, GOLD, h));
        }
        let me = self.character();
        for (r, g) in games.iter().skip(self.first).take(rows).enumerate() {
            let base = Self::row_top(r) + ROW_BASE;
            // A greyed row: its cells in the list's colours, shaded below.
            let colour = WHITE;
            let pw = if g.locked { " PW" } else { "" };
            let pw_w = w8(pw);
            let name_w = col_diff - COL_NAME - 6;
            let name = fit(&g.name, name_w - pw_w, &w16);
            out.push(text_item(FONT16, COL_NAME, base, colour, &name));
            if g.locked {
                let x = COL_NAME + w16(&name);
                out.push(text_item(FONT8, x, base, GOLD, pw));
            }
            out.push(text_item(
                FONT16,
                col_diff,
                base,
                colour,
                difficulty_short(g.difficulty),
            ));
            out.push(text_item(
                FONT16,
                col_players,
                base,
                colour,
                &format!("{}/{}", g.players, g.max_players),
            ));
            if servers {
                let cell = server_cell(&st, g, right - col_server, &w8);
                let mut x = col_server;
                if g.player_hosted {
                    out.push(text_item(FONT8, x, base, GOLD, HOST_MARK));
                    x += w8(HOST_MARK) + w8(" ");
                }
                out.push(text_item(FONT8, x, base, colour, &cell));
            }
            if g.joinable != 0 {
                // A light shade over the row, inside its frame's edge.
                out.push(DrawItem::Shade {
                    at: Point::new(area.x + 1, Self::row_top(r) + 1),
                    w: right - area.x - 1,
                    h: ROW_H - 2,
                    color: 0,
                    mode: GREY_ROW_MODE,
                });
            }
        }
        // Under the list: the selected game (two lines at most), then why
        // the other grey ones are, each wrapped and cut on its last line;
        // the legend stays on one side of the frame's edge (two lines in
        // the frame, else one).
        let room = area.w as i32 - 8;
        let mut lines: Vec<(String, i32)> = Vec::new();
        let selected = self.selected_game();
        if let Some(g) = &selected {
            let (text, colour) = selected_line(&st, g, me.as_ref());
            push_wrapped(&mut lines, &text, colour, 2, room, &w8);
        }
        let others: Vec<GameInfo> = games
            .iter()
            .filter(|g| {
                selected
                    .as_ref()
                    .is_none_or(|s| s.id != g.id || g.joinable == 0)
            })
            .cloned()
            .collect();
        if let Some(legend) = grey_legend(&others, me.as_ref()) {
            let free = if lines.is_empty() { UNDER_IN_FRAME } else { 1 };
            push_wrapped(&mut lines, &legend, GREY, free, room, &w8);
        }
        for (y, (text, colour)) in UNDER_Y.iter().zip(lines) {
            out.push(text_item(FONT8, area.x + 4, *y + dh, colour, &text));
        }
        out
    }

    fn form_controls(&mut self, v: &mut Vec<Control>) {
        let rows = [
            ("GAME NAME", None),
            ("PASSWORD", Some("Blank: public game.")),
            ("DESCRIPTION", Some("Optional.")),
            ("LEVELS", Some("Optional, e.g. 40-70.")),
        ];
        for (i, (title, hint)) in rows.into_iter().enumerate() {
            let y = FORM_Y[i];
            v.push(label(PANEL.x + 4, y - 4, 128, title, GOLD));
            if let Some(h) = hint {
                // Under the title, left of the box.
                v.push(small(
                    PANEL.x + 4,
                    y + 8,
                    (FORM_X - PANEL.x - 8) as u16,
                    h,
                    GREY,
                ));
            }
            let field = FORM_FIELDS[i];
            v.extend(field_controls(FORM_X, y, act::FIELD + field));
            self.text_ix[field as usize] = Some(v.len() - 1);
        }
        let diff_y = if self.has_servers() {
            let y = FORM_SERVER_Y;
            v.push(label(PANEL.x + 4, y - 4, 128, "SERVER", GOLD));
            server_drop::box_controls(self.server_box(), v);
            FORM_DIFF_Y_SERVERS
        } else {
            FORM_DIFF_Y
        };
        v.push(label(PANEL.x + 4, diff_y, 130, "DIFFICULTY", GOLD));
        let mask = self.mask();
        for d in 0..3u8 {
            let x = PANEL.x + 4 + 100 * i32::from(d);
            let y = diff_y + 22;
            let open = status::difficulty_open(mask, d);
            let mut r = Control::new(ControlKind::Button, x, y, 16, 16)
                .with_art(RADIO)
                .with_action(Action::Custom(act::DIFFICULTY + u32::from(d)))
                .with_on_press();
            r.checked = self.difficulty == d;
            r.enabled = open;
            v.push(r);
            v.push(label(
                x + 20,
                y + 1,
                80,
                difficulty_name(d),
                if open { WHITE } else { GREY },
            ));
        }
        if mask != 0b111 {
            v.push(small(
                PANEL.x + 4,
                diff_y + 40,
                300,
                "Grey: not unlocked by this character yet.",
                GREY,
            ));
        }
    }

    /// The form's field with focus `f` (0-3).
    fn form_field(&mut self, f: usize) -> &mut Field {
        match f {
            0 => &mut self.game_name,
            1 => &mut self.game_password,
            2 => &mut self.description,
            _ => &mut self.levels,
        }
    }

    /// The right panel's controls (`right`: their range in `v`; the chat
    /// has none) drawn with their panel where it floats
    /// ([`LobbyLayout::floating`]); where a floating panel above a control
    /// covers it, a click doesn't reach the control ([`Screen::covered`]),
    /// and a control it covers whole takes no input.
    fn stack(&mut self, v: &mut [Control], right: Range<usize>) {
        self.covers.clear();
        let floating = self.layout.floating();
        // Each control's panel (`None`: the lobby's own).
        let panel_of = |i: usize| right.contains(&i).then_some(layout::RIGHT_ID);
        let order = self.layout.order();
        let level = |id: Option<&str>| id.and_then(|id| order.iter().position(|o| *o == id));
        for (i, c) in v.iter_mut().enumerate() {
            let own = panel_of(i);
            if own.is_some_and(|id| floating.contains(&id)) {
                c.raised = RAISED_RIGHT;
            }
            if i == 0 {
                continue;
            }
            let hit = c.hit_box();
            for f in &floating {
                if Some(*f) == own || level(Some(f)) <= level(own) {
                    continue;
                }
                if let Some(part) = layout::clip(hit, self.layout.rect(f)) {
                    c.inert |= part == hit;
                    self.covers.push((i, part));
                }
            }
        }
    }

    /// MARKET's tip shows while the pointer is on the button where no
    /// panel covers it.
    fn market_tip_shown(&self) -> bool {
        self.market_at().contains(self.pointer) && self.layout.top_at(self.pointer).is_none()
    }

    /// The 16:9 lobby's own switches: the background picker and HD Text.
    fn style_controls(&self, v: &mut Vec<Control>) {
        let (hd, chosen) = self.styled();
        self.picker.controls(v, chosen, act::PICKER);
        let (x, y) = HD_SWITCH;
        v.push(Control::new(ControlKind::Image, x, y, FIELD_W, FIELD_H).with_art(TEXTBOX));
        let mut t = field_text(x, y);
        t.text = Some(format!("HD Text: {}", if hd { "On" } else { "Off" }));
        t.color = GOLD;
        t.flags |= 2;
        v.push(t);
        v.push(
            Control::new(ControlKind::Image, x, y, FIELD_W, FIELD_H)
                .with_action(Action::Custom(act::HD_TEXT)),
        );
    }

    /// MARKET's rectangle (at its 800-column place, [`Self::market_at`]).
    pub fn market_rect() -> Rect {
        Rect::new(MARKET.0, MARKET.1 - i32::from(MARKET_H), MARKET_W, MARKET_H)
    }

    /// MARKET or the market key: the window opens or closes.
    fn toggle_market(&mut self) -> Option<Trigger> {
        self.market.toggle();
        self.again()
    }

    /// The market's lines (its notices) in the chat.
    fn market_lines(&mut self, lines: Vec<String>) {
        let at_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_millis() as u64);
        for text in lines {
            self.chat
                .model
                .apply(crate::chat::ChatIn::MarketNotice { text, at_ms });
        }
    }

    /// The panel's answer, applied.
    fn panel_out(&mut self, out: Option<PanelOut>) -> Option<Trigger> {
        match out? {
            PanelOut::Rebuild => self.again(),
            PanelOut::Whisper(name) => {
                self.chat.model.whisper_to(&name);
                self.again()
            }
            PanelOut::Join(id) => {
                if self.games().iter().any(|g| g.id == id) {
                    self.selected = Some(id);
                    self.join()
                } else {
                    self.send_join(id, String::new())
                }
            }
            PanelOut::Message(m) => {
                self.message = Some(m);
                self.again()
            }
        }
    }
}

impl Screen for Lobby {
    fn build(&mut self, ctx: &mut FrontCtx) -> Vec<Control> {
        if !std::mem::take(&mut self.rebuild) {
            // A fresh entry (from the character list, or back from a game).
            self.view = View::List;
            self.phase = Phase::Idle;
            self.selected = None;
            self.first = 0;
            let me = self.character().map(|c| c.name);
            self.chat.enter(me.as_deref());
            self.listed = false;
            self.place = Choice::Auto;
            self.drop.close();
            self.difficulty = (0..3)
                .rev()
                .find(|&d| status::difficulty_open(self.mask(), d))
                .unwrap_or(0);
            {
                let mut o = self.online.borrow_mut();
                self.message = o.state.notice.take();
                o.state.games.clear();
                o.send(Request::ListGames);
            }
            self.ask_rank();
            let (dir, key) = self.layout_key();
            let note = self.layout.enter(dir, key);
            self.layout_note(note);
            self.market.start(&self.online);
        }
        self.chat.set_layout(self.layout.chat_layout());
        let st = self.online.borrow().state.clone();
        self.seen = (st.games.len(), st.chat.len());
        self.text_ix = [None; 5];
        self.now_ms = ctx.now_ms;
        let (mid, right) = self.shifts();
        // The first control is the background: the lobby art, or on the
        // 16:9 canvas nothing (the picture shows): the panels' backings go
        // over it ([`Screen::underlay`]).
        let bg = Control::new(ControlKind::Image, 0, 599, 800, 600);
        let mut v = vec![
            if self.wide { bg } else { bg.with_art(BG) },
            centred(
                mid,
                70,
                800,
                40,
                FONT30,
                if st.servers.len() == 1 {
                    crate::launch::NAME.to_string()
                } else {
                    st.server_name.clone()
                },
            ),
            centred(
                mid,
                98,
                800,
                24,
                FONT16,
                format!("Account {}", st.account.clone().unwrap_or_default()),
            ),
        ];
        // MARKET: greyed until the realm's market answered (its tip says
        // why, on hover).
        let mut market = button(
            MARKET.0 + right,
            MARKET.1,
            MARKET_W,
            MARKET_H,
            MEDIUM,
            "MARKET",
            act::MARKET,
        );
        market.enabled = self.market.can_open();
        v.push(market);
        self.market_seen = (self.market.can_open(), self.market.shown());
        let (x, top, step) = INFO;
        for (i, (text, colour)) in self.info_lines().into_iter().enumerate() {
            v.push(label(x + right, top + step * i as i32, 316, text, colour));
        }
        // The right panel's controls at its classic place, then moved
        // with it.
        let right_from = v.len();
        self.button_controls(&mut v);
        let area = self.area();
        match self.view {
            View::List => self.list_controls(&mut v),
            View::Create => self.form_controls(&mut v),
            View::Ladder | View::Friends => {
                if let Some(p) = self.panel() {
                    p.controls(area, &mut v);
                }
            }
        }
        layout::shift_controls(&mut v[right_from..], self.layout.right_offset());
        let right_end = v.len();
        let (back, primary) = match self.view {
            View::Create => ("CANCEL", "CREATE"),
            _ => ("BACK", "JOIN"),
        };
        let back = button(38, 570, 128, 35, MEDIUM, back, act::BACK);
        if self.market.shown() {
            // The market window has the input; BACK closes it (Esc is the
            // window's: a box first).
            for c in &mut v {
                c.inert = true;
            }
            self.covers.clear();
            v.push(back);
            return v;
        }
        v.push(back.with_hotkey(vk::ESC));
        v.push(
            button(635 + right, 570, 128, 35, MEDIUM, primary, act::PRIMARY).with_hotkey(vk::ENTER),
        );
        let lock = if self.layout.locked() {
            "UNLOCK PANELS"
        } else {
            "LOCK PANELS"
        };
        v.push(narrow_button(176, 570, MEDIUM_W, lock, act::LOCK));
        let mut reset = narrow_button(314, 570, MEDIUM_W, "RESET PANELS", act::RESET);
        reset.enabled = !self.layout.is_classic();
        v.push(reset);
        if self.wide {
            self.style_controls(&mut v);
        }
        self.stack(&mut v, right_from..right_end);
        let popup = v.len();
        if let Some(m) = &self.message {
            push_message(&mut v, SKY_POPUP, m, act::MESSAGE_OK);
        } else {
            match &self.phase {
                Phase::Idle => {}
                Phase::Waiting(text) => push_wait(&mut v, SKY_POPUP, text, act::CANCEL),
                Phase::Password(_) => {
                    push_popup_art(&mut v, SKY_POPUP);
                    let mut t = popup_text("This game needs a password.");
                    t.h = 50;
                    t.y = 270;
                    v.push(t);
                    v.extend(field_controls(PROMPT.0, PROMPT.1, act::FIELD + 2));
                    self.text_ix[2] = Some(v.len() - 1);
                    v.push(
                        button(
                            281,
                            337,
                            96,
                            32,
                            SKY_POPUP.button,
                            "CANCEL",
                            act::PROMPT_CANCEL,
                        )
                        .with_hotkey(vk::ESC)
                        .with_on_press(),
                    );
                    v.push(
                        button(421, 337, 96, 32, SKY_POPUP.button, "JOIN", act::PROMPT_OK)
                            .with_hotkey(vk::ENTER)
                            .with_on_press(),
                    );
                }
            }
        }
        layout::shift_controls(&mut v[popup..], (mid, 0));
        modal(&mut v);
        // The open SERVER list has the input: the rest of the screen is
        // drawn as it is.
        let free = self.message.is_none() && self.phase == Phase::Idle;
        if !(free && self.view == View::Create && !st.nodes.is_empty()) {
            self.drop.close();
        }
        if self.drop.open {
            for c in &mut v {
                c.inert = true;
            }
            let from = v.len();
            server_drop::list_controls(self.server_box(), &self.drop_entries(), &mut v);
            // The click outside stays where it is; the entries move with
            // the panel.
            layout::shift_controls(&mut v[from + 1..], self.layout.right_offset());
        }
        if self.picker.open {
            // The list has the input until a name is picked or it closes.
            for c in &mut v {
                c.inert |= c.action != Action::Custom(act::PICKER);
            }
        }
        self.layout.set_hot(&v);
        v
    }

    fn action(&mut self, _ctx: &mut FrontCtx, id: u32) -> Option<Trigger> {
        match id {
            act::MESSAGE_OK => {
                self.message = None;
                self.again()
            }
            act::CANCEL => {
                // The answer may still come: JoinOk then still starts the
                // game (the server holds the seat); an error is shown.
                self.phase = Phase::Idle;
                self.again()
            }
            act::PROMPT_CANCEL => {
                self.phase = Phase::Idle;
                self.again()
            }
            act::PROMPT_OK => {
                let Phase::Password(game) = self.phase else {
                    return None;
                };
                if self.join_password.text.is_empty() {
                    self.phase = Phase::Idle;
                    return self.fail(ErrorCode::PASSWORD_NEEDED);
                }
                let pw = std::mem::take(&mut self.join_password.text);
                self.send_join(game, pw)
            }
            act::BACK if self.market.shown() => {
                self.market.close();
                self.again()
            }
            act::MARKET => self.toggle_market(),
            act::PICKER => {
                self.picker.toggle();
                self.again()
            }
            act::HD_TEXT => {
                let (hd, b) = self.styled();
                self.edit = Some((!hd, b));
                self.again()
            }
            act::BACK => match self.view {
                View::List => {
                    self.online.borrow_mut().send(Request::LeaveLobby);
                    Some(Trigger::Exit)
                }
                _ => {
                    self.view = View::List;
                    self.again()
                }
            },
            act::PRIMARY => {
                // A typed box of the open panel takes Enter first.
                if let Some(out) = self.panel().and_then(|p| p.enter()) {
                    return self.panel_out(Some(out));
                }
                let typed = self.chat.has_input();
                if let Some(out) = self.panel().and_then(|p| p.primary(typed)) {
                    return self.panel_out(Some(out));
                }
                match self.view {
                    View::Create => self.create(),
                    _ if typed => self.send_chat(),
                    _ => self.join(),
                }
            }
            act::TAB_CREATE => {
                self.view = View::Create;
                self.focus = 0;
                self.drop.close();
                self.again()
            }
            act::TAB_JOIN => {
                self.view = View::List;
                self.again()
            }
            act::TAB_LADDER | act::TAB_FRIENDS => {
                self.view = if id == act::TAB_LADDER {
                    View::Ladder
                } else {
                    View::Friends
                };
                if let Some(p) = self.panel() {
                    p.open();
                }
                self.again()
            }
            act::SCROLL_UP => self.scroll(-1),
            act::SCROLL_DOWN => self.scroll(1),
            act::LOCK => {
                let note = self.layout.toggle_lock();
                self.layout_note(note);
                self.again()
            }
            act::RESET => {
                let note = self.layout.reset();
                self.layout_note(note);
                self.again()
            }
            f if f == act::FIELD + 2 => None,
            f if (act::FIELD..act::FIELD + 5).contains(&f) => {
                let off = f - act::FIELD;
                self.focus = FORM_FIELDS.iter().position(|&o| o == off).unwrap_or(0);
                None
            }
            act::SERVER_BOX => {
                if self.drop.open {
                    self.drop.close();
                } else {
                    let entries = self.drop_entries();
                    self.drop.open(&entries, &self.place);
                }
                self.again()
            }
            act::SERVER_GREY => None,
            act::SERVER_CLOSE => {
                self.drop.close();
                self.again()
            }
            act::SERVER_AUTO | act::SERVER_HOST => self.choose(id),
            s if (act::SERVER..act::SERVER + SERVER_ROWS as u32).contains(&s) => self.choose(s),
            d if (act::DIFFICULTY..act::DIFFICULTY + 3).contains(&d) => {
                let d = (d - act::DIFFICULTY) as u8;
                if status::difficulty_open(self.mask(), d) {
                    self.difficulty = d;
                }
                self.again()
            }
            p if (act::PANEL..act::PANEL_END).contains(&p) => {
                let out = self.panel().and_then(|panel| panel.action(p));
                self.panel_out(out)
            }
            _ => None,
        }
    }

    fn tick(&mut self, ctx: &mut FrontCtx) -> Option<Trigger> {
        self.now_ms = ctx.now_ms;
        let events = self.online.borrow_mut().take_events();
        let mut out = None;
        for ev in events {
            self.chat.feed(&ev);
            self.ladder.feed(&ev);
            self.friends.feed(&ev);
            match ev {
                Event::GameListEnd => self.listed = true,
                Event::GameGone(id) if self.selected == Some(id) => self.selected = None,
                Event::Nodes(_) => {
                    self.keep_place();
                    if self.drop.hot >= self.drop_entries().len() {
                        self.drop.hot = 0;
                    }
                }
                Event::JoinOk(_) => {
                    self.phase = Phase::Idle;
                    self.market.close();
                    self.online.borrow_mut().send(Request::LeaveLobby);
                    return Some(Trigger::Ok);
                }
                Event::Error(f) if f.code == ErrorCode::NOT_SUPPORTED => {}
                Event::Error(f) => {
                    self.phase = Phase::Idle;
                    self.message = Some(failure_text(&f));
                }
                Event::Disconnected(_) => return Some(Trigger::OnlineLost),
                _ => {}
            }
            out = Some(());
        }
        self.chat.pump(ctx.now_ms);
        let lines = self.market.tick();
        self.market_lines(lines);
        if (self.market.can_open(), self.market.shown()) != self.market_seen {
            out = Some(());
        }
        let now = {
            let st = &self.online.borrow().state;
            (st.games.len(), st.chat.len())
        };
        // A panel moved: its controls go with it.
        let moved = self.layout.take_moved();
        let restyled = std::mem::take(&mut self.restyled);
        if out.is_some() || now != self.seen || moved || restyled {
            return self.again();
        }
        None
    }

    fn char(&mut self, _ctx: &mut FrontCtx, unit: u16) {
        // A letter with Ctrl held is a chord (the market key), not text.
        if self.market.ctrl_held() {
            return;
        }
        if self.market.shown() {
            // Backspace, Enter and Esc come as keys.
            if !matches!(unit, 8 | 13 | 27) {
                self.market.char(unit);
            }
            return;
        }
        let Some(c) = char::from_u32(u32::from(unit)) else {
            return;
        };
        if let Phase::Password(_) = self.phase {
            if self.message.is_none() {
                self.join_password.type_char(c, |_| true);
            }
            return;
        }
        if self.modal_up() || self.drop.open {
            return;
        }
        let view = self.view;
        match view {
            View::Create => {
                let focus = self.focus;
                let f = self.form_field(focus);
                match focus {
                    0 => f.type_char(c, |c| {
                        c.is_ascii_alphanumeric() || c == ' ' || c == '-' || c == '_'
                    }),
                    3 => f.type_char(c, |c| c.is_ascii_digit() || c == '-'),
                    _ => f.type_char(c, |_| true),
                };
            }
            View::Ladder | View::Friends if self.panel().is_some_and(|p| p.char(c)) => {}
            _ if !self.online.borrow().state.chat_off => self.chat.type_char(c),
            _ => {}
        }
    }

    fn key(&mut self, _ctx: &mut FrontCtx, k: u16) -> Option<Trigger> {
        if k == VK_CONTROL {
            self.market.ctrl(true);
            return None;
        }
        if self.market.shown() {
            match k {
                _ if self.market.is_chord(k) => return self.toggle_market(),
                vk::ESC => {
                    self.market.esc();
                }
                vk::BACKSPACE | vk::ENTER => {
                    self.market.char(k);
                }
                _ => {}
            }
            return self.again();
        }
        if self.picker.open {
            if k == vk::ESC {
                self.picker.close();
                return self.again();
            }
            return None;
        }
        if self.market.is_chord(k) && !self.modal_up() {
            return self.toggle_market();
        }
        if let Phase::Password(_) = self.phase {
            if k == vk::BACKSPACE && self.message.is_none() {
                self.join_password.backspace();
            }
            return None;
        }
        if self.modal_up() {
            return None;
        }
        const UP: u16 = 38;
        const DOWN: u16 = 40;
        const PAGE_UP: u16 = 33;
        const PAGE_DOWN: u16 = 34;
        if self.drop.open {
            let entries = self.drop_entries();
            match k {
                UP => self.drop.step(&entries, -1),
                DOWN => self.drop.step(&entries, 1),
                vk::ENTER => {
                    let action = entries.get(self.drop.hot).map(|e| e.action);
                    return match action {
                        Some(a) => self.choose(a),
                        None => None,
                    };
                }
                vk::ESC => {
                    self.drop.close();
                    return self.again();
                }
                _ => {}
            }
            return None;
        }
        if matches!(self.view, View::Ladder | View::Friends)
            && self.panel().is_some_and(|p| p.key(k))
        {
            return self.again();
        }
        match (self.view, k) {
            (View::Create, vk::BACKSPACE) => {
                let focus = self.focus;
                self.form_field(focus).backspace();
            }
            (View::Create, vk::TAB) => self.focus = (self.focus + 1) % FORM_FIELDS.len(),
            (_, vk::BACKSPACE) if self.chat_typing() => self.chat.backspace(),
            (View::List, UP) => return self.step(-1),
            (View::List, DOWN) => return self.step(1),
            (_, PAGE_UP) => self.chat.model.scroll_by(8),
            (_, PAGE_DOWN) => self.chat.model.scroll_by(-8),
            _ => {}
        }
        None
    }

    fn key_up(&mut self, _ctx: &mut FrontCtx, k: u16) {
        if k == VK_CONTROL {
            self.market.ctrl(false);
        }
    }

    fn wheel(&mut self, _ctx: &mut FrontCtx, delta: i32) -> Option<Trigger> {
        if self.market.shown() {
            self.market.wheel(delta);
            return None;
        }
        if self.modal_up() || self.drop.open {
            return None;
        }
        match self.view {
            View::List => self.scroll(-delta.signum()),
            View::Ladder | View::Friends => {
                let out = self.panel().and_then(|p| p.wheel(delta));
                self.panel_out(out)
            }
            View::Create => None,
        }
    }

    fn pointer(&mut self, _ctx: &mut FrontCtx, p: Point) {
        self.pointer = p;
        self.market.pointer(Point::new(p.x - self.shifts().0, p.y));
        if self.picker.open {
            self.chat.hover(None);
            self.picker.pointer(p);
            return;
        }
        // The chat has the pointer where the right panel isn't over it
        // and no SERVER list is open.
        let on_chat = !self.drop.open && self.layout.top_at(p) != Some(layout::RIGHT_ID);
        self.chat.hover(on_chat.then_some(p));
        if self.drop.open {
            let entries = self.drop_entries();
            let boxed = self.server_box_now();
            self.drop.point(&entries, boxed, p);
            return;
        }
        // A pop-up or the market has the pointer (a panel held when it
        // opened lands on the release).
        if !(self.modal_up() || self.market.shown()) || self.layout.holding() {
            self.layout.pointer(p);
        }
    }

    fn mouse(&mut self, ctx: &mut FrontCtx, down: bool, p: Point) -> Option<Trigger> {
        if !down {
            // A held panel lands (the tick builds the screen again).
            let note = self.layout.release(p);
            self.layout_note(note);
        }
        if self.picker.open {
            if !down {
                return None;
            }
            // A name picks it; anywhere else the list just closes.
            if let Some(i) = self.picker.row_at(p) {
                let (hd, _) = self.styled();
                self.edit = Some((hd, Backdrop::ALL[i]));
            }
            self.picker.close();
            return self.again();
        }
        if self.market.shown() {
            self.market
                .mouse(down, Point::new(p.x - self.shifts().0, p.y));
            // Closed (its X): the lobby takes the input again.
            if !self.market.shown() {
                return self.again();
            }
            return None;
        }
        if !down {
            return None;
        }
        // The open SERVER list (its entries and the click outside) has the
        // click.
        if self.modal_up() || self.drop.open {
            return None;
        }
        // A title strip or an edge takes the panel (a chat tab is the
        // tab's).
        let free = !self.chat.on_tab(p);
        if self.layout.press(p, free) {
            return None;
        }
        // The right panel's points as at its classic place.
        let q = self.layout.to_right(p);
        let area = self.area();
        if !area.contains(q) {
            if let Some(panel) = self.panel() {
                panel.blur();
            }
        }
        // Where the panels overlap, the one on top has the click.
        let top = self.layout.top_at(p);
        if top != Some(layout::RIGHT_ID) && self.chat.click(p) {
            return None;
        }
        if top == Some(layout::CHAT_ID) {
            return None;
        }
        match self.view {
            View::List => {}
            View::Ladder | View::Friends if area.contains(q) => {
                let now = ctx.now_ms;
                let out = self.panel().and_then(|panel| panel.mouse(q, now));
                return self.panel_out(out);
            }
            _ => return None,
        }
        let games = self.games();
        let g = games.get(self.row_at(q)?)?;
        let double = self
            .last_click
            .is_some_and(|(t, id)| id == g.id && ctx.now_ms.saturating_sub(t) <= DOUBLE_MS);
        self.last_click = Some((ctx.now_ms, g.id));
        self.selected = Some(g.id);
        if double {
            return self.join();
        }
        self.again()
    }

    fn overlay(&mut self, now_ms: u64, adv: &dyn Fn(u16, &[u16]) -> i32) -> Vec<DrawItem> {
        let width = |font: u16, s: &str| adv(font, &s.encode_utf16().collect::<Vec<u16>>());
        // The fields show as much of their end as their boxes hold.
        self.field_fits = [
            &self.game_name,
            &self.game_password,
            &self.join_password,
            &self.description,
            &self.levels,
        ]
        .map(|f| field_fit(f, adv));
        let mut out = Vec::new();
        let area = self.area();
        match self.view {
            View::List => {
                // The rows, then the selected row's frame over a greyed
                // row's shade (a pop-up draws above the overlay).
                out.extend(self.list_overlay(adv));
                if let Some(i) = self
                    .selected
                    .and_then(|id| self.games().iter().position(|g| g.id == id))
                {
                    if i >= self.first && i < self.first + self.rows() {
                        out.push(DrawItem::Border {
                            at: Point::new(area.x, Self::row_top(i - self.first)),
                            w: area.w as i32 - 16,
                            h: ROW_H,
                        });
                    }
                }
            }
            View::Ladder | View::Friends => {
                if let Some(p) = self.panel() {
                    out.extend(p.overlay(area, adv, now_ms));
                }
            }
            View::Create if self.has_servers() => {
                let (text, right) = server_drop::shown(&self.online.borrow().state, &self.place);
                out.extend(server_drop::box_items(
                    self.server_box(),
                    &text,
                    &right,
                    &width,
                ));
            }
            View::Create => {}
        }
        layout::shift_items(&mut out, self.layout.right_offset());
        let mut right = std::mem::take(&mut out);
        let off = self
            .online
            .borrow()
            .state
            .chat_off
            .then_some("Chat is not available on this server.");
        let focused = self.chat_typing() && !self.modal_up();
        let mut chat = self.chat.draw(adv, now_ms, focused, off);
        // The panels from the bottom one up; a floating one hides what is
        // under it, its controls drawn with it.
        let floating = self.layout.floating();
        for id in self.layout.order() {
            let floats = floating.contains(&id);
            if floats {
                out.extend(self.layout.backdrop(id, BG));
            }
            if id == layout::RIGHT_ID {
                if floats {
                    out.push(DrawItem::Raised(RAISED_RIGHT));
                }
                out.append(&mut right);
            } else {
                out.append(&mut chat);
            }
        }
        // The open SERVER list over everything.
        if self.drop.open {
            let entries = self.drop_entries();
            let mut list = server_drop::list_items(self.server_box(), &entries, &self.drop, &width);
            layout::shift_items(&mut list, self.layout.right_offset());
            out.extend(list);
        }
        let (mid, right) = self.shifts();
        if self.market.shown() {
            let mut window = self.market.draw();
            layout::shift_items(&mut window, (mid, 0));
            out.extend(window);
        } else if !self.modal_up() {
            out.extend(self.layout.outline());
            if self.market_tip_shown() {
                // MARKET's tip, right-aligned under it, while the mouse is
                // on it.
                let tip = self.market.tip();
                let units: Vec<u16> = tip.encode_utf16().collect();
                let x = MARKET_TIP_RIGHT + right - adv(FONT8, &units);
                out.push(text_item(FONT8, x, MARKET_TIP_Y, GREY, tip));
            }
            if self.wide {
                out.extend(self.picker.overlay(self.styled().1, adv));
            }
        }
        out
    }

    fn underlay(&self) -> Vec<DrawItem> {
        if !self.wide {
            return self.layout.underlay(BG);
        }
        // The character's plate, then the panels on their backings.
        let mut plate = INFO_PLATE;
        plate.x += self.shifts().1;
        let mut out = layout::backing(plate);
        out.push(DrawItem::Frame {
            file: BG,
            rect: plate,
            style: crate::ui::float::Style::Rope,
        });
        out.extend(self.layout.underlay(BG));
        out.extend(Picker::underlay());
        out
    }

    fn wide(&self) -> bool {
        true
    }

    fn canvas_changed(&mut self, canvas: crate::ui::front_end::canvas::FrontCanvas) {
        let wide = canvas.full && canvas.width >= FrameSize::WIDE.width;
        if wide != self.wide {
            self.wide = wide;
            self.picker.close();
            let note = self.layout.set_wide(wide);
            self.layout_note(note);
        }
    }

    fn relayout(&mut self, ctx: &mut FrontCtx) -> Option<Vec<Control>> {
        self.rebuild = true;
        Some(self.build(ctx))
    }

    fn settings(&mut self, s: &mut crate::app::config::Settings) -> bool {
        let edit = self.edit.take();
        if let Some((hd, b)) = edit {
            s.hd_text = hd;
            s.lobby_background = b;
        }
        let now = (s.hd_text, s.lobby_background);
        if now != self.shown {
            self.shown = now;
            self.restyled = true;
        }
        edit.is_some()
    }

    fn sync(&mut self, controls: &mut [Control]) {
        let live = !self.modal_up();
        let prompt = matches!(self.phase, Phase::Password(_)) && self.message.is_none();
        let fields = [
            (&self.game_name, live && self.focus == 0),
            (&self.game_password, live && self.focus == 1),
            (&self.join_password, prompt),
            (&self.description, live && self.focus == 2),
            (&self.levels, live && self.focus == 3),
        ];
        for ((ix, (f, focused)), keep) in self.text_ix.iter().zip(fields).zip(self.field_fits) {
            if let Some(c) = ix.and_then(|i| controls.get_mut(i)) {
                show_field(c, f, focused, self.now_ms, keep);
            }
        }
        let now = self.now_ms;
        if let Some(p) = self.panel() {
            p.sync(controls, now);
        }
    }

    fn covered(&self, i: usize, p: Point) -> bool {
        self.covers.iter().any(|(j, r)| *j == i && r.contains(p))
    }

    fn ui_elements(&self) -> Vec<UiElement> {
        if self.market.shown() {
            let mut v = self.market.ui_elements();
            layout::shift_elements(&mut v, (self.shifts().0, 0));
            return v;
        }
        if self.modal_up() {
            return Vec::new();
        }
        if self.picker.open {
            return Backdrop::ALL
                .into_iter()
                .enumerate()
                .map(|(i, b)| {
                    UiElement::new(
                        format!("online_lobby.background({})", b.key()),
                        Picker::row(i),
                    )
                })
                .collect();
        }
        let mut v = self.chat.ui_elements();
        let area = self.area();
        let mut right = match self.view {
            View::Ladder => self.ladder.ui_elements(),
            View::Friends => self.friends.ui_elements(),
            View::Create => Vec::new(),
            View::List => self
                .games()
                .iter()
                .enumerate()
                .skip(self.first)
                .take(self.rows())
                .map(|(i, g)| {
                    UiElement::new(
                        format!("online_lobby.game({})", g.name),
                        Rect::new(
                            area.x,
                            Self::row_top(i - self.first),
                            area.w - 16,
                            ROW_H as u16,
                        ),
                    )
                })
                .collect(),
        };
        layout::shift_elements(&mut right, self.layout.right_offset());
        v.extend(right);
        // Where the texts stay ([`super::text_fit`]): the panels from the
        // bottom one up, each followed by its parts.
        let mut rooms = Vec::new();
        for id in self.layout.order() {
            if id == layout::RIGHT_ID {
                rooms.push(("right", self.layout.right()));
            } else {
                rooms.push(("chat", self.layout.chat()));
                rooms.extend(self.chat.text_rooms());
            }
        }
        v.extend(
            rooms
                .into_iter()
                .map(|(n, r)| UiElement::new(format!("online_lobby.text_room({n})"), r)),
        );
        v
    }
}

/// A text item with the pen at (x, baseline y).
fn text_item(font: u16, x: i32, y: i32, color: i32, text: &str) -> DrawItem {
    DrawItem::Text {
        string_id: 0,
        text: text.to_string(),
        font,
        at: Point::new(x, y),
        label: None,
        color,
        boxed: None,
    }
}

/// The mark before a player-hosted game's host in the list.
pub const HOST_MARK: &str = "Host";

/// The list's difficulty column.
pub fn difficulty_short(d: u8) -> &'static str {
    match d {
        0 => "Norm",
        1 => "NM",
        _ => "Hell",
    }
}

/// The list shows the server column: the main server lists game servers,
/// or a listed game names its server.
fn list_has_servers(st: &OnlineState, games: &[GameInfo]) -> bool {
    !st.nodes.is_empty() || games.iter().any(|g| !g.server.is_empty())
}

/// A game's server cell after the [`HOST_MARK`] (for a player-hosted
/// game): the server's label (or the host's character name) cut to fit
/// `w` with the ping kept whole: `EU N1 18 ms`, `Rhaego ~64 ms`, `-` for no
/// answer, `...` while measuring.
pub fn server_cell(st: &OnlineState, g: &GameInfo, w: i32, width: &dyn Fn(&str) -> i32) -> String {
    if g.player_hosted {
        let ping = st
            .host_ping(g)
            .map_or_else(|| "-".to_string(), |ms| format!("~{ms} ms"));
        let room = w - width(HOST_MARK) - width(" ");
        return fit_with(&g.server, &ping, room, width);
    }
    let ping = match st.node_of(g) {
        Some(n) => ping_text(st.ping(&n.node)),
        None if g.node_id.is_empty() => String::new(),
        None => ping_text(st.ping(&g.node_id)),
    };
    fit_with(&g.server, &ping, w, width)
}

/// `text` wrapped to `w` onto `lines`, `most` lines at most (the last
/// one cut).
fn push_wrapped(
    lines: &mut Vec<(String, i32)>,
    text: &str,
    colour: i32,
    most: usize,
    w: i32,
    width: &dyn Fn(&str) -> i32,
) {
    if text.is_empty() || most == 0 {
        return;
    }
    let mut rows = wrap(text, w, width);
    if rows.len() > most {
        let rest = rows.split_off(most - 1).join(" ");
        rows.push(fit(&rest, w, width));
    }
    lines.extend(rows.into_iter().map(|r| (r, colour)));
}

/// `label` cut so that `label tail` fits `w` (the tail kept whole).
fn fit_with(label: &str, tail: &str, w: i32, width: &dyn Fn(&str) -> i32) -> String {
    if tail.is_empty() {
        return fit(label, w, width);
    }
    let room = w - width(tail) - width(" ");
    let label = fit(label, room, width);
    if label.is_empty() {
        tail.to_string()
    } else {
        format!("{label} {tail}")
    }
}

/// Why character `c` can't join `g`, as the list's grey text says it
/// (lobby contract section 7); empty for a game it can join.
pub fn grey_reason(g: &GameInfo, c: Option<&CharInfo>) -> String {
    match ErrorCode(g.joinable) {
        ErrorCode(0) => String::new(),
        ErrorCode::DEAD_HARDCORE => "dead character".into(),
        ErrorCode::WRONG_GAME_KIND => {
            let st = c.map_or(0, |c| c.status);
            let hc = st & status::HARDCORE != 0;
            if g.hardcore != hc || c.is_none() {
                if g.hardcore {
                    "hardcore game"
                } else {
                    "softcore game"
                }
                .into()
            } else if g.expansion {
                "expansion game".into()
            } else {
                "classic game".into()
            }
        }
        ErrorCode::DIFFICULTY_LOCKED => format!("{} not unlocked", difficulty_name(g.difficulty)),
        lobby_code::LEVEL_RANGE => match (g.level_min, g.level_max) {
            (0, 0) => "level range".into(),
            (min, 0) => format!("levels {min}+"),
            (0, max) => format!("levels 1-{max}"),
            (min, max) => format!("levels {min}-{max}"),
        },
        ErrorCode::GAME_FULL => "full".into(),
        code => failure_text(&Failure {
            code,
            wait_s: 0,
            text: String::new(),
        })
        .trim_end_matches('.')
        .to_string(),
    }
}

/// The first line under the list for the selected game: why it can't be
/// joined (red), or what it is: description, level range, server and
/// password.
pub fn selected_line(st: &OnlineState, g: &GameInfo, c: Option<&CharInfo>) -> (String, i32) {
    if g.joinable != 0 {
        return (format!("{}: {}", g.name, refusal_sentence(g, c)), RED);
    }
    let mut parts = Vec::new();
    if !g.description.is_empty() {
        parts.push(g.description.clone());
    }
    match (g.level_min, g.level_max) {
        (0, 0) => {}
        (min, 0) => parts.push(format!("levels {min}+")),
        (0, max) => parts.push(format!("levels 1-{max}")),
        (min, max) => parts.push(format!("levels {min}-{max}")),
    }
    if g.player_hosted {
        let ping = st
            .host_ping(g)
            .map_or_else(|| "-".to_string(), |ms| format!("~{ms} ms"));
        parts.push(format!("player hosted by {}, {ping}", g.server));
    } else if !g.server.is_empty() {
        parts.push(match st.node_of(g) {
            Some(n) => format!("on {}, {}", n.name, ping_text(st.ping(&n.node))),
            None => format!("on {}", g.server),
        });
    }
    if g.locked {
        parts.push("password game".into());
    }
    if parts.is_empty() {
        return (String::new(), WHITE);
    }
    (format!("{}: {}", g.name, parts.join(" - ")), WHITE)
}

/// Why character `c` can't join `g`, as a sentence: the selected grey
/// game's line under the list (the row and the legend say it short).
pub fn refusal_sentence(g: &GameInfo, c: Option<&CharInfo>) -> String {
    match ErrorCode(g.joinable) {
        ErrorCode::DIFFICULTY_LOCKED => {
            format!("{} is not unlocked yet.", difficulty_name(g.difficulty))
        }
        ErrorCode::GAME_FULL => "The game is full.".into(),
        ErrorCode::DEAD_HARDCORE => "A dead hardcore character can't join.".into(),
        ErrorCode::WRONG_GAME_KIND => {
            let kind = grey_reason(g, c);
            format!("For {} characters.", kind.trim_end_matches(" game"))
        }
        lobby_code::LEVEL_RANGE => format!("For {} only.", grey_reason(g, c)),
        code => failure_text(&Failure {
            code,
            wait_s: 0,
            text: String::new(),
        }),
    }
}

/// The line naming why the grey games are grey (each reason once, in list
/// order); `None` when every game can be joined.
pub fn grey_legend(games: &[GameInfo], c: Option<&CharInfo>) -> Option<String> {
    let mut reasons: Vec<String> = Vec::new();
    for g in games.iter().filter(|g| g.joinable != 0) {
        let r = grey_reason(g, c);
        if !reasons.contains(&r) {
            reasons.push(r);
        }
    }
    (!reasons.is_empty()).then(|| format!("Grey: {}", reasons.join(" / ")))
}

/// The form's level range: blank (none), `40-70`, `40-` or `40` (from 40),
/// `-70` (up to 70). Levels are 1-99, min ≤ max, and the character's own
/// `level` must be inside (the realm refuses it otherwise, `LevelRange`).
pub fn parse_levels(text: &str, level: u8) -> Result<(u8, u8), String> {
    let text = text.trim();
    if text.is_empty() {
        return Ok((0, 0));
    }
    let bad = || "Type the level range as two levels, like 40-70 (or 40- or -70).".to_string();
    let num = |s: &str| -> Result<u8, String> {
        if s.is_empty() {
            return Ok(0);
        }
        match s.parse::<u8>() {
            Ok(n) if (1..=99).contains(&n) => Ok(n),
            _ => Err("Levels are 1 to 99.".to_string()),
        }
    };
    let (min, max) = match text.split_once('-') {
        Some((a, b)) if !b.contains('-') => (num(a.trim())?, num(b.trim())?),
        Some(_) => return Err(bad()),
        None => (num(text)?, 0),
    };
    if min == 0 && max == 0 {
        return Err(bad());
    }
    if min != 0 && max != 0 && min > max {
        return Err("The lowest level must not be above the highest.".into());
    }
    if (min != 0 && level < min) || (max != 0 && level > max) {
        return Err(format!(
            "Your level ({level}) must be inside the game's level range."
        ));
    }
    Ok((min, max))
}

/// The character line under the difficulties: the guild's tag and the
/// rank in its own ladder (`Guild <NUG> - Ladder rank #37`), or what of it
/// is known.
pub fn standing_line(tag: Option<&str>, rank: Option<u32>, season: u16) -> Option<String> {
    let guild = tag.map(|t| format!("Guild <{t}>"));
    // A non-ladder character has no rank to show.
    let rank = rank
        .filter(|_| season != 0)
        .map(|r| format!("Ladder rank #{r}"));
    match (guild, rank) {
        (Some(g), Some(r)) => Some(format!("{g} - {r}")),
        (g, r) => g.or(r),
    }
}

/// The ladder view a character is ranked in: its season, hardcore or
/// softcore, expansion or classic, all classes; the page around it.
pub fn rank_query(c: &CharInfo) -> LadderQuery {
    use super::link::ladder_split;
    let mut split = 0;
    if c.status & status::HARDCORE != 0 {
        split |= ladder_split::HARDCORE;
    }
    if c.status & status::EXPANSION != 0 {
        split |= ladder_split::EXPANSION;
    }
    LadderQuery {
        season: c.season,
        split,
        class: 255,
        at: LadderAt::Me,
        rows: 1,
    }
}

/// `text` cut into rows no wider than `w` (at spaces; a word wider than a
/// row is cut where it overflows).
pub fn wrap(text: &str, w: i32, width: &dyn Fn(&str) -> i32) -> Vec<String> {
    let mut rows = Vec::new();
    let mut row = String::new();
    for word in text.split(' ') {
        let candidate = if row.is_empty() {
            word.to_string()
        } else {
            format!("{row} {word}")
        };
        if width(&candidate) <= w || row.is_empty() && width(word) <= w {
            row = candidate;
            continue;
        }
        if !row.is_empty() {
            rows.push(std::mem::take(&mut row));
        }
        let mut piece = String::new();
        for c in word.chars() {
            piece.push(c);
            if width(&piece) > w {
                piece.pop();
                rows.push(std::mem::take(&mut piece));
                piece.push(c);
            }
        }
        row = piece;
    }
    if !row.is_empty() || rows.is_empty() {
        rows.push(row);
    }
    rows
}

pub fn register(reg: &mut Registry, online: OnlineHandle) {
    reg.register(ONLINE_LOBBY, Box::new(Lobby::new(online)));
}

#[cfg(test)]
mod tests;
