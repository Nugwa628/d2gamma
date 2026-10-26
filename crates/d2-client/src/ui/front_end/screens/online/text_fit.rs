//! The online screens' texts against their boxes, as a window shows them:
//! every text whole inside its room (a button's between the art's ends, a
//! no-wrap label's box, else the part of the screen its pen is in: a panel,
//! a part of the chat, the frame), and no two texts on one line running
//! into each other. Measured in the window's pixels where the native UI
//! pass puts the 800 x 600 frame ([`UiPlacement`]), so a text font that
//! measures per window size (HD text) is checked at each. The lobby's
//! tests run it with conservative advances, its captures on the install
//! with the game's fonts.

use crate::gpu_compositor::ui_pass::UiPlacement;
use crate::ui::front_end::{DrawItem, SQUEEZED_LABEL_MX};
use crate::ui::geom::{Point, Rect};

/// Text control flags: no wrap (either bit), centred, right-aligned.
const NO_WRAP: u16 = 0x21;
const CENTRED: u16 = 2;
const RIGHT: u16 = 0x10;

/// One text's ink, in frame pixels: its left edge and width, its baseline,
/// and the room it must stay in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Placed {
    pub text: String,
    pub x: i32,
    pub w: i32,
    pub base: i32,
    pub room: Rect,
    /// A free text (no box of its own): it also must not run into the
    /// free texts on its line in its room.
    pub free: bool,
}

/// The texts of `items` with their rooms; `regions`: the parts of the
/// screen a free text may be in, from the bottom one up (a panel before
/// its parts and before a panel over it): the last one holding a text's
/// pen is its room, else the frame of `frame` size. Texts by string id are
/// left out (their strings are the game's), and so are texts a panel's
/// ground drawn after them hides (a panel taken over theirs).
pub fn placed(
    items: &[DrawItem],
    regions: &[Rect],
    frame: (u16, u16),
    adv: &dyn Fn(u16, &[u16]) -> i32,
) -> Vec<Placed> {
    let whole = Rect::new(0, 0, frame.0, frame.1);
    let mut out = Vec::new();
    for (k, it) in items.iter().enumerate() {
        let DrawItem::Text {
            string_id: 0,
            text,
            font,
            at,
            label,
            boxed,
            ..
        } = it
        else {
            continue;
        };
        if text.trim().is_empty() || hidden(&items[k + 1..], Point::new(at.x, at.y - 1)) {
            continue;
        }
        let units: Vec<u16> = text.encode_utf16().collect();
        let w = adv(*font, &units);
        let p = match (label, boxed) {
            // A button's label: centred between the art's ends.
            (Some(l), _) => {
                let bw = i32::from(l.w);
                let room = Rect::new(
                    at.x + SQUEEZED_LABEL_MX,
                    at.y - i32::from(l.h),
                    (bw - 2 * SQUEEZED_LABEL_MX).max(0) as u16,
                    l.h,
                );
                Placed {
                    text: text.clone(),
                    x: at.x + (bw - w) / 2,
                    w,
                    base: at.y,
                    room,
                    free: false,
                }
            }
            // A wrapping box: its rows are cut to it.
            (None, Some(b)) if b.flags & NO_WRAP == 0 => continue,
            (None, Some(b)) => {
                let bw = i32::from(b.w);
                let room = Rect::new(
                    at.x + b.mx,
                    at.y - i32::from(b.h),
                    (bw - 2 * b.mx).max(0) as u16,
                    b.h,
                );
                let x = if b.flags & CENTRED != 0 {
                    at.x + (bw - w) / 2
                } else if b.flags & RIGHT != 0 {
                    at.x - w
                } else {
                    at.x + b.mx
                };
                Placed {
                    text: text.clone(),
                    x,
                    w,
                    base: at.y,
                    room,
                    free: false,
                }
            }
            (None, None) => {
                let pen = Point::new(at.x, at.y - 1);
                let room = regions
                    .iter()
                    .rev()
                    .find(|r| r.contains(pen))
                    .copied()
                    .unwrap_or(whole);
                Placed {
                    text: text.clone(),
                    x: at.x,
                    w,
                    base: at.y,
                    room,
                    free: true,
                }
            }
        };
        out.push(p);
    }
    out
}

/// Whether a ground drawn in `later` covers `pen`.
fn hidden(later: &[DrawItem], pen: Point) -> bool {
    later.iter().any(|i| {
        matches!(i, DrawItem::Cut { src, at, .. } if Rect::new(at.x, at.y, src.w, src.h).contains(pen))
    })
}

/// What leaves its room or runs into another text at window placement
/// `place` (one line each; empty when everything fits).
pub fn clashes(texts: &[Placed], place: UiPlacement) -> Vec<String> {
    let sx = f64::from(place.out_w) / f64::from(place.frame_w);
    let at = |x: i32| (f64::from(x) * sx).round() as i64;
    let size = format!("{}x{}", place.out_w, place.out_h);
    let mut out = Vec::new();
    for t in texts {
        let (l, r) = (at(t.x), at(t.x + t.w));
        let (rl, rr) = (at(t.room.x), at(t.room.x + i32::from(t.room.w)));
        if l < rl || r > rr {
            out.push(format!(
                "{size}: {:?} spans {l}..{r}, its room {rl}..{rr}",
                t.text
            ));
        }
    }
    let mut free: Vec<&Placed> = texts.iter().filter(|t| t.free).collect();
    free.sort_by_key(|t| (t.room.x, t.room.y, t.base, t.x));
    for pair in free.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        if a.room != b.room || a.base != b.base {
            continue;
        }
        if at(a.x + a.w) > at(b.x) {
            out.push(format!(
                "{size}: {:?} runs into {:?} on line {}",
                a.text, b.text, a.base
            ));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::front_end::TextBox;

    fn adv(_: u16, t: &[u16]) -> i32 {
        t.len() as i32 * 6
    }

    fn text(s: &str, x: i32, y: i32, boxed: Option<TextBox>) -> DrawItem {
        DrawItem::Text {
            string_id: 0,
            text: s.into(),
            font: 0,
            at: Point::new(x, y),
            label: None,
            color: 0,
            boxed,
        }
    }

    const AT_1080: UiPlacement = UiPlacement {
        frame_w: 800,
        frame_h: 600,
        out_w: 1440,
        out_h: 1080,
    };

    /// A free text past its panel's edge, two cells of one line running
    /// into each other, a no-wrap label wider than its box: each named. A
    /// text in a panel over another one is the top panel's.
    #[test]
    fn it_names_what_leaves_its_room_or_overlaps() {
        let panel = Rect::new(100, 100, 100, 100);
        let label = TextBox {
            w: 40,
            h: 20,
            mx: 4,
            my: 0,
            flags: 0x20,
            row: 0,
        };
        let items = vec![
            text("fits", 110, 120, None),
            text("ok", 140, 120, None),
            text("too long for it", 150, 140, None),
            text("abc", 110, 160, None),
            text("def", 120, 160, None),
            text("wide label", 300, 300, Some(label)),
            text("short", 300, 330, Some(label)),
        ];
        let texts = placed(&items, &[panel], (800, 600), &adv);
        let c = clashes(&texts, AT_1080);
        assert_eq!(c.len(), 3, "{c:#?}");
        assert!(c[0].contains("too long for it"), "{c:#?}");
        assert!(c[1].contains("wide label"), "{c:#?}");
        assert!(c[2].contains("\"abc\" runs into \"def\""), "{c:#?}");
        // Over a panel listed before it, the text is the top panel's.
        let under = Rect::new(0, 0, 400, 400);
        let t = placed(
            &[text("on top", 120, 120, None)],
            &[under, panel],
            (800, 600),
            &adv,
        );
        assert_eq!(t[0].room, panel);
        // Under a panel's ground drawn after it: hidden, not checked.
        let ground = DrawItem::Cut {
            file: "bg",
            src: Rect::new(100, 100, 100, 100),
            at: Point::new(100, 100),
        };
        let t = placed(
            &[text("x".repeat(40).as_str(), 110, 120, None), ground],
            &[],
            (800, 600),
            &adv,
        );
        assert!(t.is_empty());
        // Wrapping boxes cut their rows: not checked.
        let wraps = TextBox { flags: 0, ..label };
        let t = placed(
            &[text("x".repeat(40).as_str(), 0, 50, Some(wraps))],
            &[],
            (800, 600),
            &adv,
        );
        assert!(t.is_empty());
    }
}
