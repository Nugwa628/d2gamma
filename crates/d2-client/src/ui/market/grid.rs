// D2RUST's own: the market browser's grid view.
//! The results as compact names in their quality's colour, three to a
//! row, under a header per quality group (the page comes ordered group
//! first, in the contract's grid order). The mouse over a name shows the
//! item's full tip; a click shows it on the right.

use crate::ui::draw::UiDrawSink;
use crate::ui::geom::Rect;

use super::details::group_title;
use super::window::{DrawIn, GREY, SMALL};
use super::MarketUi;
use crate::market::source::{Goods, Group};

/// Names in a row.
pub const COLUMNS: usize = 3;
/// A row's height.
const ROW_H: i32 = 11;

/// A line of the grid: a group's header or a row of up to three names
/// (their indexes on the page).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Line {
    Header(Group),
    Names(Vec<usize>),
}

/// The grid's lines, top to bottom.
pub fn lines(ui: &MarketUi) -> Vec<Line> {
    let mut out = Vec::new();
    let Some(r) = &ui.results else { return out };
    let mut group = None;
    let mut row: Vec<usize> = Vec::new();
    for (i, l) in r.rows.iter().enumerate() {
        if group != Some(l.group) {
            if !row.is_empty() {
                out.push(Line::Names(std::mem::take(&mut row)));
            }
            out.push(Line::Header(l.group));
            group = Some(l.group);
        }
        row.push(i);
        if row.len() == COLUMNS {
            out.push(Line::Names(std::mem::take(&mut row)));
        }
    }
    if !row.is_empty() {
        out.push(Line::Names(row));
    }
    out
}

pub fn row_count(ui: &MarketUi) -> usize {
    lines(ui).len()
}

/// The lines `area` shows at once.
pub fn visible(area: Rect) -> usize {
    ((i32::from(area.h) - 4) / ROW_H).max(1) as usize
}

/// A shown part of the grid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cell {
    Header(Group, Rect),
    /// Listing `i` of the page.
    Item(usize, Rect),
}

/// The parts shown in `area`, from the scrolled line.
pub fn cells(ui: &MarketUi, area: Rect) -> Vec<Cell> {
    let col_w = (i32::from(area.w) - 4) / COLUMNS as i32;
    let mut out = Vec::new();
    let all = lines(ui);
    let top = ui.scroll.min(all.len().saturating_sub(1));
    for (k, line) in all.into_iter().skip(top).take(visible(area)).enumerate() {
        let y = area.y + 2 + k as i32 * ROW_H;
        match line {
            Line::Header(g) => out.push(Cell::Header(
                g,
                Rect::new(area.x + 2, y, area.w - 4, ROW_H as u16),
            )),
            Line::Names(ix) => {
                for (c, i) in ix.into_iter().enumerate() {
                    let x = area.x + 2 + c as i32 * col_w;
                    out.push(Cell::Item(
                        i,
                        Rect::new(x, y, (col_w - 2) as u16, ROW_H as u16),
                    ));
                }
            }
        }
    }
    out
}

/// A listing's grid name: its name, a stack's count after it.
fn grid_name(ui: &MarketUi, i: usize) -> String {
    let Some(l) = ui.listing(i) else {
        return String::new();
    };
    match &l.goods {
        Goods::Stack { count, .. } if *count > 1 => format!("{} x{count}", l.item.name),
        Goods::Gold(n) => format!("{n} gold"),
        _ => l.item.name.clone(),
    }
}

pub fn draw(i: &DrawIn, area: Rect, out: &mut dyn UiDrawSink) {
    let (ui, pen) = (i.ui, &i.pen);
    for c in cells(ui, area) {
        match c {
            Cell::Header(g, r) => {
                let n = ui
                    .results
                    .as_ref()
                    .map_or(0, |p| p.rows.iter().filter(|l| l.group == g).count());
                pen.text(
                    r.x + 2,
                    r.y + 2,
                    &format!("{} ({n})", group_title(g)),
                    SMALL,
                    GREY,
                    out,
                );
                let w = pen.width(SMALL, group_title(g)) + 30;
                pen.hline(
                    r.x + w,
                    r.x + i32::from(r.w) - 2,
                    r.y + 6,
                    pen.kit.edge,
                    out,
                );
            }
            Cell::Item(n, r) => {
                let Some(l) = ui.listing(n) else { continue };
                let picked = ui.selected.as_ref().is_some_and(|s| s.listing == l.listing);
                if picked {
                    pen.fill(r, pen.kit.on, out);
                    pen.frame(r, pen.kit.gold, out);
                } else if r.contains(pen.mouse) {
                    pen.fill(r, pen.kit.hit, out);
                }
                pen.line(r, &grid_name(ui, n), SMALL, l.group.color(), out);
            }
        }
    }
}
