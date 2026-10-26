// D2RUST's own: the settings screens' widgets.
//! The widget kit of the settings menu and the Ctrl+O overlay: dark panels
//! with gold accents, rows, sliders, switches, segmented choices,
//! drop-downs, tabs, key caps and buttons. Each widget is a few UI nodes
//! placed absolutely, in view units, inside a layer that covers the
//! 1280 x 720 view ([`Layer`]); the nodes are drawn at the window's own
//! resolution with smooth edges and rounded corners.
//!
//! Widgets that take input carry a [`HitTag`] (and a `Button`, so the UI
//! reports hover and press); the screens map those onto
//! [`super::state::Hit`]s.

use bevy::prelude::*;
use bevy::ui::{FocusPolicy, RelativeCursorPosition};

use super::state::Hit;
use super::text::{TextCtx, TextRole};
use super::Metrics;

/// The palette of the settings screens (sRGB).
pub mod colour {
    pub const GOLD: [u8; 3] = [212, 176, 106];
    pub const LIGHT: [u8; 3] = [225, 216, 198];
    pub const DIM: [u8; 3] = [160, 150, 132];
    pub const MUTED: [u8; 3] = [139, 122, 87];
    pub const WHITE: [u8; 3] = [240, 228, 200];
    pub const GREEN: [u8; 3] = [150, 220, 130];
    /// A row or value that can't be changed now.
    pub const GREYED: [u8; 3] = [96, 89, 78];
    /// The search box's placeholder.
    pub const HINT: [u8; 3] = [138, 128, 112];
    /// The words on the stone-coloured buttons.
    pub const ENGRAVED: [u8; 3] = [52, 36, 16];

    pub const PANEL: [u8; 3] = [14, 12, 10];
    pub const PANEL_EDGE: [u8; 3] = [107, 87, 51];
    pub const BAR: [u8; 3] = [27, 22, 16];
    pub const BAR_FOOT: [u8; 3] = [18, 15, 12];
    pub const RULE: [u8; 3] = [58, 49, 34];
    pub const DIVIDER: [u8; 3] = [44, 37, 26];
    pub const FIELD: [u8; 3] = [15, 13, 10];
    pub const FIELD_EDGE: [u8; 3] = [74, 62, 42];
    pub const SEARCH: [u8; 3] = [13, 11, 9];
    pub const PICKED: [u8; 3] = [107, 85, 48];
    pub const RAIL_ON: [u8; 3] = [52, 42, 26];
    pub const ROW_ON: [u8; 3] = [40, 33, 22];
    pub const ROW_ON_EDGE: [u8; 3] = [120, 100, 60];
    pub const ROW_HOVER: [u8; 3] = [30, 25, 18];
    pub const TRACK: [u8; 3] = [44, 37, 26];
    pub const TRACK_FILL: [u8; 3] = [181, 143, 74];
    pub const KNOB: [u8; 3] = [240, 220, 174];
    pub const KNOB_OFF: [u8; 3] = [125, 114, 98];
    pub const STONE: [u8; 3] = [150, 140, 124];
    pub const STONE_EDGE: [u8; 3] = [190, 178, 160];
    pub const STONE_GOLD: [u8; 3] = [196, 168, 112];
    pub const STONE_GOLD_EDGE: [u8; 3] = [232, 204, 140];
    pub const TAB_ON: [u8; 3] = [48, 40, 26];
}

/// An sRGB colour with alpha 0..=255.
pub fn rgba(c: [u8; 3], a: u8) -> Color {
    Color::srgba_u8(c[0], c[1], c[2], a)
}

pub fn rgb(c: [u8; 3]) -> Color {
    rgba(c, 255)
}

/// Which screen a widget belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    Menu,
    Overlay,
}

/// What a widget node stands for when pressed or hovered, and on which
/// screen.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct HitTag {
    pub hit: Hit,
    pub screen: Screen,
}

/// How a text sits on its point.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
    Left,
    Centre,
    Right,
}

/// The nodes of a screen, placed in view units from the view's top-left.
pub struct Layer<'a, 'w, 's> {
    pub p: &'a mut ChildSpawnerCommands<'w>,
    pub text: TextCtx<'s>,
    pub screen: Screen,
}

/// A box: `(x0, y0, x1, y1)` in view units.
pub type Rect = (f32, f32, f32, f32);

impl Layer<'_, '_, '_> {
    fn m(&self) -> Metrics {
        self.text.metrics
    }

    /// The tag of `hit` on this layer's screen.
    pub fn tag(&self, hit: Hit) -> HitTag {
        HitTag {
            hit,
            screen: self.screen,
        }
    }

    fn node(&self, r: Rect) -> Node {
        let m = self.m();
        Node {
            position_type: PositionType::Absolute,
            left: m.px(r.0),
            top: m.px(r.1),
            width: m.px(r.2 - r.0),
            height: m.px(r.3 - r.1),
            ..default()
        }
    }

    /// A filled box with an optional outline and corner radius.
    pub fn rect(
        &mut self,
        r: Rect,
        fill: Option<Color>,
        edge: Option<Color>,
        radius: f32,
    ) -> EntityCommands<'_> {
        let m = self.m();
        let mut node = self.node(r);
        node.border_radius = BorderRadius::all(m.px(radius));
        if edge.is_some() {
            // One output pixel per view unit, at least one.
            node.border = UiRect::all(Val::Px(m.logical(1.0).max(1.0 / m.scale_factor)));
        }
        let mut e = self.p.spawn(node);
        if let Some(f) = fill {
            e.insert(BackgroundColor(f));
        }
        if let Some(c) = edge {
            e.insert(BorderColor::all(c));
        }
        e
    }

    /// A box that takes the pointer as `hit`.
    pub fn hit(&mut self, r: Rect, hit: Hit) -> EntityCommands<'_> {
        let node = self.node(r);
        let tag = self.tag(hit);
        self.p.spawn((node, Button, FocusPolicy::Block, tag))
    }

    /// `s` in `role` and `colour`, its line's middle on `y`, starting at,
    /// centred on or ending at `x`.
    pub fn text(&mut self, role: TextRole, s: &str, colour: [u8; 3], x: f32, y: f32, align: Align) {
        let (left, justify) = match align {
            Align::Left => (x, JustifyContent::FlexStart),
            Align::Centre => (x - 400.0, JustifyContent::Center),
            Align::Right => (x - 800.0, JustifyContent::FlexEnd),
        };
        let width = match align {
            Align::Left => 800.0,
            Align::Centre => 800.0,
            Align::Right => 800.0,
        };
        let mut node = self.node((left, y - 20.0, left + width, y + 20.0));
        node.justify_content = justify;
        node.align_items = AlignItems::Center;
        node.overflow = Overflow::visible();
        let text = &mut self.text;
        self.p.spawn(node).with_children(|c| {
            text.spawn(c, role, s, colour);
        });
    }

    /// `s` in `role` wrapped to `width` units, first line's middle on `y`,
    /// lines `pitch` apart; returns the middle of the line after the last.
    #[allow(clippy::too_many_arguments)]
    pub fn wrapped(
        &mut self,
        role: TextRole,
        s: &str,
        colour: [u8; 3],
        x: f32,
        y: f32,
        width: f32,
        pitch: f32,
    ) -> f32 {
        let mut y = y;
        for line in wrap(s, width, |t| self.text.measure(role, t)) {
            self.text(role, &line, colour, x, y, Align::Left);
            y += pitch;
        }
        y
    }

    /// The width of `s` in `role`, view units.
    pub fn measure(&self, role: TextRole, s: &str) -> f32 {
        self.text.measure(role, s)
    }

    /// A horizontal rule one unit high.
    pub fn hline(&mut self, x0: f32, x1: f32, y: f32, c: [u8; 3]) {
        self.rect((x0, y, x1, y + 1.0), Some(rgb(c)), None, 0.0);
    }

    /// A vertical rule one unit wide.
    pub fn vline(&mut self, x: f32, y0: f32, y1: f32, c: [u8; 3]) {
        self.rect((x, y0, x + 1.0, y1), Some(rgb(c)), None, 0.0);
    }

    /// The dark panel with its gold edge.
    pub fn panel(&mut self, r: Rect, alpha: u8) {
        let tag = self.tag(Hit::Panel);
        self.rect(
            r,
            Some(rgba(colour::PANEL, alpha)),
            Some(rgb(colour::PANEL_EDGE)),
            10.0,
        )
        .insert((Button, FocusPolicy::Block, tag));
    }

    /// A row's band: the selected row has a gold-edged band, a hovered one
    /// a faint one. It takes the pointer as the row.
    pub fn row_band(&mut self, r: Rect, row: usize, selected: bool, hovered: bool) {
        let (fill, edge) = if selected {
            (Some(rgb(colour::ROW_ON)), Some(rgb(colour::ROW_ON_EDGE)))
        } else if hovered {
            (Some(rgb(colour::ROW_HOVER)), None)
        } else {
            (None, None)
        };
        let tag = self.tag(Hit::Row(row));
        self.rect(r, fill, edge, 6.0)
            .insert((Button, FocusPolicy::Block, tag));
    }

    /// A slider from `x0` to `x1` on line `y` with its knob at `f` (0..1)
    /// and its value written right of it, ending at `value_x`.
    #[allow(clippy::too_many_arguments)]
    pub fn slider(
        &mut self,
        row: usize,
        x0: f32,
        x1: f32,
        y: f32,
        f: f32,
        value: &str,
        value_x: f32,
        on: bool,
    ) {
        let f = f.clamp(0.0, 1.0);
        let xm = x0 + (x1 - x0) * f;
        self.rect(
            (x0, y - 2.0, x1, y + 2.0),
            Some(rgb(colour::TRACK)),
            None,
            2.0,
        );
        if on {
            self.rect(
                (x0, y - 2.0, xm, y + 2.0),
                Some(rgb(colour::TRACK_FILL)),
                None,
                2.0,
            );
            self.rect(
                (xm - 10.0, y - 10.0, xm + 10.0, y + 10.0),
                Some(rgba(colour::GOLD, 64)),
                None,
                10.0,
            );
        }
        let knob = if on { colour::KNOB } else { colour::KNOB_OFF };
        self.rect(
            (xm - 7.0, y - 7.0, xm + 7.0, y + 7.0),
            Some(rgb(knob)),
            None,
            7.0,
        );
        // The track's hit box is the whole height of the row, a little
        // beyond its ends, so the knob is easy to catch.
        self.hit(
            (x0 - 8.0, y - 14.0, x1 + 8.0, y + 14.0),
            Hit::Slider(row, f),
        )
        .insert(RelativeCursorPosition::default())
        .insert(SliderTrack {
            row,
            x0: x0 - 8.0,
            x1: x1 + 8.0,
            start: x0,
            end: x1,
        });
        let c = if on { colour::LIGHT } else { colour::GREYED };
        self.text(TextRole::Label, value, c, value_x, y, Align::Right);
    }

    /// A drop-down box from `x0` to `x1` on line `y` showing `value`.
    pub fn dropdown(&mut self, x0: f32, x1: f32, y: f32, value: &str, hit: Hit, on: bool) {
        let tag = self.tag(hit);
        self.rect(
            (x0, y - 14.0, x1, y + 14.0),
            Some(rgb(colour::FIELD)),
            Some(rgb(colour::FIELD_EDGE)),
            5.0,
        )
        .insert((Button, FocusPolicy::Block, tag));
        let c = if on { colour::LIGHT } else { colour::GREYED };
        self.text(TextRole::Label, value, c, x0 + 10.0, y, Align::Left);
        self.caret(x1 - 14.0, y, c);
    }

    /// A small down-pointing caret centred on `x`, `y`.
    pub fn caret(&mut self, x: f32, y: f32, c: [u8; 3]) {
        // Three rows of shrinking width make the triangle.
        for (k, w) in [8.0f32, 5.0, 2.0].into_iter().enumerate() {
            let top = y - 3.0 + k as f32 * 1.7;
            self.rect(
                (x - w / 2.0, top, x + w / 2.0, top + 1.8),
                Some(rgb(c)),
                None,
                0.0,
            );
        }
    }

    /// Segmented values from `x0` to `x1` on line `y`, value `picked` on.
    #[allow(clippy::too_many_arguments)]
    pub fn segmented(
        &mut self,
        row: usize,
        x0: f32,
        x1: f32,
        y: f32,
        names: &[&str],
        picked: i64,
        on: bool,
    ) {
        self.rect(
            (x0, y - 13.0, x1, y + 13.0),
            Some(rgb(colour::FIELD)),
            Some(rgb(colour::FIELD_EDGE)),
            5.0,
        );
        let w = (x1 - x0) / names.len().max(1) as f32;
        for (i, s) in names.iter().enumerate() {
            let a = x0 + i as f32 * w;
            let is = i as i64 == picked;
            if is {
                self.rect(
                    (a + 1.0, y - 12.0, a + w - 1.0, y + 12.0),
                    Some(rgb(if on { colour::PICKED } else { colour::DIVIDER })),
                    None,
                    4.0,
                );
            } else if i > 0 && i as i64 != picked + 1 {
                self.vline(a, y - 12.0, y + 12.0, colour::DIVIDER);
            }
            self.hit((a, y - 13.0, a + w, y + 13.0), Hit::Segment(row, i as i64));
            let c = match (on, is) {
                (false, _) => colour::GREYED,
                (true, true) => colour::WHITE,
                (true, false) => colour::DIM,
            };
            self.text(TextRole::Label, s, c, a + w / 2.0, y, Align::Centre);
        }
    }

    /// A switch ending at `x1` on line `y`.
    pub fn toggle(&mut self, row: usize, x1: f32, y: f32, value: bool, on: bool) {
        let tag = self.tag(Hit::Toggle(row));
        let fill = if value && on {
            colour::PICKED
        } else {
            colour::TRACK
        };
        self.rect(
            (x1 - 38.0, y - 10.0, x1, y + 10.0),
            Some(rgb(fill)),
            Some(rgb(colour::FIELD_EDGE)),
            10.0,
        )
        .insert((Button, FocusPolicy::Block, tag));
        let cx = if value { x1 - 10.0 } else { x1 - 28.0 };
        let knob = if value && on {
            colour::KNOB
        } else {
            colour::KNOB_OFF
        };
        self.rect(
            (cx - 7.0, y - 7.0, cx + 7.0, y + 7.0),
            Some(rgb(knob)),
            None,
            7.0,
        );
    }

    /// A key cap at `x` on line `y`; returns its width.
    pub fn keycap(&mut self, x: f32, y: f32, s: &str) -> f32 {
        let w = self.measure(TextRole::Label, s) + 10.0;
        self.rect(
            (x, y - 9.0, x + w, y + 9.0),
            None,
            Some(rgb(colour::FIELD_EDGE)),
            3.0,
        );
        self.text(TextRole::Label, s, colour::LIGHT, x + 5.0, y, Align::Left);
        w
    }

    /// A stone-coloured button (gold for the main one) with engraved words.
    pub fn button(&mut self, r: Rect, s: &str, main: bool, hit: Hit) {
        let (fill, edge) = if main {
            (colour::STONE_GOLD, colour::STONE_GOLD_EDGE)
        } else {
            (colour::STONE, colour::STONE_EDGE)
        };
        let tag = self.tag(hit);
        self.rect(r, Some(rgb(fill)), Some(rgb(edge)), 6.0).insert((
            Button,
            FocusPolicy::Block,
            tag,
        ));
        self.text(
            TextRole::Button,
            s,
            colour::ENGRAVED,
            (r.0 + r.2) / 2.0,
            (r.1 + r.3) / 2.0,
            Align::Centre,
        );
    }

    /// A tab from `x0` to `x1` with its top at `y0`: the picked one has a
    /// gold underline.
    pub fn tab(&mut self, x0: f32, x1: f32, y0: f32, s: &str, picked: bool, hit: Hit) {
        if picked {
            self.rect(
                (x0, y0, x1, y0 + 24.0),
                Some(rgb(colour::TAB_ON)),
                None,
                0.0,
            );
            self.rect(
                (x0, y0 + 22.0, x1, y0 + 24.0),
                Some(rgb(colour::GOLD)),
                None,
                0.0,
            );
        }
        self.hit((x0, y0, x1, y0 + 24.0), hit);
        let c = if picked { colour::WHITE } else { colour::DIM };
        self.text(
            TextRole::Label,
            s,
            c,
            (x0 + x1) / 2.0,
            y0 + 12.0,
            Align::Centre,
        );
    }

    /// An open list under (or over) a box: `items` (text, enabled, current)
    /// from `x0` to `x1`, its first item's top at `top`, each `pitch` high,
    /// item `k` hitting `hit(k)`.
    pub fn list(
        &mut self,
        x0: f32,
        x1: f32,
        top: f32,
        pitch: f32,
        items: &[(String, bool, bool)],
        hit: impl Fn(usize) -> Hit,
    ) {
        let bottom = top + pitch * items.len() as f32;
        let tag = self.tag(Hit::Panel);
        self.rect(
            (x0, top - 3.0, x1, bottom + 3.0),
            Some(rgb(colour::FIELD)),
            Some(rgb(colour::PANEL_EDGE)),
            5.0,
        )
        .insert((Button, FocusPolicy::Block, tag));
        for (k, (s, on, current)) in items.iter().enumerate() {
            let y0 = top + pitch * k as f32;
            if *current {
                self.rect(
                    (x0 + 2.0, y0, x1 - 2.0, y0 + pitch),
                    Some(rgb(colour::RAIL_ON)),
                    None,
                    4.0,
                );
            }
            self.hit((x0, y0, x1, y0 + pitch), hit(k));
            let c = match (on, current) {
                (false, _) => colour::GREYED,
                (true, true) => colour::WHITE,
                (true, false) => colour::LIGHT,
            };
            self.text(
                TextRole::Label,
                s,
                c,
                x0 + 10.0,
                y0 + pitch / 2.0,
                Align::Left,
            );
        }
    }
}

/// A slider's track: its row and where its values lie, view units from
/// the layer's left (the pointer's place on it is the value while it is
/// held).
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct SliderTrack {
    pub row: usize,
    /// The hit box's left and right.
    pub x0: f32,
    pub x1: f32,
    /// The track's own ends: the lowest and highest value.
    pub start: f32,
    pub end: f32,
}

impl SliderTrack {
    /// The value fraction (0..1) of the pointer at `normalized` (the UI's
    /// cursor position over the hit box: −0.5 left edge, 0.5 right edge).
    pub fn fraction(&self, normalized: f32) -> f32 {
        let x = self.x0 + (normalized + 0.5) * (self.x1 - self.x0);
        ((x - self.start) / (self.end - self.start).max(f32::EPSILON)).clamp(0.0, 1.0)
    }
}

/// `s` split into lines no wider than `width` (by `measure`), at spaces;
/// a word wider than the line keeps a line of its own.
pub fn wrap(s: &str, width: f32, measure: impl Fn(&str) -> f32) -> Vec<String> {
    let mut lines = Vec::new();
    let mut cur = String::new();
    for w in s.split_whitespace() {
        let t = if cur.is_empty() {
            w.to_string()
        } else {
            format!("{cur} {w}")
        };
        if measure(&t) > width && !cur.is_empty() {
            lines.push(std::mem::replace(&mut cur, w.to_string()));
        } else {
            cur = t;
        }
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrap_breaks_at_spaces_within_the_width() {
        let m = |t: &str| t.len() as f32;
        assert_eq!(wrap("aa bb cc dd", 5.0, m), ["aa bb", "cc dd"]);
        assert_eq!(wrap("a verylongword b", 5.0, m), ["a", "verylongword", "b"]);
        assert!(wrap("", 5.0, m).is_empty());
    }

    #[test]
    fn the_track_maps_the_pointer_onto_its_own_ends() {
        let t = SliderTrack {
            row: 0,
            x0: 592.0,
            x1: 864.0,
            start: 600.0,
            end: 856.0,
        };
        assert_eq!(t.fraction(-0.5), 0.0);
        assert_eq!(t.fraction(0.5), 1.0);
        assert!((t.fraction(0.0) - 0.5).abs() < 1e-6);
    }
}
