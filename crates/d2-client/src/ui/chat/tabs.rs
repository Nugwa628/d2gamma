//! The tab strip: a framed box per tab, the open one in gold with a double
//! frame, the others tan; a tab with unread lines carries its count in a
//! round red badge. The strip always fits its width: the full labels
//! ("MOD: D2GAMMA", "GUILD: WG"), else the short ones ("MOD", "GUILD") with
//! the full label as the tab's tip, each in the line font, else in the
//! small font.

use super::paint::{colour, ChatFont, ChatPaint};
use crate::chat::{ChatModel, Tab};
use crate::ui::geom::{Point, Rect};

const PAD: i32 = 8;
const GAP: i32 = 3;
/// Room kept free at the strip's right end: a glyph's cel may reach past
/// its advance.
const SLACK: i32 = 2;

/// A tab's box and what it shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TabBox {
    pub tab: Tab,
    pub at: Rect,
    pub label: String,
    /// The full label when `label` is the short one (the tab's tip).
    pub tip: Option<String>,
    pub unread: u16,
    pub font: ChatFont,
}

/// An unread badge's size for count `n`: as high as a small-font row and
/// 2 px, round for one digit, longer for more.
pub fn badge_size(paint: &dyn ChatPaint, n: u16) -> (i32, i32) {
    let h = paint.row_height(ChatFont::Small) + 2;
    let w = (paint.width(ChatFont::Small, &n.to_string()) + 6).max(h);
    (w, h)
}

/// The tabs' boxes in `strip`.
pub fn boxes(model: &ChatModel, strip: Rect, paint: &dyn ChatPaint) -> Vec<TabBox> {
    let tabs = model.tabs();
    let layout = |font: ChatFont, short: bool| {
        let mut x = strip.x;
        let mut v = Vec::new();
        for t in &tabs {
            let full = model.tab_label(*t).to_uppercase();
            let label = if short {
                model.tab_short_label(*t).to_uppercase()
            } else {
                full.clone()
            };
            let unread = model.unread(*t);
            let mut w = paint.width(font, &label) + 2 * PAD;
            if unread > 0 {
                w += badge_size(paint, unread).0 + 2;
            }
            v.push(TabBox {
                tab: *t,
                at: Rect::new(x, strip.y, w.max(1) as u16, strip.h),
                tip: (label != full).then_some(full),
                label,
                unread,
                font,
            });
            x += w + GAP;
        }
        (v, x - GAP - strip.x)
    };
    let tries = [
        (ChatFont::Text, false),
        (ChatFont::Text, true),
        (ChatFont::Small, true),
    ];
    let mut last = Vec::new();
    for (font, short) in tries {
        let (v, w) = layout(font, short);
        if w + SLACK <= i32::from(strip.w) {
            return v;
        }
        last = v;
    }
    last
}

/// The tab under `p` whose label is shortened: its full label.
pub fn tip_at(boxes: &[TabBox], p: Point) -> Option<(&TabBox, &str)> {
    boxes
        .iter()
        .find(|b| b.at.contains(p))
        .and_then(|b| b.tip.as_deref().map(|t| (b, t)))
}

/// Draws the strip.
pub fn draw(model: &ChatModel, strip: Rect, paint: &mut dyn ChatPaint) {
    let open = model.open_tab_now();
    for b in boxes(model, strip, paint) {
        let r = b.at;
        paint.panel(r);
        paint.outline(r);
        let on = b.tab == open;
        if on {
            paint.outline(Rect::new(r.x + 1, r.y + 1, r.w - 2, r.h - 2));
        }
        let row_h = paint.row_height(b.font);
        let base = r.y + (i32::from(r.h) + row_h) / 2 - 1;
        let c = if on { colour::GOLD } else { colour::TAN };
        paint.text(b.font, r.x + PAD, base, c, &b.label);
        if b.unread > 0 {
            let (bw, bh) = badge_size(paint, b.unread);
            let bx = r.x + i32::from(r.w) - PAD / 2 - bw;
            let by = r.y + (i32::from(r.h) - bh) / 2;
            paint.badge(
                Rect::new(bx, by, bw as u16, bh as u16),
                &b.unread.to_string(),
            );
        }
    }
}
