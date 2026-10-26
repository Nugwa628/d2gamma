//! The create form's SERVER drop-down (owner 2026-10-24, on plan N12's
//! game-server choice): a box in the form's text-box art showing the
//! choice and its ping, with the list's down arrow; a click opens the
//! list under it (dark, gold outline): **Automatic (lowest ping)** first,
//! then each game server by its short region name with its ping; a full
//! or closing server is grey with that word and can't be chosen. A click
//! on an entry or outside, or Esc, closes it; Up / Down / Enter while it
//! is open. The data and the pings are the lobby's ([`OnlineState`]):
//! only the look is here.

use crate::ui::front_end::{Action, Control, ControlKind, DrawItem};
use crate::ui::geom::{Point, Rect};

use super::link::{ping_text, NodeState};
use super::lobby::{act, Choice, SERVER_ROWS};
use super::widgets::*;
use super::OnlineState;

/// The list's entries: a row's height, the padding above the first and
/// under the last, the text's inset from the edges, its baseline below a
/// row's top.
const ROW: i32 = 18;
const PAD: i32 = 2;
const INSET: i32 = 6;
const BASE: i32 = 14;
/// The list's width (it ends where the box ends; it is wider than the box
/// so "Automatic (lowest ping)" and a region with its ping fit).
pub const LIST_W: i32 = 260;
/// The arrow: the game list's scroll-bar down arrow, its size.
const ARROW: (i32, i32) = (12, 14);

/// One entry of the list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub choice: Choice,
    /// What the list says: "Automatic (lowest ping)", a region.
    pub text: String,
    /// On the right: the ping, or why it can't be chosen (Full, Closing).
    pub right: String,
    pub enabled: bool,
    /// Its action (`act::SERVER_AUTO`, `act::SERVER + i`, `act::SERVER_HOST`).
    pub action: u32,
}

/// The entries in the main server's order: Automatic, the game servers
/// (the first [`SERVER_ROWS`]), Host on my PC with the hosting switch on.
pub fn entries(st: &OnlineState) -> Vec<Entry> {
    let auto = st.auto_node();
    let pinged = st
        .nodes
        .iter()
        .any(|n| st.ping(&n.node).flatten().is_some());
    let mut v = vec![Entry {
        choice: Choice::Auto,
        text: "Automatic (lowest ping)".into(),
        right: match auto {
            Some(n) if pinged => ping_text(st.ping(&n.node)),
            _ => String::new(),
        },
        enabled: true,
        action: act::SERVER_AUTO,
    }];
    for (i, n) in st.nodes.iter().take(SERVER_ROWS).enumerate() {
        let right = match n.state {
            NodeState::Open => ping_text(st.ping(&n.node)),
            NodeState::Full => "Full".into(),
            NodeState::Draining => "Closing".into(),
            NodeState::Down => "Down".into(),
        };
        v.push(Entry {
            choice: Choice::Server(n.node.clone()),
            text: n.name.clone(),
            right,
            enabled: n.open(),
            action: act::SERVER + i as u32,
        });
    }
    if super::PLAYER_HOSTING {
        v.push(Entry {
            choice: Choice::Host,
            text: "Host on my PC".into(),
            right: String::new(),
            enabled: true,
            action: act::SERVER_HOST,
        });
    }
    v
}

/// What the box shows for `choice`: Automatic, or the server's region,
/// and the ping on the right.
pub fn shown(st: &OnlineState, choice: &Choice) -> (String, String) {
    let all = entries(st);
    match all.iter().find(|e| e.choice == *choice) {
        Some(e) if e.choice == Choice::Auto => ("Automatic".into(), e.right.clone()),
        Some(e) => (e.text.clone(), e.right.clone()),
        None => ("Automatic".into(), String::new()),
    }
}

/// The drop-down's state: open or not, and the entry the keys (and the
/// pointer) are on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Drop {
    pub open: bool,
    pub hot: usize,
}

impl Drop {
    /// Opens on the chosen entry.
    pub fn open(&mut self, entries: &[Entry], choice: &Choice) {
        self.open = true;
        self.hot = entries
            .iter()
            .position(|e| e.choice == *choice)
            .unwrap_or(0);
    }

    pub fn close(&mut self) {
        self.open = false;
    }

    /// Up (-1) / Down (+1): the next entry that can be chosen; it stays at
    /// the ends.
    pub fn step(&mut self, entries: &[Entry], d: i32) {
        let mut i = self.hot as i32;
        loop {
            i += d;
            match usize::try_from(i).ok().and_then(|u| entries.get(u)) {
                Some(e) if e.enabled => {
                    self.hot = i as usize;
                    return;
                }
                Some(_) => {}
                None => return,
            }
        }
    }

    /// The entry the pointer is on in the list under `boxed` (an entry that
    /// can be chosen).
    pub fn point(&mut self, entries: &[Entry], boxed: Rect, p: Point) {
        if let Some(i) = (0..entries.len()).find(|&i| row(boxed, i).contains(p)) {
            if entries[i].enabled {
                self.hot = i;
            }
        }
    }
}

/// The box: the form's field at (x, cel bottom y), its width the text
/// box art's.
pub fn box_rect(x: i32, y: i32) -> Rect {
    Rect::new(x, y - i32::from(FIELD_H), FIELD_W, FIELD_H)
}

/// The list under the box (its right edge on the box's).
fn list_rect(boxed: Rect, n: usize) -> Rect {
    let right = boxed.x + i32::from(boxed.w);
    Rect::new(
        right - LIST_W,
        boxed.y + i32::from(boxed.h),
        LIST_W as u16,
        (ROW * n as i32 + 2 * PAD) as u16,
    )
}

/// Entry `i`'s row.
fn row(boxed: Rect, i: usize) -> Rect {
    let l = list_rect(boxed, i + 1);
    Rect::new(l.x, l.y + PAD + ROW * i as i32, LIST_W as u16, ROW as u16)
}

/// The box's controls: the text-box art, the arrow, and the click area
/// that opens (and closes) the list.
pub fn box_controls(boxed: Rect, v: &mut Vec<Control>) {
    let bottom = boxed.y + i32::from(boxed.h);
    v.push(Control::new(ControlKind::Image, boxed.x, bottom, boxed.w, boxed.h).with_art(TEXTBOX));
    let mut arrow = Control::new(
        ControlKind::Image,
        boxed.x + i32::from(boxed.w) - ARROW.0 - 6,
        bottom - (i32::from(boxed.h) - ARROW.1) / 2,
        ARROW.0 as u16,
        ARROW.1 as u16,
    )
    .with_art(SCROLL);
    arrow.cel = Some(1);
    v.push(arrow);
    v.push(
        Control::new(ControlKind::Image, boxed.x, bottom, boxed.w, boxed.h)
            .with_action(Action::Custom(act::SERVER_BOX)),
    );
}

/// The open list's controls: the click outside it that closes it (and Esc),
/// then one click area per entry (a grey one's click does nothing: the
/// list stays open).
pub fn list_controls(boxed: Rect, entries: &[Entry], v: &mut Vec<Control>) {
    v.push(
        Control::new(ControlKind::Image, 0, 599, 800, 600)
            .with_action(Action::Custom(act::SERVER_CLOSE))
            .with_hotkey(crate::ui::front_end::control::vk::ESC),
    );
    for (i, e) in entries.iter().enumerate() {
        let r = row(boxed, i);
        let action = if e.enabled {
            e.action
        } else {
            act::SERVER_GREY
        };
        v.push(
            Control::new(ControlKind::Image, r.x, r.y + ROW, r.w, r.h)
                .with_action(Action::Custom(action)),
        );
    }
}

/// The box's text, measured with `width`: the choice cut to fit, its ping
/// (in the list's font) before the arrow when there is room for it.
pub fn box_items(
    boxed: Rect,
    text: &str,
    right: &str,
    width: &dyn Fn(u16, &str) -> i32,
) -> Vec<DrawItem> {
    let base = boxed.y + i32::from(boxed.h) - 7;
    let (start, end) = (boxed.x + 7, boxed.x + i32::from(boxed.w) - ARROW.0 - 10);
    let tw = width(FONT_FIELD, text);
    let rw = width(FONT16, right);
    let mut out = Vec::new();
    // The ping in the list's font, when the choice leaves room for it.
    let room = if !right.is_empty() && start + tw + 6 + rw <= end {
        out.push(item(FONT16, end - rw, base, GREY, right));
        end - rw - 6 - start
    } else {
        end - start
    };
    let shown = fit(text, room, &|s| width(FONT_FIELD, s));
    out.push(item(FONT_FIELD, start, base, WHITE, &shown));
    out
}

/// The open list: its dark box and gold outline, the entry the keys are
/// on outlined, each entry's text (gold on the hot one, grey when it
/// can't be chosen) and its ping or state on the right.
pub fn list_items(
    boxed: Rect,
    entries: &[Entry],
    drop: &Drop,
    width: &dyn Fn(u16, &str) -> i32,
) -> Vec<DrawItem> {
    let l = list_rect(boxed, entries.len());
    let at = Point::new(l.x, l.y);
    let (w, h) = (i32::from(l.w), i32::from(l.h));
    let mut out = vec![DrawItem::Rect { at, w, h }, DrawItem::Border { at, w, h }];
    for (i, e) in entries.iter().enumerate() {
        let r = row(boxed, i);
        if i == drop.hot {
            out.push(DrawItem::Border {
                at: Point::new(r.x + 2, r.y),
                w: i32::from(r.w) - 4,
                h: ROW,
            });
        }
        let colour = if !e.enabled {
            GREY
        } else if i == drop.hot {
            GOLD
        } else {
            WHITE
        };
        let base = r.y + BASE;
        let right_end = r.x + i32::from(r.w) - INSET;
        let rw = width(FONT16, &e.right);
        let room = right_end - rw - 8 - (r.x + INSET);
        let text = fit(&e.text, room, &|s| width(FONT16, s));
        out.push(item(FONT16, r.x + INSET, base, colour, &text));
        if !e.right.is_empty() {
            out.push(item(FONT16, right_end - rw, base, colour, &e.right));
        }
    }
    out
}

fn item(font: u16, x: i32, y: i32, color: i32, s: &str) -> DrawItem {
    DrawItem::Text {
        string_id: 0,
        text: s.to_string(),
        font,
        at: Point::new(x, y),
        label: None,
        color,
        boxed: None,
    }
}
