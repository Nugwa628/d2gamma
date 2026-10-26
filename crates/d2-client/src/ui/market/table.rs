// D2RUST's own: the market browser's table view.
//! The results one listing a row: the item (in its quality's colour),
//! its key stats, its level requirement, the price, the seller and how
//! long ago it was listed. A click on a column's header sorts by it (a
//! second click turns the order round); the key stats column does not
//! sort (a stat sort picks its stat, which comes with the stat filters).

use crate::market::source::MarketSort;
use crate::ui::draw::UiDrawSink;
use crate::ui::geom::Rect;

use super::details::{age, listing_price};
use super::window::{DrawIn, GOLD, GREY, SMALL, TAN, WHITE};
use super::MarketUi;

/// The columns: the sort a header click picks, the title, the width's
/// share (of 362). Each title fits its column with the sort mark
/// ("LISTED v").
const COLUMNS: [(Option<MarketSort>, &str, i32); 6] = [
    (Some(MarketSort::Name), "ITEM", 84),
    (None, "KEY STATS", 70),
    (Some(MarketSort::Level), "REQ", 32),
    (Some(MarketSort::Price), "PRICE", 76),
    (Some(MarketSort::Seller), "SELLER", 52),
    (Some(MarketSort::Posted), "LISTED", 48),
];
const SHARES: i32 = 362;
const HEAD_H: i32 = 13;
const ROW_H: i32 = 11;

/// The columns' rectangles of a row at `y`.
fn columns(area: Rect, y: i32, h: i32) -> Vec<Rect> {
    let w = i32::from(area.w) - 4;
    let mut x = area.x + 2;
    COLUMNS
        .iter()
        .map(|(_, _, share)| {
            let cw = w * share / SHARES;
            let r = Rect::new(x, y, cw as u16, h as u16);
            x += cw;
            r
        })
        .collect()
}

/// The headers: the sort each picks, its title, its rectangle.
pub fn headers(area: Rect) -> Vec<(Option<MarketSort>, &'static str, Rect)> {
    columns(area, area.y + 2, HEAD_H)
        .into_iter()
        .zip(COLUMNS)
        .map(|(r, (s, t, _))| (s, t, r))
        .collect()
}

pub fn row_count(ui: &MarketUi) -> usize {
    ui.results.as_ref().map_or(0, |r| r.rows.len())
}

/// The rows `area` shows at once.
pub fn visible(area: Rect) -> usize {
    ((i32::from(area.h) - HEAD_H - 6) / ROW_H).max(1) as usize
}

/// The shown rows: the listing's index on the page and its rectangle.
pub fn rows(ui: &MarketUi, area: Rect) -> Vec<(usize, Rect)> {
    let n = row_count(ui);
    let top = ui.scroll.min(n.saturating_sub(1));
    (top..n)
        .take(visible(area))
        .enumerate()
        .map(|(k, i)| {
            let y = area.y + 4 + HEAD_H + k as i32 * ROW_H;
            (i, Rect::new(area.x + 2, y, area.w - 4, ROW_H as u16))
        })
        .collect()
}

pub fn draw(i: &DrawIn, area: Rect, out: &mut dyn UiDrawSink) {
    let (ui, pen) = (i.ui, &i.pen);
    for (s, t, r) in headers(area) {
        let on = s.is_some_and(|s| s == ui.sort);
        let title = match (on, ui.desc) {
            (true, true) => format!("{t} v"),
            (true, false) => format!("{t} ^"),
            _ => t.to_owned(),
        };
        let color = if on {
            GOLD
        } else if s.is_some() && r.contains(pen.mouse) {
            WHITE
        } else {
            GREY
        };
        pen.line(r, &title, SMALL, color, out);
    }
    let y = area.y + 2 + HEAD_H;
    pen.hline(
        area.x + 2,
        area.x + i32::from(area.w) - 3,
        y,
        pen.kit.edge,
        out,
    );
    let now = ui.results.as_ref().map_or(0, |r| r.now_ms);
    for (n, r) in rows(ui, area) {
        let Some(l) = ui.listing(n) else { continue };
        let picked = ui.selected.as_ref().is_some_and(|s| s.listing == l.listing);
        if picked {
            pen.fill(r, pen.kit.on, out);
            pen.frame(r, pen.kit.gold, out);
        } else if r.contains(pen.mouse) {
            pen.fill(r, pen.kit.hit, out);
        }
        let cols = columns(area, r.y, ROW_H);
        let stats = l.item.stats.join(", ");
        let req = if l.item.level_req > 0 {
            l.item.level_req.to_string()
        } else {
            "-".into()
        };
        let cells: [(&str, u16); 6] = [
            (&l.item.name, l.group.color()),
            (&stats, 3),
            (&req, WHITE),
            (&listing_price(ui, l), GOLD),
            (&l.seller, TAN),
            (&age(now, l.posted_ms), GREY),
        ];
        for (c, (s, color)) in cols.into_iter().zip(cells) {
            pen.line(c, s, SMALL, color, out);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::geom::Point;
    use crate::ui::layout::Screen;
    use crate::ui::market::window::{Pen, Places};
    use crate::ui::shared_stash::Kit;

    /// Every header fits its column with the sort mark: "LISTED" was cut
    /// to "LIST.." in the table view.
    #[test]
    fn table_headers_fit_with_sort_mark() {
        assert_eq!(COLUMNS.iter().map(|c| c.2).sum::<i32>(), SHARES);
        let pen = Pen {
            kit: Kit {
                panel: 0,
                edge: 0,
                gold: 0,
                on: 0,
                field: 0,
                hit: 0,
            },
            fonts: None,
            mouse: Point::new(-1, -1),
        };
        // The window at its full width (800 x 600 and up).
        let area = Places::of(&Screen::R800).area();
        for (s, t, r) in headers(area) {
            let title = if s.is_some() {
                format!("{t} v")
            } else {
                t.to_owned()
            };
            // `Pen::line` fits the text into the width less 4.
            let room = i32::from(r.w) - 4;
            assert_eq!(pen.fit(SMALL, &title, room), title);
        }
    }
}
