//! Floating panels: the core the lobby (its chat and right panel) and the
//! in-game chat window build their movable, resizable panels on.
//!
//! A host makes a [`FloatLayer`] with its panels' classic rects (the fixed
//! layout, the default) and minimum sizes, feeds it the pointer
//! ([`FloatLayer::press`], [`FloatLayer::drag`], [`FloatLayer::release`])
//! before its own controls, and draws each panel at its rect with the gold
//! Celtic frame ([`celtic`]). A panel's title strip moves it, its edges and
//! corners resize it ([`grab`]); edges snap to the canvas and to the other
//! panels and every rect stays on the canvas ([`geom`]). Locked, a layer has
//! no grab zones; reset puts back the classic rects. The layout is saved per
//! realm, account and scene ([`store`]) with each rect anchored to its
//! nearest canvas corner, so it fits the lobby's 800 × 600 and a game's
//! view alike.
//!
//! Everything here is plain Rust: no Bevy types, no drawing.

pub mod celtic;
pub mod geom;
pub mod grab;
pub mod store;

use crate::ui::geom::{Point, Rect};

pub use celtic::{CelticArt, Style};
pub use geom::{Anchored, Canvas, Corner, MinSize};
pub use grab::{Edges, Grab, Zone};
pub use store::{LayoutKey, LayoutStore, SavedLayout, Scene};

/// One floating panel.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FloatPanel {
    /// The id the layout is saved under (`chat`, `right`).
    pub id: &'static str,
    pub rect: Rect,
    /// The host's fixed layout for this panel.
    pub classic: Rect,
    pub min: MinSize,
    /// Edges and corners resize it (else only the title strip moves it).
    pub resizable: bool,
}

impl FloatPanel {
    /// A resizable panel at its classic rect.
    pub fn new(id: &'static str, classic: Rect, min: MinSize) -> Self {
        Self {
            id,
            rect: classic,
            classic,
            min,
            resizable: true,
        }
    }

    pub fn fixed_size(mut self) -> Self {
        self.resizable = false;
        self
    }
}

/// A host's floating panels, in draw order (the last on top).
#[derive(Clone, Debug)]
pub struct FloatLayer {
    canvas: Canvas,
    panels: Vec<FloatPanel>,
    locked: bool,
    /// The panel being dragged or resized.
    held: Option<(&'static str, Grab)>,
    /// The layout changed since [`Self::take_changed`].
    changed: bool,
}

impl FloatLayer {
    /// The panels at their classic rects, unlocked.
    pub fn new(canvas: Canvas, panels: Vec<FloatPanel>) -> Self {
        Self {
            canvas,
            panels,
            locked: false,
            held: None,
            changed: false,
        }
    }

    pub fn canvas(&self) -> Canvas {
        self.canvas
    }

    /// The panels in draw order.
    pub fn panels(&self) -> &[FloatPanel] {
        &self.panels
    }

    pub fn rect(&self, id: &str) -> Option<Rect> {
        self.panel(id).map(|p| p.rect)
    }

    fn panel(&self, id: &str) -> Option<&FloatPanel> {
        self.panels.iter().find(|p| p.id == id)
    }

    fn panel_mut(&mut self, id: &str) -> Option<&mut FloatPanel> {
        self.panels.iter_mut().find(|p| p.id == id)
    }

    pub fn locked(&self) -> bool {
        self.locked
    }

    /// Locks or unlocks the layout; locking drops a held grab where the
    /// panel is.
    pub fn set_locked(&mut self, locked: bool) {
        if locked != self.locked {
            self.locked = locked;
            self.held = None;
            self.changed = true;
        }
    }

    /// A panel is being dragged or resized.
    pub fn holding(&self) -> Option<&'static str> {
        self.held.map(|(id, _)| id)
    }

    /// Every panel back at its classic rect (the lock stays as it is).
    pub fn reset(&mut self) {
        self.held = None;
        for p in &mut self.panels {
            p.rect = p.classic;
        }
        self.changed = true;
    }

    /// Every panel is at its classic rect.
    pub fn is_classic(&self) -> bool {
        self.panels.iter().all(|p| p.rect == p.classic)
    }

    /// The host's classic rect of `id` for the current canvas.
    pub fn set_classic(&mut self, id: &str, classic: Rect) {
        if let Some(p) = self.panel_mut(id) {
            p.classic = classic;
        }
    }

    /// The topmost panel and its zone under `p`; none when locked.
    pub fn zone_at(&self, p: Point) -> Option<(&'static str, Zone)> {
        if self.locked {
            return None;
        }
        let top = self.panels.iter().rev().find(|f| f.rect.contains(p))?;
        grab::zone_at(top.rect, p, top.resizable).map(|z| (top.id, z))
    }

    /// The pointer went down at `p`: on a grab zone the panel is taken (and
    /// raised to the top). `true`: the layer took the press (the host's
    /// controls don't get it).
    pub fn press(&mut self, p: Point) -> bool {
        let Some((id, zone)) = self.zone_at(p) else {
            return false;
        };
        if let Some(i) = self.panels.iter().position(|f| f.id == id) {
            let f = self.panels.remove(i);
            self.held = Some((id, Grab::new(zone, p, f.rect)));
            self.panels.push(f);
        }
        true
    }

    /// The pointer moved to `p`: a held panel follows. `true` while a
    /// panel is held.
    pub fn drag(&mut self, p: Point) -> bool {
        let Some((id, g)) = self.held else {
            return false;
        };
        let others: Vec<Rect> = self
            .panels
            .iter()
            .filter(|f| f.id != id)
            .map(|f| f.rect)
            .collect();
        let canvas = self.canvas;
        if let Some(f) = self.panel_mut(id) {
            f.rect = g.rect_at(p, f.min, canvas, &others);
        }
        true
    }

    /// The pointer went up at `p`: a held panel lands there. `true` when a
    /// panel was held.
    pub fn release(&mut self, p: Point) -> bool {
        if !self.drag(p) {
            return false;
        }
        if let Some((id, g)) = self.held.take() {
            if self.rect(id) != Some(g.from) {
                self.changed = true;
            }
        }
        true
    }

    /// Drops a held grab, the panel back where it was (Esc, or the window
    /// lost the pointer).
    pub fn cancel(&mut self) {
        if let Some((id, g)) = self.held.take() {
            if let Some(f) = self.panel_mut(id) {
                f.rect = g.from;
            }
        }
    }

    /// The canvas changed size: each panel keeps its distance from its
    /// nearest corner. The host gives the new classic rects with
    /// [`Self::set_classic`].
    pub fn set_canvas(&mut self, canvas: Canvas) {
        if canvas == self.canvas {
            return;
        }
        self.cancel();
        let old = self.canvas;
        for f in &mut self.panels {
            f.rect = geom::at_least(Anchored::of(f.rect, old).place(canvas), f.min, canvas);
        }
        self.canvas = canvas;
    }

    /// The layout changed (moved, resized, locked or reset) since the last
    /// call: the host saves [`Self::saved`] then.
    pub fn take_changed(&mut self) -> bool {
        std::mem::take(&mut self.changed)
    }

    /// The layout to store: the lock and every panel's anchored rect.
    pub fn saved(&self) -> SavedLayout {
        SavedLayout {
            locked: self.locked,
            panels: self
                .panels
                .iter()
                .map(|f| (f.id.to_owned(), Anchored::of(f.rect, self.canvas)))
                .collect(),
        }
    }

    /// Puts back a stored layout: each panel it names at its anchored rect
    /// on this canvas (kept at its minimum), the others at their classic
    /// rects; ids the host doesn't have are ignored.
    pub fn apply(&mut self, saved: &SavedLayout) {
        self.held = None;
        self.locked = saved.locked;
        let canvas = self.canvas;
        for f in &mut self.panels {
            f.rect = match saved.panel(f.id) {
                Some(a) => geom::at_least(a.place(canvas), f.min, canvas),
                None => f.classic,
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOBBY: Canvas = Canvas::new(800, 600);
    const CHAT: Rect = Rect::new(48, 179, 352, 326);
    const RIGHT: Rect = Rect::new(446, 312, 320, 192);

    fn lobby() -> FloatLayer {
        FloatLayer::new(
            LOBBY,
            vec![
                FloatPanel::new("chat", CHAT, MinSize::new(200, 120)),
                FloatPanel::new("right", RIGHT, MinSize::new(260, 160)),
            ],
        )
    }

    #[test]
    fn a_title_drag_moves_the_panel_and_marks_the_layout() {
        let mut l = lobby();
        assert!(l.press(Point::new(200, 190)));
        assert_eq!(l.holding(), Some("chat"));
        assert!(l.drag(Point::new(230, 215)));
        assert_eq!(l.rect("chat"), Some(Rect::new(78, 204, 352, 326)));
        assert!(l.release(Point::new(230, 215)));
        assert_eq!(l.holding(), None);
        assert!(l.take_changed());
        assert!(!l.take_changed());
        assert!(!l.is_classic());
        // The body of a panel isn't a grab zone: the host's controls get it.
        assert!(!l.press(Point::new(600, 400)));
        assert!(!l.drag(Point::new(610, 400)));
        assert!(!l.release(Point::new(610, 400)));
    }

    #[test]
    fn the_grabbed_panel_is_raised() {
        let mut l = lobby();
        assert_eq!(l.panels().last().unwrap().id, "right");
        assert!(l.press(Point::new(100, 190)));
        assert!(l.release(Point::new(100, 190)));
        assert_eq!(l.panels().last().unwrap().id, "chat");
        // A click without a move changes nothing to save.
        assert!(!l.take_changed());
    }

    #[test]
    fn locked_has_no_grab() {
        let mut l = lobby();
        l.set_locked(true);
        assert!(l.take_changed());
        for p in [
            Point::new(200, 190),
            Point::new(48, 300),
            Point::new(765, 503),
            Point::new(600, 320),
        ] {
            assert_eq!(l.zone_at(p), None, "{p:?}");
            assert!(!l.press(p));
        }
        assert_eq!(l.rect("chat"), Some(CHAT));
        // Locking mid-drag drops the grab where the panel is.
        let mut l = lobby();
        assert!(l.press(Point::new(200, 190)));
        assert!(l.drag(Point::new(210, 190)));
        l.set_locked(true);
        assert_eq!(l.holding(), None);
        assert!(!l.drag(Point::new(300, 300)));
        assert_eq!(l.rect("chat"), Some(Rect::new(58, 179, 352, 326)));
    }

    #[test]
    fn panels_snap_to_each_other() {
        let mut l = lobby();
        // The right panel's left edge dragged to 5 px from the chat's
        // right edge (400): it snaps onto it, and its bottom (504) onto
        // the chat's bottom (505).
        assert!(l.press(Point::new(600, 320)));
        assert!(l.release(Point::new(559, 320)));
        assert_eq!(l.rect("right"), Some(Rect::new(400, 313, 320, 192)));
    }

    #[test]
    fn cancel_puts_the_panel_back() {
        let mut l = lobby();
        assert!(l.press(Point::new(447, 400)));
        assert!(l.drag(Point::new(300, 400)));
        assert_ne!(l.rect("right"), Some(RIGHT));
        l.cancel();
        assert_eq!(l.rect("right"), Some(RIGHT));
        assert!(!l.take_changed());
    }

    #[test]
    fn reset_gives_the_classic_rects() {
        let mut l = lobby();
        assert!(l.press(Point::new(200, 190)));
        assert!(l.release(Point::new(260, 240)));
        l.set_locked(true);
        l.reset();
        assert!(l.is_classic());
        assert!(l.locked());
        assert_eq!(l.rect("chat"), Some(CHAT));
        assert_eq!(l.rect("right"), Some(RIGHT));
    }

    #[test]
    fn a_saved_layout_comes_back_on_another_canvas() {
        let mut l = lobby();
        assert!(l.press(Point::new(600, 320)));
        assert!(l.release(Point::new(620, 380)));
        let moved = l.rect("right").unwrap();
        l.set_locked(true);
        let saved = l.saved();

        // The store keeps it; read back into a fresh lobby.
        let mut store = LayoutStore::default();
        let key = LayoutKey::new("d2gamma-eu", "Nugwa", Scene::Lobby);
        store.set(key.clone(), saved);
        let store = LayoutStore::parse(&store.to_text()).unwrap();
        let mut again = lobby();
        again.apply(store.get(&key).unwrap());
        assert!(again.locked());
        assert_eq!(again.rect("right"), Some(moved));
        assert_eq!(again.rect("chat"), Some(CHAT));

        // The same layout in a 1280 x 720 view: same gaps to the same
        // corners.
        let game = Canvas::new(1280, 720);
        let mut wide = FloatLayer::new(
            game,
            vec![
                FloatPanel::new("chat", CHAT, MinSize::new(200, 120)),
                FloatPanel::new("right", RIGHT, MinSize::new(260, 160)),
            ],
        );
        wide.apply(store.get(&key).unwrap());
        let r = wide.rect("right").unwrap();
        assert_eq!(
            (1280 - (r.x + i32::from(r.w)), 720 - (r.y + i32::from(r.h))),
            (
                800 - (moved.x + i32::from(moved.w)),
                600 - (moved.y + i32::from(moved.h))
            )
        );
        // And set_canvas moves live panels the same way.
        let mut live = again.clone();
        live.set_canvas(game);
        assert_eq!(live.rect("right"), Some(r));
        // The chat's centre is below the middle: bottom-left.
        assert_eq!(live.rect("chat"), Some(Rect::new(48, 299, 352, 326)));
    }

    #[test]
    fn apply_keeps_minimums_and_ignores_unknown_panels() {
        let mut l = lobby();
        l.apply(&SavedLayout {
            locked: false,
            panels: vec![
                (
                    "chat".into(),
                    Anchored {
                        corner: Corner::TopLeft,
                        dx: 10,
                        dy: 10,
                        w: 20,
                        h: 20,
                    },
                ),
                (
                    "ladder".into(),
                    Anchored {
                        corner: Corner::TopLeft,
                        dx: 0,
                        dy: 0,
                        w: 300,
                        h: 300,
                    },
                ),
            ],
        });
        assert_eq!(l.rect("chat"), Some(Rect::new(10, 10, 200, 120)));
        assert_eq!(l.rect("right"), Some(RIGHT));
        assert_eq!(l.panels().len(), 2);
    }

    #[test]
    fn a_fixed_size_panel_only_moves() {
        let mut l = FloatLayer::new(
            LOBBY,
            vec![
                FloatPanel::new("box", Rect::new(100, 100, 200, 100), MinSize::new(50, 50))
                    .fixed_size(),
            ],
        );
        assert_eq!(l.zone_at(Point::new(100, 150)), None);
        assert_eq!(l.zone_at(Point::new(100, 105)), Some(("box", Zone::Title)));
        assert!(l.press(Point::new(150, 100)));
        assert!(l.release(Point::new(160, 120)));
        assert_eq!(l.rect("box"), Some(Rect::new(110, 120, 200, 100)));
    }
}
