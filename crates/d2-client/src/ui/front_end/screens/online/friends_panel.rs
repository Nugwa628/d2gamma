//! The lobby's Friends panel (plan `lobby-ui.md` L12; lobby contract
//! sections 3.4 and 5) in the lobby's right panel through [`RightPanel`].
//! A strip of four tabs at its top:
//!
//! - **Friends**: each friend's last character (class, level and the
//!   guild's `<TAG>`), a `*` after a mutual friend, and where they are:
//!   "in the lobby", the game's name, "online" (a one-sided friend sees no
//!   more), "hidden" (their privacy), or "last seen 2 h ago". Online
//!   friends first. A name in the box and ADD adds a friend (one-sided: no
//!   answer needed); REMOVE, WHISPER (starts `/w NAME` in the chat's input
//!   line) and JOIN (a mutual friend's game, through the lobby's own join
//!   with its password prompt) act on the selected friend. Enter in the box
//!   adds; JOIN or Enter otherwise joins the selected friend's game.
//! - **Ignored**: the ignore list; IGNORE and UNIGNORE.
//! - **Privacy**: who may whisper (everyone, friends and guild, nobody) and
//!   whether mutual friends see where the player is.
//! - **Guild**: [`GuildPanel`].
//!
//! The requests go out on the lobby's link; the lists are fetched when the
//! panel opens and again after a change or a friend's notice. What a
//! request did (or why the realm refused it) is the line under the panel.
//! A realm without friends (`NotSupported`) gets a line that says so.

use std::collections::VecDeque;

use crate::ui::chat::users::class_short;
use crate::ui::front_end::{Action, Control, ControlKind, DrawItem};
use crate::ui::geom::{Point, Rect};
use crate::ui::ui_ids::UiElement;

use super::guild_panel::GuildPanel;
use super::link::{
    failure_text, friend_notice, Asked, ErrorCode, Event, Failure, FriendOp, FriendRow, Privacy,
    Request, Where, Who,
};
use super::lobby::{act as lobby_act, PanelOut, RightPanel};
use super::widgets::{
    field_controls, fit, show_field, Field, FONT16, FONT8, FONT_TINY, RADIO, SCROLL,
};
use super::widgets::{GOLD, GREEN, GREY, RED, WHITE};
use super::OnlineHandle;

/// The panel's actions (inside the lobby's `PANEL..PANEL_END`).
pub mod act {
    use super::lobby_act::PANEL;

    /// The tabs: `TAB + Sub` (0 Friends, 1 Ignored, 2 Privacy, 3 Guild).
    pub const TAB: u32 = PANEL;
    pub const FIELD: u32 = PANEL + 5;
    /// ADD on Friends, IGNORE on Ignored.
    pub const ADD: u32 = PANEL + 6;
    /// REMOVE on Friends, UNIGNORE on Ignored.
    pub const REMOVE: u32 = PANEL + 7;
    pub const WHISPER: u32 = PANEL + 8;
    pub const JOIN: u32 = PANEL + 9;
    pub const SCROLL_UP: u32 = PANEL + 10;
    pub const SCROLL_DOWN: u32 = PANEL + 11;
    /// The whisper choices: `WHISPERS + 0..3`.
    pub const WHISPERS: u32 = PANEL + 12;
    /// The place choices: `SHOW_WHERE + 0..2`.
    pub const SHOW_WHERE: u32 = PANEL + 15;
    /// The guild tab's own: `GUILD..PANEL_END`.
    pub const GUILD: u32 = PANEL + 40;
}

/// The panel's tabs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sub {
    Friends,
    Ignored,
    Privacy,
    Guild,
}

const SUBS: [(Sub, &str); 4] = [
    (Sub::Friends, "FRIENDS"),
    (Sub::Ignored, "IGNORED"),
    (Sub::Privacy, "PRIVACY"),
    (Sub::Guild, "GUILD"),
];

/// The tab strip's top and every chip's height (inside the panel).
pub(super) const STRIP_TOP: i32 = 2;
pub(super) const CHIP_H: u16 = 16;
const CHIP_GAP: i32 = 3;
/// A chip label's least room from either side of its box.
pub(super) const CHIP_PAD: i32 = 5;
/// List rows: the first row's top, a row's height, rows shown.
const ROWS_TOP: i32 = 22;
pub(super) const ROW_H: i32 = 18;
pub const ROWS: usize = 6;
/// A row's text baseline below its top.
pub(super) const ROW_BASE: i32 = 14;
/// The name box: its cel bottom and x (a label left of it).
const FIELD_BOTTOM: i32 = 158;
pub(super) const FIELD_X: i32 = 44;
/// The buttons under the box.
const BUTTONS_TOP: i32 = 162;
/// The columns (from the panel's left): the names' room ends at the
/// class, whose text (class and level) ends [`COL_GAP`] before where.
pub(super) const COL_CLASS: i32 = 150;
pub(super) const COL_PLACE: i32 = 196;
pub(super) const COL_GAP: i32 = 6;
/// The rows' right edge (left of the scroll bar).
pub(super) const ROWS_RIGHT: i32 = 296;
/// The lines under the panel (baselines below its bottom).
pub(super) const UNDER: [i32; 2] = [15, 27];
/// Most characters of a character name (or `*account`) typed.
const NAME_MAX: usize = 33;
/// Double click.
const DOUBLE_MS: u64 = 500;

/// A clickable box with a label (the chat's tab look): a dark box with the
/// gold outline, the label centred.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Chip {
    pub at: Rect,
    pub label: &'static str,
    pub id: u32,
    /// Its UI element's name (`online_lobby.<name>`).
    pub name: String,
    /// Drawn as chosen (gold, double outline).
    pub on: bool,
}

/// Equal chips across `area`'s width, their tops `top` below its top.
pub(super) fn chip_row(area: Rect, top: i32, items: &[(&'static str, u32, String)]) -> Vec<Chip> {
    let n = items.len().max(1) as i32;
    let w = (i32::from(area.w) - CHIP_GAP * (n - 1)) / n;
    items
        .iter()
        .enumerate()
        .map(|(i, (label, id, name))| Chip {
            at: Rect::new(
                area.x + (w + CHIP_GAP) * i as i32,
                area.y + top,
                w as u16,
                CHIP_H,
            ),
            label,
            id: *id,
            name: name.clone(),
            on: false,
        })
        .collect()
}

/// The chips' click areas (they fire on the press).
pub(super) fn chip_controls(chips: &[Chip], v: &mut Vec<Control>) {
    for c in chips {
        let r = c.at;
        v.push(
            Control::new(ControlKind::Image, r.x, r.y + i32::from(r.h), r.w, r.h)
                .with_action(Action::Custom(c.id))
                .with_on_press(),
        );
    }
}

/// The chips' label fonts from the largest (Font8's capitals are near
/// Font16's width), with their baselines above the box's bottom.
const CHIP_FONTS: [(u16, i32); 3] = [(FONT16, 2), (FONT8, 3), (FONT_TINY, 4)];

/// The chips' boxes and labels; a row's labels in the first of
/// [`CHIP_FONTS`] each of them fits within its box's padding.
pub(super) fn chip_items(chips: &[Chip], adv: &dyn Fn(u16, &[u16]) -> i32) -> Vec<DrawItem> {
    let measure = |f: u16, c: &Chip| adv(f, &c.label.encode_utf16().collect::<Vec<u16>>());
    let row_font = |y: i32| {
        CHIP_FONTS
            .into_iter()
            .find(|&(f, _)| {
                chips
                    .iter()
                    .filter(|c| c.at.y == y)
                    .all(|c| measure(f, c) <= i32::from(c.at.w) - 2 * CHIP_PAD)
            })
            .unwrap_or(CHIP_FONTS[CHIP_FONTS.len() - 1])
    };
    let mut out = Vec::new();
    for c in chips {
        let r = c.at;
        let (w, h) = (i32::from(r.w), i32::from(r.h));
        out.push(DrawItem::Rect {
            at: r.origin(),
            w,
            h,
        });
        out.push(DrawItem::Border {
            at: r.origin(),
            w,
            h,
        });
        if c.on {
            out.push(DrawItem::Border {
                at: Point::new(r.x + 1, r.y + 1),
                w: w - 2,
                h: h - 2,
            });
        }
        let (font, up) = row_font(r.y);
        let (tw, base) = (measure(font, c), r.y + h - up);
        let colour = if c.on { GOLD } else { TAN };
        out.push(text(font, r.x + (w - tw) / 2, base, colour, c.label));
    }
    out
}

/// The chat tabs' label colour for a chip that isn't chosen.
pub(super) const TAN: i32 = 7;

/// A text item with the pen at (x, baseline y).
pub(super) fn text(font: u16, x: i32, y: i32, color: i32, text: &str) -> DrawItem {
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

/// A width measure for `font`.
pub(super) fn width(adv: &dyn Fn(u16, &[u16]) -> i32, font: u16) -> impl Fn(&str) -> i32 + '_ {
    move |s: &str| adv(font, &s.encode_utf16().collect::<Vec<u16>>())
}

/// How long ago `then` was at `now` (Unix ms): "just now", "5 min ago",
/// "2 h ago", "3 days ago".
pub fn ago(now_ms: u64, then_ms: u64) -> String {
    let min = now_ms.saturating_sub(then_ms) / 60_000;
    match min {
        0 => "just now".into(),
        m if m < 60 => format!("{m} min ago"),
        m if m < 48 * 60 => format!("{} h ago", m / 60),
        m => format!("{} days ago", m / (24 * 60)),
    }
}

/// Where a friend is, as the row says it, and its colour. `now_ms` is the
/// realm's clock.
pub fn place_text(f: &FriendRow, now_ms: u64) -> (String, i32) {
    match (f.place, &f.game) {
        (Where::Offline, _) if f.last_seen_ms == 0 => ("offline".into(), GREY),
        (Where::Offline, _) => (format!("last seen {}", ago(now_ms, f.last_seen_ms)), GREY),
        (Where::Lobby, _) => ("in the lobby".into(), GREEN),
        (Where::InGame, Some(g)) => (
            format!("in {}", g.game_name),
            if g.joinable == 0 { GOLD } else { GREY },
        ),
        (Where::InGame, None) => ("in a game".into(), GREEN),
        // A one-sided friend sees online or offline only; a mutual one
        // who hides the place says so.
        (Where::Hidden, _) if f.mutual => ("hidden".into(), GREY),
        (Where::Hidden, _) => ("online".into(), GREEN),
    }
}

/// How a friend is named in requests: `*account` when the realm showed it
/// (it holds when they play another character), else the character.
pub fn address(w: &Who) -> String {
    if w.account.is_empty() {
        w.char.clone()
    } else {
        format!("*{}", w.account)
    }
}

/// A player's shown name: the character, else the account.
pub(super) fn shown(w: &Who) -> &str {
    if w.char.is_empty() {
        &w.account
    } else {
        &w.char
    }
}

/// The realm's clock: its `now_ms` at a local time, moved on by the local
/// clock since.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct RealmClock {
    at: Option<(u64, u64)>,
}

impl RealmClock {
    pub(super) fn set(&mut self, realm_ms: u64, local_ms: u64) {
        self.at = Some((realm_ms, local_ms));
    }

    /// The realm's time at local time `local_ms` (the system clock before
    /// the realm said its time).
    pub(super) fn now(&self, local_ms: u64) -> u64 {
        match self.at {
            Some((realm, at)) => realm + local_ms.saturating_sub(at),
            None => std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_millis() as u64),
        }
    }
}

pub struct FriendsPanel {
    pub online: OnlineHandle,
    /// The fourth tab.
    pub guild: GuildPanel,
    pub tab: Sub,
    /// The friends, online ones first, and the ignored players.
    pub friends: Vec<FriendRow>,
    pub ignored: Vec<FriendRow>,
    /// The lists arrived (friends, ignored).
    loaded: (bool, bool),
    /// The realm has no friends lists.
    pub unsupported: bool,
    pub privacy: Privacy,
    /// The selected row's [`address`].
    pub selected: Option<String>,
    /// The first row shown.
    pub first: usize,
    /// The name box (friends and ignored tabs).
    pub name: Field,
    pub focused: bool,
    /// The list changes out, oldest first (their answers come in order).
    edits: VecDeque<FriendOp>,
    /// A notice or a change asks for the lists again when next shown.
    stale: bool,
    /// The line under the panel: what the last request did.
    pub note: Option<(String, i32)>,
    clock: RealmClock,
    now_ms: u64,
    last_click: Option<(u64, String)>,
    /// The name box's text control in the built list.
    text_ix: Option<usize>,
    /// The chips as last built.
    chips: Vec<Chip>,
    area: Rect,
}

impl FriendsPanel {
    pub fn new(online: OnlineHandle) -> Self {
        FriendsPanel {
            guild: GuildPanel::new(online.clone()),
            online,
            tab: Sub::Friends,
            friends: Vec::new(),
            ignored: Vec::new(),
            loaded: (false, false),
            unsupported: false,
            privacy: Privacy::default(),
            selected: None,
            first: 0,
            name: Field::new(NAME_MAX),
            focused: false,
            edits: VecDeque::new(),
            stale: false,
            note: None,
            clock: RealmClock::default(),
            now_ms: 0,
            last_click: None,
            text_ix: None,
            chips: Vec::new(),
            area: super::lobby::PANEL,
        }
    }

    fn send(&self, r: Request) {
        self.online.borrow_mut().send(r);
    }

    /// Asks for both lists.
    fn fetch(&mut self) {
        self.stale = false;
        self.send(Request::FriendsGet { ignored: false });
        self.send(Request::FriendsGet { ignored: true });
    }

    /// The rows of the open list.
    pub fn rows(&self) -> &[FriendRow] {
        match self.tab {
            Sub::Ignored => &self.ignored,
            _ => &self.friends,
        }
    }

    pub fn selected_row(&self) -> Option<&FriendRow> {
        let key = self.selected.as_ref()?;
        self.rows().iter().find(|r| address(&r.who) == *key)
    }

    /// A list change goes out.
    fn edit(&mut self, op: FriendOp) {
        self.edits.push_back(op.clone());
        self.send(Request::FriendEdit(op));
    }

    fn say(&mut self, text: impl Into<String>, colour: i32) -> Option<PanelOut> {
        self.note = Some((text.into(), colour));
        Some(PanelOut::Rebuild)
    }

    /// The typed name, trimmed (`None`: empty).
    fn typed(&self) -> Option<String> {
        let n = self.name.text.trim().to_string();
        (!n.is_empty()).then_some(n)
    }

    /// ADD (friends) or IGNORE (ignored) with the typed name.
    fn add(&mut self) -> Option<PanelOut> {
        let Some(name) = self.typed() else {
            return self.say(
                "Type a character name (or *account) in the box first.",
                WHITE,
            );
        };
        self.name.text.clear();
        self.focused = false;
        let op = if self.tab == Sub::Ignored {
            FriendOp::Ignore(name)
        } else {
            FriendOp::Add(name)
        };
        self.edit(op);
        Some(PanelOut::Rebuild)
    }

    /// REMOVE (friends) or UNIGNORE (ignored) the selected row.
    fn remove(&mut self) -> Option<PanelOut> {
        let Some(r) = self.selected_row() else {
            return self.say("Choose a player in the list first.", WHITE);
        };
        let who = address(&r.who);
        let op = if self.tab == Sub::Ignored {
            FriendOp::Unignore(who)
        } else {
            FriendOp::Remove(who)
        };
        self.edit(op);
        Some(PanelOut::Rebuild)
    }

    fn whisper(&mut self) -> Option<PanelOut> {
        match self.selected_row() {
            Some(r) => Some(PanelOut::Whisper(whisper_name(&r.who))),
            None => self.say("Choose a friend in the list first.", WHITE),
        }
    }

    /// JOIN: the selected friend's game, when the character may join it.
    fn join(&mut self) -> Option<PanelOut> {
        let Some(r) = self.selected_row().cloned() else {
            return self.say("Choose a friend in a game first.", WHITE);
        };
        let name = shown(&r.who).to_string();
        match r.game {
            Some(g) if g.joinable == 0 => Some(PanelOut::Join(g.game_id)),
            Some(g) => {
                let why = failure_text(&Failure {
                    code: ErrorCode(g.joinable),
                    wait_s: 0,
                    text: String::new(),
                });
                self.say(why, RED)
            }
            None if !r.mutual => self.say(
                format!("Only mutual friends see each other's games: ask {name} to add you."),
                WHITE,
            ),
            None => self.say(format!("{name} is not in a game you can see."), WHITE),
        }
    }

    /// A privacy choice goes out (applied when the realm says done).
    fn set_privacy(&mut self, p: Privacy) -> Option<PanelOut> {
        if p != self.privacy {
            self.edit(FriendOp::Privacy(p));
        }
        Some(PanelOut::Rebuild)
    }

    /// The answer to the oldest list change.
    fn edited(&mut self, refused: Option<&Failure>) {
        let Some(op) = self.edits.pop_front() else {
            // The chat's own change (`/f add`): the lists follow.
            self.stale |= refused.is_none();
            return;
        };
        if let Some(f) = refused {
            self.note = Some((failure_text(f), RED));
            return;
        }
        let (text, colour) = match &op {
            FriendOp::Add(n) => (format!("{} is on your friends list.", bare(n)), GREEN),
            FriendOp::Remove(n) => (format!("{} is off your friends list.", bare(n)), WHITE),
            FriendOp::Ignore(n) => (format!("You ignore {}.", bare(n)), WHITE),
            FriendOp::Unignore(n) => (format!("You no longer ignore {}.", bare(n)), WHITE),
            FriendOp::Privacy(p) => {
                self.privacy = *p;
                ("Your privacy choices are saved.".into(), GREEN)
            }
        };
        if let FriendOp::Remove(n) | FriendOp::Unignore(n) = &op {
            if self.selected.as_deref() == Some(n.as_str()) {
                self.selected = None;
            }
        }
        self.note = Some((text, colour));
        self.stale = true;
    }

    /// A friend's notice: their row follows.
    fn notice(&mut self, n: &super::link::FriendNotice) {
        let name = shown(&n.who).to_string();
        if n.what == friend_notice::ADDED_YOU {
            self.note = Some((format!("{name} added you as a friend."), GREEN));
            self.stale = true;
            return;
        }
        let key = address(&n.who);
        let at = n.at_ms;
        if let Some(r) = self.friends.iter_mut().find(|r| address(&r.who) == key) {
            r.who = n.who.clone();
            r.place = n.place;
            r.game = n.game.clone();
            if n.place == Where::Offline {
                r.last_seen_ms = at;
            }
        } else {
            self.stale = true;
        }
        let text = match n.what {
            friend_notice::ONLINE => format!("{name} came online."),
            friend_notice::OFFLINE => format!("{name} went offline."),
            friend_notice::ENTERED_GAME => match &n.game {
                Some(g) => format!("{name} entered {}.", g.game_name),
                None => format!("{name} entered a game."),
            },
            friend_notice::LEFT_GAME => format!("{name} left the game."),
            _ => return,
        };
        sort_friends(&mut self.friends);
        self.note = Some((text, WHITE));
    }

    /// Moves the selection by `d` rows, keeping it in view.
    fn step(&mut self, d: i32) {
        let keys: Vec<String> = self.rows().iter().map(|r| address(&r.who)).collect();
        if keys.is_empty() {
            return;
        }
        let at = self
            .selected
            .as_ref()
            .and_then(|k| keys.iter().position(|o| o == k))
            .map_or(0, |i| {
                (i as i32 + d).clamp(0, keys.len() as i32 - 1) as usize
            });
        self.selected = Some(keys[at].clone());
        if at < self.first {
            self.first = at;
        } else if at >= self.first + ROWS {
            self.first = at + 1 - ROWS;
        }
    }

    fn scroll(&mut self, d: i32) {
        let max = self.rows().len().saturating_sub(ROWS);
        self.first = (self.first as i32 + d).clamp(0, max as i32) as usize;
    }

    /// The top of row `r` (screen y).
    fn row_top(&self, r: usize) -> i32 {
        self.area.y + ROWS_TOP + ROW_H * r as i32
    }

    fn tab_chips(&self, area: Rect) -> Vec<Chip> {
        let invites = self.guild.invites_waiting();
        let items: Vec<(&'static str, u32, String)> = SUBS
            .iter()
            .enumerate()
            .map(|(i, (sub, label))| {
                let label = match sub {
                    Sub::Guild if invites > 0 => "GUILD (!)",
                    _ => *label,
                };
                (
                    label,
                    act::TAB + i as u32,
                    format!("friends_tab({})", format!("{sub:?}").to_lowercase()),
                )
            })
            .collect();
        let mut chips = chip_row(area, STRIP_TOP, &items);
        for (c, (sub, _)) in chips.iter_mut().zip(SUBS) {
            c.on = sub == self.tab;
        }
        chips
    }

    fn list_chips(&self, area: Rect) -> Vec<Chip> {
        let n = |s: &str| format!("friends.{s}");
        let items: Vec<(&'static str, u32, String)> = if self.tab == Sub::Ignored {
            vec![
                ("IGNORE", act::ADD, n("ignore")),
                ("UNIGNORE", act::REMOVE, n("unignore")),
            ]
        } else {
            vec![
                ("ADD", act::ADD, n("add")),
                ("REMOVE", act::REMOVE, n("remove")),
                ("WHISPER", act::WHISPER, n("whisper")),
                ("JOIN", act::JOIN, n("join")),
            ]
        };
        chip_row(area, BUTTONS_TOP, &items)
    }

    /// The privacy tab's radios: (action, x, bottom y, checked, label, name).
    fn privacy_radios(&self, area: Rect) -> Vec<(u32, i32, i32, bool, &'static str, String)> {
        let mut v = Vec::new();
        let x = area.x + 8;
        for (i, label) in ["Everyone", "My friends and guild", "Nobody"]
            .into_iter()
            .enumerate()
        {
            let y = area.y + 66 + 20 * i as i32;
            let on = usize::from(self.privacy.whispers) == i;
            v.push((
                act::WHISPERS + i as u32,
                x,
                y,
                on,
                label,
                format!("privacy.whispers({i})"),
            ));
        }
        for (i, label) in ["My mutual friends", "Nobody (I show as hidden)"]
            .into_iter()
            .enumerate()
        {
            let y = area.y + 150 + 20 * i as i32;
            let on = usize::from(self.privacy.show_where) == i;
            v.push((
                act::SHOW_WHERE + i as u32,
                x,
                y,
                on,
                label,
                format!("privacy.where({i})"),
            ));
        }
        v
    }

    fn list_controls(&mut self, area: Rect, v: &mut Vec<Control>) {
        if self.rows().len() > ROWS {
            let cel = |frame: u32, top: i32, id: Option<u32>| {
                let mut c = Control::new(ControlKind::Image, area.x + 298, top + 14, 12, 14)
                    .with_art(SCROLL);
                c.cel = Some(frame);
                if let Some(id) = id {
                    c.action = Action::Custom(id);
                }
                c
            };
            let top = area.y + ROWS_TOP;
            let span = ROW_H * ROWS as i32;
            let range = (self.rows().len() - ROWS) as i32;
            v.push(cel(0, top, Some(act::SCROLL_UP)));
            v.push(cel(1, top + span - 14, Some(act::SCROLL_DOWN)));
            let track = span - 14 * 3;
            v.push(cel(4, top + 14 + track * self.first as i32 / range, None));
        }
        v.push(
            Control::new(
                ControlKind::Text,
                area.x + 4,
                area.y + FIELD_BOTTOM - 4,
                FIELD_X as u16 - 6,
                20,
            )
            .with_font(FONT8, 0x20)
            .with_text("NAME", GOLD),
        );
        v.extend(field_controls(
            area.x + FIELD_X,
            area.y + FIELD_BOTTOM,
            act::FIELD,
        ));
        self.text_ix = Some(v.len() - 1);
    }

    fn privacy_controls(&self, area: Rect, v: &mut Vec<Control>) {
        let head = |y: i32, t: &str| {
            Control::new(ControlKind::Text, area.x + 4, y, 300, 20)
                .with_font(FONT16, 0x20)
                .with_text(t, GOLD)
        };
        v.push(head(area.y + 46, "WHO MAY WHISPER ME"));
        v.push(head(area.y + 130, "WHO SEES WHERE I AM"));
        for (id, x, y, on, label, _) in self.privacy_radios(area) {
            let mut r = Control::new(ControlKind::Button, x, y, 16, 16)
                .with_art(RADIO)
                .with_action(Action::Custom(id))
                .with_on_press();
            r.checked = on;
            v.push(r);
            v.push(
                Control::new(ControlKind::Text, x + 20, y + 1, 260, 20)
                    .with_font(FONT16, 0x20)
                    .with_text(label, WHITE),
            );
        }
    }

    /// The rows of the open list, the selected row's frame and the lines
    /// under the panel.
    fn list_overlay(&self, area: Rect, adv: &dyn Fn(u16, &[u16]) -> i32) -> Vec<DrawItem> {
        let w16 = width(adv, FONT16);
        let w8 = width(adv, FONT8);
        let now = self.clock.now(self.now_ms);
        let mut out = Vec::new();
        let rows = self.rows();
        let right = if rows.len() > ROWS {
            area.x + ROWS_RIGHT
        } else {
            area.x + i32::from(area.w) - 4
        };
        if rows.is_empty() {
            let loaded = if self.tab == Sub::Ignored {
                self.loaded.1
            } else {
                self.loaded.0
            };
            let t = match (loaded, self.tab) {
                (false, _) => "Getting the list...",
                (true, Sub::Ignored) => "You ignore nobody.",
                _ => "No friends yet: add one below.",
            };
            out.push(text(FONT16, area.x + 6, area.y + ROWS_TOP + 40, GREY, t));
        }
        for (i, r) in rows.iter().enumerate().skip(self.first).take(ROWS) {
            let top = self.row_top(i - self.first);
            let base = top + ROW_BASE;
            if self.selected.as_deref() == Some(address(&r.who).as_str()) {
                out.push(DrawItem::Border {
                    at: Point::new(area.x, top),
                    w: right - area.x + 2,
                    h: ROW_H,
                });
            }
            let ignored = self.tab == Sub::Ignored;
            let online = r.place != Where::Offline;
            let colour = if online && !ignored { WHITE } else { GREY };
            let tag = r.who.tag.as_ref().map(|t| format!(" <{t}>"));
            let mark = if r.mutual { "*" } else { "" };
            let tag_w = tag.as_deref().map_or(0, &w8);
            let mut x = area.x + 4;
            let name_room = COL_CLASS - 8 - tag_w - w16(mark);
            let name = fit(shown(&r.who), name_room, &w16);
            out.push(text(FONT16, x, base, colour, &name));
            x += w16(&name);
            if r.mutual {
                out.push(text(FONT16, x, base, GOLD, mark));
                x += w16(mark);
            }
            if let Some(t) = &tag {
                out.push(text(FONT8, x, base, GOLD, t));
            }
            if r.who.class != 255 {
                // Right-aligned: the level's digits never run into the place.
                let cl = format!("{} {}", class_short(r.who.class), r.who.level);
                let x = area.x + COL_PLACE - COL_GAP - w8(&cl);
                out.push(text(FONT8, x, base, GREY, &cl));
            }
            if !ignored {
                let (p, c) = place_text(r, now);
                let room = right - (area.x + COL_PLACE);
                out.push(text(
                    FONT8,
                    area.x + COL_PLACE,
                    base,
                    c,
                    &fit(&p, room, &w8),
                ));
            }
        }
        out
    }

    /// The lines under the panel: the note, else what the selected row
    /// means, else the mark's legend.
    fn under_lines(&self) -> Vec<(String, i32)> {
        let mut lines = Vec::new();
        if self.tab == Sub::Friends {
            match self.selected_row() {
                Some(r) => {
                    let name = shown(&r.who);
                    let line = match (&r.game, r.mutual) {
                        (Some(g), _) if g.joinable == 0 => {
                            format!("{name} is in {}: JOIN joins it.", g.game_name)
                        }
                        (Some(g), _) => format!("{name} is in {}.", g.game_name),
                        (None, true) => format!("{name} and you are mutual friends."),
                        (None, false) => {
                            format!("{name} hasn't added you: you see online or offline only.")
                        }
                    };
                    lines.push((line, WHITE));
                }
                None => lines.push((
                    "* a mutual friend: you see where each other is.".into(),
                    GREY,
                )),
            }
        } else if self.tab == Sub::Privacy {
            lines.push(("Replies to whispers you sent always arrive.".into(), GREY));
        }
        if let Some(n) = &self.note {
            lines.insert(0, n.clone());
        }
        lines.truncate(UNDER.len());
        lines
    }

    /// A press on a list row.
    fn click_row(&mut self, p: Point, now_ms: u64) -> Option<PanelOut> {
        let top = self.row_top(0);
        let area = self.area;
        if p.y < top || p.y >= top + ROW_H * ROWS as i32 || p.x >= area.x + ROWS_RIGHT + 2 {
            return None;
        }
        let i = self.first + ((p.y - top) / ROW_H) as usize;
        let key = address(&self.rows().get(i)?.who);
        let double = self
            .last_click
            .as_ref()
            .is_some_and(|(t, k)| *k == key && now_ms.saturating_sub(*t) <= DOUBLE_MS);
        self.last_click = Some((now_ms, key.clone()));
        self.selected = Some(key);
        self.note = None;
        if double && self.tab == Sub::Friends {
            return self.whisper();
        }
        Some(PanelOut::Rebuild)
    }
}

/// The name a whisper goes to: the character, else `*account`.
fn whisper_name(w: &Who) -> String {
    if w.char.is_empty() {
        format!("*{}", w.account)
    } else {
        w.char.clone()
    }
}

/// A typed name without the account mark, for the notes.
fn bare(n: &str) -> &str {
    n.trim_start_matches('*')
}

/// Online friends first, then by name.
fn sort_friends(rows: &mut [FriendRow]) {
    rows.sort_by_key(|r| (r.place == Where::Offline, shown(&r.who).to_lowercase()));
}

impl RightPanel for FriendsPanel {
    fn open(&mut self) {
        self.note = None;
        self.focused = false;
        self.fetch();
        if self.tab == Sub::Guild {
            self.guild.open();
        }
    }

    fn feed(&mut self, ev: &Event) {
        self.guild.feed(ev);
        match ev {
            Event::SocialState(s) => {
                self.clock.set(s.now_ms, self.now_ms);
                if self
                    .edits
                    .iter()
                    .all(|o| !matches!(o, FriendOp::Privacy(_)))
                {
                    self.privacy = Privacy {
                        whispers: s.whispers,
                        show_where: s.show_where,
                    };
                }
            }
            Event::Friends { ignored, rows } => {
                let mut rows = rows.clone();
                if *ignored {
                    rows.sort_by_key(|r| shown(&r.who).to_lowercase());
                    self.ignored = rows;
                    self.loaded.1 = true;
                } else {
                    sort_friends(&mut rows);
                    self.friends = rows;
                    self.loaded.0 = true;
                }
                self.unsupported = false;
                let max = self.rows().len().saturating_sub(ROWS);
                self.first = self.first.min(max);
            }
            Event::FriendNotice(n) => self.notice(n),
            Event::Done(d) if d.asked == Asked::FriendEdit => self.edited(None),
            Event::Refused {
                asked: Asked::FriendEdit,
                failure,
            } => self.edited(Some(failure)),
            Event::Refused {
                asked: Asked::FriendsGet,
                failure,
            } => {
                if failure.code == ErrorCode::NOT_SUPPORTED {
                    self.unsupported = true;
                } else {
                    self.note = Some((failure_text(failure), RED));
                }
            }
            _ => {}
        }
    }

    fn controls(&mut self, area: Rect, v: &mut Vec<Control>) {
        self.area = area;
        self.text_ix = None;
        if self.stale && !self.unsupported {
            self.fetch();
        }
        let mut chips = self.tab_chips(area);
        chip_controls(&chips, v);
        match self.tab {
            Sub::Guild => {
                self.guild.controls(area, v);
            }
            _ if self.unsupported => {}
            Sub::Privacy => self.privacy_controls(area, v),
            Sub::Friends | Sub::Ignored => {
                self.list_controls(area, v);
                let more = self.list_chips(area);
                chip_controls(&more, v);
                chips.extend(more);
            }
        }
        self.chips = chips;
    }

    fn overlay(
        &mut self,
        area: Rect,
        adv: &dyn Fn(u16, &[u16]) -> i32,
        now_ms: u64,
    ) -> Vec<DrawItem> {
        self.now_ms = now_ms;
        let mut out = chip_items(&self.chips, adv);
        if self.tab == Sub::Guild {
            out.extend(self.guild.overlay(area, adv, now_ms));
            return out;
        }
        if self.unsupported {
            out.push(text(
                FONT16,
                area.x + 6,
                area.y + 80,
                GREY,
                "This realm has no friends lists yet.",
            ));
            return out;
        }
        if self.tab != Sub::Privacy {
            out.extend(self.list_overlay(area, adv));
        }
        let w8 = width(adv, FONT8);
        let bottom = area.y + i32::from(area.h);
        for (dy, (t, c)) in UNDER.iter().zip(self.under_lines()) {
            let line = fit(&t, i32::from(area.w) - 8, &w8);
            out.push(text(FONT8, area.x + 4, bottom + dy, c, &line));
        }
        out
    }

    fn sync(&mut self, controls: &mut [Control], now_ms: u64) {
        self.now_ms = now_ms;
        if self.tab == Sub::Guild {
            return self.guild.sync(controls, now_ms);
        }
        if let Some(c) = self.text_ix.and_then(|i| controls.get_mut(i)) {
            show_field(c, &self.name, self.focused, now_ms, 18);
        }
    }

    fn action(&mut self, id: u32) -> Option<PanelOut> {
        if id >= act::GUILD {
            return self.guild.action(id);
        }
        match id {
            t if (act::TAB..act::TAB + 4).contains(&t) => {
                let sub = SUBS[(t - act::TAB) as usize].0;
                if sub != self.tab {
                    self.tab = sub;
                    self.selected = None;
                    self.first = 0;
                    self.focused = false;
                    self.note = None;
                    if sub == Sub::Guild {
                        self.guild.open();
                    }
                }
                Some(PanelOut::Rebuild)
            }
            act::FIELD => {
                self.focused = true;
                None
            }
            act::ADD => self.add(),
            act::REMOVE => self.remove(),
            act::WHISPER => self.whisper(),
            act::JOIN => self.join(),
            act::SCROLL_UP => {
                self.scroll(-1);
                Some(PanelOut::Rebuild)
            }
            act::SCROLL_DOWN => {
                self.scroll(1);
                Some(PanelOut::Rebuild)
            }
            w if (act::WHISPERS..act::WHISPERS + 3).contains(&w) => self.set_privacy(Privacy {
                whispers: (w - act::WHISPERS) as u8,
                ..self.privacy
            }),
            s if (act::SHOW_WHERE..act::SHOW_WHERE + 2).contains(&s) => self.set_privacy(Privacy {
                show_where: (s - act::SHOW_WHERE) as u8,
                ..self.privacy
            }),
            _ => None,
        }
    }

    /// JOIN joins the selected friend's game; Enter does too when the chat
    /// line is empty.
    fn primary(&mut self, chat_typed: bool) -> Option<PanelOut> {
        match self.tab {
            Sub::Friends if !chat_typed && self.selected.is_some() => self.join(),
            _ => None,
        }
    }

    /// Enter in the name box adds (or ignores) the name.
    fn enter(&mut self) -> Option<PanelOut> {
        match self.tab {
            Sub::Guild => self.guild.enter(),
            Sub::Friends | Sub::Ignored if self.focused => self.add(),
            _ => None,
        }
    }

    fn blur(&mut self) {
        self.focused = false;
        self.guild.blur();
    }

    fn mouse(&mut self, p: Point, now_ms: u64) -> Option<PanelOut> {
        if self.chips.iter().any(|c| c.at.contains(p)) {
            return None;
        }
        if self.tab == Sub::Guild {
            return self.guild.mouse(p, now_ms);
        }
        // A press anywhere but the box leaves it (typing goes to the chat).
        let field = Rect::new(
            self.area.x + FIELD_X,
            self.area.y + FIELD_BOTTOM - 26,
            super::widgets::FIELD_W,
            super::widgets::FIELD_H,
        );
        if !field.contains(p) && self.focused {
            self.focused = false;
        }
        match self.tab {
            Sub::Friends | Sub::Ignored => self.click_row(p, now_ms),
            _ => None,
        }
    }

    fn wheel(&mut self, delta: i32) -> Option<PanelOut> {
        if self.tab == Sub::Guild {
            return self.guild.wheel(delta);
        }
        self.scroll(-delta.signum());
        Some(PanelOut::Rebuild)
    }

    fn char(&mut self, c: char) -> bool {
        if self.tab == Sub::Guild {
            return self.guild.char(c);
        }
        if !self.focused {
            return false;
        }
        self.name.type_char(c, |c| {
            c.is_ascii_alphanumeric() || c == '*' || c == '-' || c == '_'
        });
        true
    }

    fn key(&mut self, k: u16) -> bool {
        use crate::ui::front_end::control::vk;
        const UP: u16 = 38;
        const DOWN: u16 = 40;
        if self.tab == Sub::Guild {
            return self.guild.key(k);
        }
        match k {
            vk::BACKSPACE if self.focused => self.name.backspace(),
            UP if !self.focused && self.tab != Sub::Privacy => self.step(-1),
            DOWN if !self.focused && self.tab != Sub::Privacy => self.step(1),
            _ => return false,
        }
        true
    }

    fn ui_elements(&self) -> Vec<UiElement> {
        let mut v: Vec<UiElement> = self
            .chips
            .iter()
            .map(|c| UiElement::new(format!("online_lobby.{}", c.name), c.at))
            .collect();
        let area = self.area;
        match self.tab {
            Sub::Guild => v.extend(self.guild.ui_elements()),
            _ if self.unsupported => {}
            Sub::Friends | Sub::Ignored => {
                let what = if self.tab == Sub::Ignored {
                    "ignored"
                } else {
                    "friend"
                };
                for (i, r) in self.rows().iter().enumerate().skip(self.first).take(ROWS) {
                    let top = self.row_top(i - self.first);
                    v.push(UiElement::new(
                        format!("online_lobby.{what}({})", shown(&r.who)),
                        Rect::new(area.x, top, ROWS_RIGHT as u16, ROW_H as u16),
                    ));
                }
                v.push(UiElement::new(
                    "online_lobby.friends.name",
                    Rect::new(
                        area.x + FIELD_X,
                        area.y + FIELD_BOTTOM - 26,
                        super::widgets::FIELD_W,
                        super::widgets::FIELD_H,
                    ),
                ));
            }
            Sub::Privacy => {
                for (_, x, y, _, _, name) in self.privacy_radios(area) {
                    v.push(UiElement::new(
                        format!("online_lobby.{name}"),
                        Rect::new(x, y - 16, 16, 16),
                    ));
                }
            }
        }
        v
    }
}

#[cfg(test)]
mod tests;
