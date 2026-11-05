//! The state icon row of a mod game whose `states.toml` gives states an
//! icon (`d2_sim::rules::StateDef::icon`): one small icon per such state
//! the local player has, left to right in file order, just above the left
//! orb, with the state's stack count in the icon's bottom-right corner
//! when it is more than one.
//!
//! The panel is added to the root only when the game's rules hold state
//! rules, so D2Gamma's own UI has the same panels as without mods. It
//! takes no input and is never hit. The stack count comes from the server
//! (`ClientUnit::state_stacks`, S→C 0xB7 op 3); a state with no count has
//! one stack.
//!
//! # Icon art
//!
//! The icon name `smolder` is the file `files/icons/smolder.png` of the mod
//! (the last mod of the stack that has it): an 8-bit greyscale or indexed
//! PNG of up to 32 × 32 pixels whose values are palette indices, 0 being
//! transparent, like D2Gamma's own UI cels ([`super::own_art`]). It is
//! drawn as runs of the game's palette colours. A state whose icon file is
//! missing or unusable gets a plain square in a colour taken from the
//! name, with the name's first letter, so a mod works before it has art.

use std::path::{Path, PathBuf};

use d2_sim::rules::StateDef;

use super::draw::{RectRequest, TextRequest, TextStyle, UiDraw, UiDrawSink};
use super::geom::{Point, Rect, FRAME};
use super::panel::{Panel, PanelId, UiCtx, UiEvent, UiResponse, WidgetId};
use super::text::TextOpts;

/// The panel's id: not a UI state.
pub const STATE_ICONS_PANEL: PanelId = PanelId(0x118);

/// Where the icon row starts: the left edge, its bottom just above the
/// life orb.
const ROW_LEFT: i32 = 6;
const ROW_BOTTOM: i32 = 484;
/// Space between two icons.
const GAP: i32 = 3;
/// Side of the fallback square.
const FALLBACK_SIDE: i32 = 20;
/// Longest side of an icon file.
pub const MAX_SIDE: u32 = 32;
/// The font of the stack count and of the fallback letter.
const FONT: u16 = 1;
/// Text colour white.
const WHITE: u16 = 0;

/// An icon's pixels: palette indices, 0 transparent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IconArt {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

/// One state with an icon.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Entry {
    state: u16,
    name: String,
    art: Option<IconArt>,
}

/// Draws the local player's state icons and stack counts.
pub struct StateIconsUi {
    entries: Vec<Entry>,
}

impl StateIconsUi {
    /// The panel for `defs` (those with an `icon`), reading the icon files
    /// from the `files/icons` folder of each of `layers` (mod folders,
    /// bases first).
    pub fn new(defs: &[StateDef], layers: &[PathBuf]) -> StateIconsUi {
        let entries = defs
            .iter()
            .filter_map(|d| {
                let name = d.icon.clone()?;
                Some(Entry {
                    state: d.id,
                    art: load_icon(layers, &name),
                    name,
                })
            })
            .collect();
        StateIconsUi { entries }
    }

    /// The icons drawn for a unit that has the states `has` says, with the
    /// stack counts `stacks` gives.
    fn draw_row(
        &self,
        has: impl Fn(u16) -> bool,
        stacks: impl Fn(u16) -> u16,
        out: &mut dyn UiDrawSink,
    ) {
        let mut x = ROW_LEFT;
        for e in self.entries.iter().filter(|e| has(e.state)) {
            let (w, h) = match &e.art {
                Some(a) => (a.width as i32, a.height as i32),
                None => (FALLBACK_SIDE, FALLBACK_SIDE),
            };
            let top = ROW_BOTTOM - h;
            match &e.art {
                Some(a) => draw_art(a, Point::new(x, top), out),
                None => {
                    let color = fallback_color(&e.name);
                    out.push(UiDraw::Rect(RectRequest::sized(x, top, w, h, color, 0)));
                    let letter: String = e
                        .name
                        .chars()
                        .take(1)
                        .flat_map(char::to_uppercase)
                        .collect();
                    out.push(text(&letter, Point::new(x + 6, top + h - 4)));
                }
            }
            let count = stacks(e.state);
            if count > 1 {
                let s = count.to_string();
                // Right-aligned to the icon's corner at about 6 px a digit.
                let at = Point::new(x + w - 6 * s.len() as i32, ROW_BOTTOM - 1);
                out.push(text(&s, at));
            }
            x += w + GAP;
        }
    }
}

/// The icon `name` from the last layer that has a usable file for it.
pub fn load_icon(layers: &[PathBuf], name: &str) -> Option<IconArt> {
    if name.contains(['/', '\\', ':']) || name.starts_with('.') {
        return None;
    }
    layers
        .iter()
        .rev()
        .filter_map(|l| icon_file(&l.join("files").join("icons").join(format!("{name}.png"))))
        .next()
}

fn icon_file(path: &Path) -> Option<IconArt> {
    decode_icon(&std::fs::read(path).ok()?)
}

/// An 8-bit greyscale or indexed PNG of at most [`MAX_SIDE`] square, as its
/// raw values.
pub fn decode_icon(bytes: &[u8]) -> Option<IconArt> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::IDENTITY);
    let mut reader = decoder.read_info().ok()?;
    let info = reader.info();
    let kind = matches!(
        info.color_type,
        png::ColorType::Grayscale | png::ColorType::Indexed
    );
    let (width, height) = (info.width, info.height);
    if !kind
        || info.bit_depth != png::BitDepth::Eight
        || width == 0
        || height == 0
        || width > MAX_SIDE
        || height > MAX_SIDE
    {
        return None;
    }
    let mut buf = vec![0; reader.output_buffer_size()?];
    let line = reader.next_frame(&mut buf).ok()?.line_size;
    let pixels = (0..height as usize)
        .flat_map(|y| &buf[y * line..y * line + width as usize])
        .copied()
        .collect();
    Some(IconArt {
        width,
        height,
        pixels,
    })
}

/// A palette index for the fallback square, from the name: the same name is
/// always the same colour. Kept off index 0 (transparent) and the darkest
/// entries.
fn fallback_color(name: &str) -> u8 {
    let h = name
        .bytes()
        .fold(7u32, |h, b| h.wrapping_mul(31).wrapping_add(u32::from(b)));
    32 + (h % 200) as u8
}

fn text(s: &str, at: Point) -> UiDraw {
    UiDraw::Text(TextRequest {
        text: s.encode_utf16().collect(),
        at,
        style: TextStyle {
            font: FONT,
            color: WHITE,
        },
        opts: TextOpts::default(),
        clip: FRAME,
    })
}

/// `art` with its top-left at `at`, as one rectangle per run of equal
/// opaque values in a row.
fn draw_art(art: &IconArt, at: Point, out: &mut dyn UiDrawSink) {
    let w = art.width as usize;
    for (y, row) in art.pixels.chunks(w).enumerate() {
        let mut x = 0;
        while x < w {
            let v = row[x];
            let mut end = x + 1;
            while end < w && row[end] == v {
                end += 1;
            }
            if v != 0 {
                out.push(UiDraw::Rect(RectRequest::sized(
                    at.x + x as i32,
                    at.y + y as i32,
                    (end - x) as i32,
                    1,
                    v,
                    0,
                )));
            }
            x = end;
        }
    }
}

impl Panel for StateIconsUi {
    fn id(&self) -> PanelId {
        STATE_ICONS_PANEL
    }

    /// Empty: the panel is never under the pointer.
    fn rect(&self) -> Rect {
        Rect::default()
    }

    fn draw(&self, ctx: &UiCtx, out: &mut dyn UiDrawSink) {
        let Some(unit) = ctx.world.local() else {
            return;
        };
        self.draw_row(
            |s| unit.states.contains(&s),
            |s| unit.state_stacks.get(&s).copied().unwrap_or(1),
            out,
        );
    }

    fn hit(&self, _p: Point) -> Option<WidgetId> {
        None
    }

    fn event(&mut self, _e: UiEvent, _ctx: &UiCtx) -> UiResponse {
        UiResponse::Ignored
    }
}

#[cfg(test)]
#[path = "state_icons_tests.rs"]
mod tests;
