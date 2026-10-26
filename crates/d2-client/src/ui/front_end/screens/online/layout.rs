//! The lobby's movable panels (plan `lobby-ui.md` L15) on the floating
//! panels of [`crate::ui::float`]: the chat and the right panel (the
//! buttons, then the games, the form, the ladder or the friends) move by
//! their title strip and resize by their edges. Moved, a panel draws its
//! gold Celtic frame itself, and the frame painted into the lobby art at
//! its classic place is covered.
//!
//! - **Classic**: both panels where the art frames them ([`CHAT_CLASSIC`],
//!   [`RIGHT_CLASSIC`]); nothing is drawn over the art then, so the lobby
//!   looks as it always did. RESET puts both back.
//! - **Title strips**: the chat's is its tab row (a tab still takes its
//!   click), the right panel's the 20 rows over its buttons.
//! - **Sizes**: the chat down to [`CHAT_MIN`]; the right panel only grows
//!   (its list gets more rows and a wider name column).
//! - **Saved** per realm and account ([`LayoutKey`], `layouts.toml` in the
//!   config folder) when a panel lands, on LOCK and on RESET; a damaged
//!   file keeps the classic layout and says so in the chat.
//! - **Cover**: the art has no picture of the scene without its frames
//!   (the other front-end backgrounds are other places or other views), so
//!   [`Cover::Patch`] patches the painted panes with the same art: the
//!   palisade from the frameless strip between the two panes, tiled
//!   sideways, and below it the bare ground under the panes. A background
//!   named in [`Cover::Scene`] is cut at the same place instead.
//! - **16:9** ([`LobbyLayout::set_wide`]): the panels move over the wide
//!   canvas; their classic places are the chat's and the right panel at
//!   the right edge ([`super::lobby::WIDE_RIGHT`]). There is no lobby art:
//!   every panel draws its frame on a dark backing ([`backing`]) where the
//!   scene shows through faintly.

use std::path::PathBuf;

use crate::ui::chat::ChatLayout;
use crate::ui::float::celtic::Blit;
use crate::ui::float::{Canvas, FloatLayer, FloatPanel, LayoutKey, LayoutStore, MinSize, Style};
use crate::ui::front_end::{Control, DrawItem};
use crate::ui::geom::{Point, Rect};
use crate::ui::ui_ids::UiElement;

/// The panels' ids (saved in `layouts.toml`).
pub const CHAT_ID: &str = "chat";
pub const RIGHT_ID: &str = "right";

/// The chat's classic rect: its tab row on top, then the art's left frame
/// with the input box at its bottom.
pub const CHAT_CLASSIC: Rect = Rect::new(48, 158, 352, 347);
/// The right panel's classic rect: its title strip, the four buttons, the
/// art's right frame and the lines under it.
pub const RIGHT_CLASSIC: Rect = Rect::new(446, 250, 320, 290);
pub const CHAT_MIN: MinSize = MinSize::new(280, 220);
pub const RIGHT_MIN: MinSize = MinSize::new(RIGHT_CLASSIC.w, RIGHT_CLASSIC.h);

/// The frames painted into the lobby art: the chat's (its knot band on the
/// left, the input box inside) and the right pane's.
pub const CHAT_ZONE: Rect = Rect::new(48, 179, 352, 326);
pub const RIGHT_ZONE: Rect = Rect::new(446, 312, 320, 192);

/// The painted frame's place in a panel: below the chat's tab row; the
/// right pane between its buttons and the lines under it.
const CHAT_FRAME_TOP: i32 = 21;
const RIGHT_FRAME_TOP: i32 = 62;
const RIGHT_FRAME_LESS: u16 = 98;

/// The patch's sources in the lobby art: the palisade between the panes
/// (columns `STRIP_X..STRIP_X + STRIP_W`, the same rows) down to
/// [`GROUND_Y`], below it the bare ground under the panes, tiled from
/// [`GROUND_Y`].
const STRIP_X: i32 = 401;
const STRIP_W: i32 = 45;
const GROUND_Y: i32 = 398;
const GROUND: Rect = Rect::new(170, 505, 460, 95);

/// What covers a painted frame whose panel moved away.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cover {
    /// Pieces of the lobby art itself.
    Patch,
    /// The same rect of another background (800 × 600).
    Scene(&'static str),
}

/// The lobby's panels, their saved layouts and the pointer over them.
pub struct LobbyLayout {
    layer: FloatLayer,
    /// On the 16:9 canvas ([`Self::set_wide`]).
    wide: bool,
    /// The config folder the layouts are saved in (`None`: not saved).
    dir: Option<PathBuf>,
    key: Option<LayoutKey>,
    pub cover: Cover,
    /// The panel under the pointer's grab zone (its outline).
    hover: Option<&'static str>,
    /// Panels moved since the last build (the lobby builds again).
    moved: bool,
    /// The clickable rects of the last build: a press there is theirs.
    hot: Vec<Rect>,
}

impl Default for LobbyLayout {
    fn default() -> Self {
        Self::new()
    }
}

impl LobbyLayout {
    /// The classic layout, not saved until [`Self::enter`].
    pub fn new() -> Self {
        LobbyLayout {
            layer: Self::layer(false),
            wide: false,
            dir: None,
            key: None,
            cover: Cover::Patch,
            hover: None,
            moved: false,
            hot: Vec::new(),
        }
    }

    /// The panels at their classic places on the 800 × 600 or the 16:9
    /// canvas.
    fn layer(wide: bool) -> FloatLayer {
        let (canvas, right) = if wide {
            let mut r = RIGHT_CLASSIC;
            r.x += super::lobby::WIDE_RIGHT;
            (
                Canvas::new(crate::rules::camera::FrameSize::WIDE.width as u16, 600),
                r,
            )
        } else {
            (Canvas::new(800, 600), RIGHT_CLASSIC)
        };
        FloatLayer::new(
            canvas,
            // The chat on top, as the lobby always drew it last.
            vec![
                FloatPanel::new(RIGHT_ID, right, RIGHT_MIN),
                FloatPanel::new(CHAT_ID, CHAT_CLASSIC, CHAT_MIN),
            ],
        )
    }

    /// The panels on the 16:9 canvas (`true`) or the 800 × 600 one: the
    /// classic places of that canvas, and the saved layout (anchored to
    /// the canvas's edges) put back. `Some(text)` when the file could not
    /// be read.
    pub fn set_wide(&mut self, wide: bool) -> Option<String> {
        if wide == self.wide {
            return None;
        }
        self.wide = wide;
        self.layer = Self::layer(wide);
        let (dir, key) = (self.dir.take(), self.key.take());
        self.enter(dir, key)
    }

    pub fn is_wide(&self) -> bool {
        self.wide
    }

    /// A fresh lobby entry for `key`, its layouts in config folder `dir`:
    /// its saved layout, else the classic one. `Some(text)` when the file
    /// could not be read.
    pub fn enter(&mut self, dir: Option<PathBuf>, key: Option<LayoutKey>) -> Option<String> {
        self.dir = dir;
        self.key = key;
        self.hover = None;
        self.layer.reset();
        self.layer.set_locked(false);
        let mut note = None;
        if let (Some(dir), Some(key)) = (&self.dir, &self.key) {
            match LayoutStore::load(dir) {
                Ok(store) => {
                    if let Some(saved) = store.get(key) {
                        self.layer.apply(saved);
                    }
                }
                Err(e) => note = Some(format!("The panel layouts could not be read ({e}).")),
            }
        }
        self.layer.take_changed();
        note
    }

    /// Saves the layout when it changed; `Some(text)` when that failed.
    fn save(&mut self) -> Option<String> {
        if !self.layer.take_changed() {
            return None;
        }
        let (Some(dir), Some(key)) = (&self.dir, &self.key) else {
            return None;
        };
        let mut store = LayoutStore::load(dir).ok()?;
        if self.layer.is_classic() && !self.layer.locked() {
            store.remove(key);
        } else {
            store.set(key.clone(), self.layer.saved());
        }
        store
            .save(dir)
            .err()
            .map(|e| format!("The panel layout could not be saved ({e})."))
    }

    pub fn chat(&self) -> Rect {
        self.layer.rect(CHAT_ID).unwrap_or(CHAT_CLASSIC)
    }

    pub fn right(&self) -> Rect {
        self.layer.rect(RIGHT_ID).unwrap_or(RIGHT_CLASSIC)
    }

    pub fn locked(&self) -> bool {
        self.layer.locked()
    }

    pub fn is_classic(&self) -> bool {
        self.layer.is_classic()
    }

    /// A panel is being moved or resized.
    pub fn holding(&self) -> bool {
        self.layer.holding().is_some()
    }

    /// LOCK / UNLOCK; `Some(text)` when saving failed.
    pub fn toggle_lock(&mut self) -> Option<String> {
        self.layer.set_locked(!self.layer.locked());
        self.hover = None;
        self.save()
    }

    /// RESET: both panels back where the art frames them.
    pub fn reset(&mut self) -> Option<String> {
        self.layer.reset();
        self.save()
    }

    /// The chat's layout in its rect (the classic rect gives
    /// `chat_pane::LAYOUT`).
    pub fn chat_layout(&self) -> ChatLayout {
        chat_layout(self.chat())
    }

    /// How far the right panel moved from its classic place.
    pub fn right_offset(&self) -> (i32, i32) {
        let r = self.right();
        (r.x - RIGHT_CLASSIC.x, r.y - RIGHT_CLASSIC.y)
    }

    /// How much the chat's top-left moved (the form's server list in it).
    pub fn chat_offset(&self) -> (i32, i32) {
        let r = self.chat();
        (r.x - CHAT_CLASSIC.x, r.y - CHAT_CLASSIC.y)
    }

    /// How much wider and taller than classic the right panel is.
    pub fn right_grow(&self) -> (i32, i32) {
        let r = self.right();
        (
            i32::from(r.w) - i32::from(RIGHT_CLASSIC.w),
            i32::from(r.h) - i32::from(RIGHT_CLASSIC.h),
        )
    }

    /// `p` in the right panel's classic coordinates (its content is built
    /// there, then moved).
    pub fn to_right(&self, p: Point) -> Point {
        let (dx, dy) = self.right_offset();
        Point::new(p.x - dx, p.y - dy)
    }

    /// The chat is over the right panel (the last one taken is on top):
    /// it draws after it and has the clicks where they overlap.
    pub fn chat_on_top(&self) -> bool {
        self.layer.panels().last().is_some_and(|f| f.id == CHAT_ID)
    }

    /// The topmost panel under `p`.
    pub fn top_at(&self, p: Point) -> Option<&'static str> {
        self.layer
            .panels()
            .iter()
            .rev()
            .find(|f| f.rect.contains(p))
            .map(|f| f.id)
    }

    /// The clickable rects of the built controls (a press on one is the
    /// control's, not a grab).
    pub fn set_hot(&mut self, controls: &[Control]) {
        self.hot = controls
            .iter()
            .filter(|c| c.visible && c.action != crate::ui::front_end::Action::None)
            .map(Control::hit_box)
            .collect();
    }

    /// The left button went down at `p` (not on a control or a chat tab:
    /// `free`): `true` when a panel was taken.
    pub fn press(&mut self, p: Point, free: bool) -> bool {
        free && !self.hot.iter().any(|r| r.contains(p)) && self.layer.press(p)
    }

    /// The button went up: a held panel lands and the layout is saved.
    /// `Some(text)` when saving failed.
    pub fn release(&mut self, p: Point) -> Option<String> {
        if self.layer.release(p) {
            self.moved = true;
            return self.save();
        }
        None
    }

    /// The pointer moved: a held panel follows, the outline follows the
    /// grab zones.
    pub fn pointer(&mut self, p: Point) {
        let before = (self.layer.rect(CHAT_ID), self.layer.rect(RIGHT_ID));
        if self.layer.drag(p) {
            self.moved |= before != (self.layer.rect(CHAT_ID), self.layer.rect(RIGHT_ID));
        }
        let over_hot = self.hot.iter().any(|r| r.contains(p));
        self.hover = match self.layer.zone_at(p) {
            Some((id, _)) if !over_hot => Some(id),
            _ => None,
        };
    }

    /// A panel moved since the last call (the lobby builds again).
    pub fn take_moved(&mut self) -> bool {
        std::mem::take(&mut self.moved)
    }

    /// Over the lobby art, under the controls: each moved panel's painted
    /// frame covered ([`Cover`]), then each moved panel's dark pane and
    /// gold frame, in the layer's order (the last taken on top).
    /// On the 16:9 canvas every panel that doesn't float draws its frames
    /// on [`backing`] (there is no art under them); a floating one draws
    /// them in [`Self::backdrop`].
    pub fn underlay(&self, bg: &'static str) -> Vec<DrawItem> {
        if self.wide {
            // A floating panel draws its frames in its backdrop.
            let floating = self.floating();
            return self
                .layer
                .panels()
                .iter()
                .filter(|f| !floating.contains(&f.id))
                .flat_map(|f| backed_frames(f.id, f.rect, bg))
                .collect();
        }
        let mut out: Vec<DrawItem> = self
            .covers(bg)
            .into_iter()
            .map(|(file, src, at)| DrawItem::Cut { file, src, at })
            .collect();
        for f in self.layer.panels().iter().filter(|f| f.rect != f.classic) {
            out.extend(panes(f.id, f.rect, bg));
        }
        out
    }

    /// The panels from the bottom one to the top one.
    pub fn order(&self) -> Vec<&'static str> {
        self.layer.panels().iter().map(|f| f.id).collect()
    }

    /// The panels that draw over what is under them: a moved one (over
    /// the lobby's buttons and lines), and the top one where the two
    /// overlap. The bottom one first.
    pub fn floating(&self) -> Vec<&'static str> {
        let panels = self.layer.panels();
        let overlap = clip(self.chat(), self.right()).is_some();
        panels
            .iter()
            .enumerate()
            .filter(|(i, f)| f.rect != f.classic || (overlap && *i + 1 == panels.len()))
            .map(|(_, f)| f.id)
            .collect()
    }

    /// The rect of panel `id`.
    pub fn rect(&self, id: &str) -> Rect {
        if id == CHAT_ID {
            self.chat()
        } else {
            self.right()
        }
    }

    /// A floating panel's ground, drawn in the overlay before its items:
    /// the lobby art in its whole rect, as [`Self::underlay`] left it there
    /// (the moved panels' painted frames covered), then its own pane and
    /// frame when it moved. It hides what lies under the panel. On the
    /// 16:9 canvas (no art): the panels below it blacked out where it
    /// covers them, then its frames on their backing.
    pub fn backdrop(&self, id: &str, bg: &'static str) -> Vec<DrawItem> {
        let Some(f) = self.layer.panels().iter().find(|f| f.id == id) else {
            return Vec::new();
        };
        let r = f.rect;
        if self.wide {
            // The panels below it hidden where it lies over them, then its
            // own frames on their backing.
            let mut out: Vec<DrawItem> = self
                .layer
                .panels()
                .iter()
                .take_while(|g| g.id != id)
                .filter_map(|g| clip(g.rect, r))
                .map(|c| DrawItem::Rect {
                    at: c.origin(),
                    w: i32::from(c.w),
                    h: i32::from(c.h),
                })
                .collect();
            out.extend(backed_frames(f.id, r, bg));
            return out;
        }
        let mut out = vec![DrawItem::Cut {
            file: bg,
            src: r,
            at: r.origin(),
        }];
        for (file, src, at) in self.covers(bg) {
            let piece = Rect::new(at.x, at.y, src.w, src.h);
            if let Some(c) = clip(piece, r) {
                let (dx, dy) = (c.x - at.x, c.y - at.y);
                out.push(DrawItem::Cut {
                    file,
                    src: Rect::new(src.x + dx, src.y + dy, c.w, c.h),
                    at: c.origin(),
                });
            }
        }
        if f.rect != f.classic {
            out.extend(panes(f.id, r, bg));
        }
        out
    }

    /// The cuts over the moved panels' painted frames ([`Cover`]): file,
    /// source rect and place.
    fn covers(&self, bg: &'static str) -> Vec<(&'static str, Rect, Point)> {
        let mut out = Vec::new();
        for f in self.layer.panels().iter().filter(|f| f.rect != f.classic) {
            let zone = if f.id == CHAT_ID {
                CHAT_ZONE
            } else {
                RIGHT_ZONE
            };
            match self.cover {
                Cover::Patch => {
                    out.extend(cover_blits(zone).into_iter().map(|b| (bg, b.src, b.at)))
                }
                Cover::Scene(file) => out.push((file, zone, zone.origin())),
            }
        }
        out
    }

    /// Over everything: the outline of the panel being moved or under a
    /// grab zone (unlocked only).
    pub fn outline(&self) -> Vec<DrawItem> {
        let id = self.layer.holding().or(self.hover);
        id.and_then(|id| self.layer.rect(id))
            .map(|r| DrawItem::Border {
                at: r.origin(),
                w: i32::from(r.w),
                h: i32::from(r.h),
            })
            .into_iter()
            .collect()
    }
}

/// A moved panel's dark pane and gold frame at `r`.
fn panes(id: &str, r: Rect, bg: &'static str) -> Vec<DrawItem> {
    let mut out = Vec::new();
    for (rect, style) in panel_frames(id, r) {
        out.push(DrawItem::Rect {
            at: rect.origin(),
            w: i32::from(rect.w),
            h: i32::from(rect.h),
        });
        out.push(DrawItem::Frame {
            file: bg,
            rect,
            style,
        });
    }
    out
}

/// Where `a` and `b` overlap.
pub fn clip(a: Rect, b: Rect) -> Option<Rect> {
    let x = a.x.max(b.x);
    let y = a.y.max(b.y);
    let right = a.right().min(b.right());
    let bottom = a.bottom().min(b.bottom());
    (right > i64::from(x) && bottom > i64::from(y)).then(|| {
        Rect::new(
            x,
            y,
            (right - i64::from(x)) as u16,
            (bottom - i64::from(y)) as u16,
        )
    })
}

/// A dark backing over `r` that lets the scene under it show through
/// faintly: black blended twice, at 75 % then 50 % (draw modes 2 and 1),
/// about 87 % dark.
pub fn backing(r: Rect) -> Vec<DrawItem> {
    [2, 1]
        .map(|mode| DrawItem::Shade {
            at: r.origin(),
            w: i32::from(r.w),
            h: i32::from(r.h),
            color: 0,
            mode,
        })
        .to_vec()
}

/// A panel's gold frames at `r`, each on [`backing`] (16:9: no art under
/// them).
fn backed_frames(id: &str, r: Rect, bg: &'static str) -> Vec<DrawItem> {
    let mut out = Vec::new();
    for (rect, style) in panel_frames(id, r) {
        out.extend(backing(rect));
        out.push(DrawItem::Frame {
            file: bg,
            rect,
            style,
        });
    }
    out
}

/// The chat's layout in panel rect `r`: the tabs on its top row, the lines
/// and the column inside the frame, the input line in the box at its
/// bottom.
pub fn chat_layout(r: Rect) -> ChatLayout {
    let bottom = r.y + i32::from(r.h);
    ChatLayout {
        tabs: Rect::new(r.x + 2, r.y + 2, r.w - 2, 18),
        body: Rect::new(r.x + 20, r.y + 28, r.w - 26, r.h - 83),
        column_w: 112,
        input: Rect::new(r.x + 74, bottom - 25, r.w - 82, 20),
    }
}

/// The frames a moved panel draws: the chat's knot frame and its input
/// box; the right pane's rope.
pub fn panel_frames(id: &str, r: Rect) -> Vec<(Rect, Style)> {
    if id == CHAT_ID {
        let input = chat_layout(r).input;
        vec![
            (
                Rect::new(r.x, r.y + CHAT_FRAME_TOP, r.w, r.h - CHAT_FRAME_TOP as u16),
                Style::Knot,
            ),
            (
                Rect::new(input.x - 4, input.y - 3, input.w + 10, 25),
                Style::Rope,
            ),
        ]
    } else {
        vec![(
            Rect::new(r.x, r.y + RIGHT_FRAME_TOP, r.w, r.h - RIGHT_FRAME_LESS),
            Style::Rope,
        )]
    }
}

/// [`Cover::Patch`] of `zone`: above [`GROUND_Y`] the palisade strip
/// tiled sideways at the same rows (a tile starts every [`STRIP_W`]
/// columns from [`STRIP_X`]), below it the ground tiled from the zone's
/// left and from [`GROUND_Y`]. The copies cover the zone exactly.
pub fn cover_blits(zone: Rect) -> Vec<Blit> {
    let (left, top) = (zone.x, zone.y);
    let right = zone.x + i32::from(zone.w);
    let bottom = zone.y + i32::from(zone.h);
    let mut out = Vec::new();
    let split = GROUND_Y.clamp(top, bottom);
    if split > top {
        let mut x = left;
        while x < right {
            let start = STRIP_X + (x - STRIP_X).div_euclid(STRIP_W) * STRIP_W;
            let end = (start + STRIP_W).min(right);
            out.push(Blit {
                src: Rect::new(
                    STRIP_X + x - start,
                    top,
                    (end - x) as u16,
                    (split - top) as u16,
                ),
                at: Point::new(x, top),
            });
            x = end;
        }
    }
    let gw = i32::from(GROUND.w);
    let gh = i32::from(GROUND.h);
    let mut y = split;
    while y < bottom {
        let h = (gh - (y - GROUND_Y) % gh).min(bottom - y);
        let sy = GROUND.y + (y - GROUND_Y) % gh;
        let mut x = left;
        while x < right {
            let w = (gw - (x - left) % gw).min(right - x);
            let sx = GROUND.x + (x - left) % gw;
            out.push(Blit {
                src: Rect::new(sx, sy, w as u16, h as u16),
                at: Point::new(x, y),
            });
            x += w;
        }
        y += h;
    }
    out
}

/// Moves controls by (`dx`, `dy`).
pub fn shift_controls(controls: &mut [Control], (dx, dy): (i32, i32)) {
    for c in controls {
        c.x += dx;
        c.y += dy;
    }
}

/// Moves draw items by (`dx`, `dy`).
pub fn shift_items(items: &mut [DrawItem], (dx, dy): (i32, i32)) {
    for it in items {
        match it {
            DrawItem::Art { at, .. }
            | DrawItem::Text { at, .. }
            | DrawItem::Blend { at, .. }
            | DrawItem::Doll { at, .. }
            | DrawItem::UiCel { at, .. }
            | DrawItem::Shade { at, .. }
            | DrawItem::Strip { at, .. }
            | DrawItem::Rect { at, .. }
            | DrawItem::Border { at, .. }
            | DrawItem::Pill { at, .. }
            | DrawItem::Cut { at, .. } => {
                at.x += dx;
                at.y += dy;
            }
            DrawItem::Frame { rect, .. } => {
                rect.x += dx;
                rect.y += dy;
            }
            DrawItem::Dim { .. } | DrawItem::Raised(_) => {}
        }
    }
}

/// Moves UI elements by (`dx`, `dy`).
pub fn shift_elements(elements: &mut [UiElement], (dx, dy): (i32, i32)) {
    let by = |p: &mut Point| {
        p.x += dx;
        p.y += dy;
    };
    for e in elements {
        e.rect.x += dx;
        e.rect.y += dy;
        by(&mut e.at);
        for p in e.ends.iter_mut().flatten() {
            by(p);
        }
    }
}

#[cfg(test)]
mod tests;
