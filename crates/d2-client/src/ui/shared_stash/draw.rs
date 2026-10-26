// D2RUST's own: the shared stash's draw on the stash panel.
//! The shared stash's parts over the stash panel art, in the colours of
//! the Display v3 kit (`ui::settings::widgets::colour`: dark panels, gold
//! accents, greyed rows), matched to the act palette ([`Kit::of`]): the
//! panel is drawn on the game's palette surface, so it is scaled with the
//! rest of the UI and its rectangles are the hit-test rectangles
//! ([`super::layout`]).
//!
//! Items are drawn with the game's item pictures: from the item's last
//! client look when the store kept one, else from its code.

use d2_shared_stash::StackPage;

use super::layout::{Category, Places, Tab, CATEGORIES, CELL, TABS};
use super::state::{SharedStashUi, Shown};
use crate::ui::draw::{LineRequest, RectRequest, UiDraw, UiDrawSink};
use crate::ui::geom::{Point, Rect};
use crate::ui::item_moves::{hint_line, Shortcut, ShortcutSet};
use crate::ui::item_tip::{TipAnchor, TipLine};
use crate::ui::original::FontMeasure;
use crate::ui::panels::inv_items::ItemsUi;
use crate::ui::panels::{text, UiFiles};
use crate::ui::settings::widgets::colour;

/// Font 6 (`Font6`): counts and tab names.
const SMALL: u16 = 6;
/// Font 1 (`Font16`): buttons and the gold line.
const LABEL: u16 = 1;
/// Text colours of the string tables' colour codes.
const WHITE: u16 = 0;
const GOLD: u16 = 4;
const GREY: u16 = 5;
const TAN: u16 = 7;

/// The rectangle draw mode that writes the colour (mode 0 blends it).
const OPAQUE: u8 = 5;

/// The kit's colours as act palette indices.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Kit {
    pub panel: u8,
    pub edge: u8,
    pub gold: u8,
    pub on: u8,
    pub field: u8,
    pub hit: u8,
}

impl Kit {
    /// The nearest palette entries of the kit's colours.
    pub fn of(palette: &[[u8; 3]]) -> Option<Kit> {
        let n = |c: [u8; 3]| crate::ui::inv_grid::nearest_index(palette, (c[0], c[1], c[2]));
        Some(Kit {
            panel: n(colour::PANEL)?,
            edge: n(colour::PANEL_EDGE)?,
            gold: n(colour::GOLD)?,
            on: n(colour::TAB_ON)?,
            field: n(colour::FIELD)?,
            hit: n(colour::PICKED)?,
        })
    }
}

/// What a draw reads.
pub struct DrawIn<'a> {
    pub st: &'a SharedStashUi,
    pub places: Places,
    pub kit: Kit,
    pub items: &'a ItemsUi,
    pub files: &'a UiFiles,
    pub fonts: Option<&'a FontMeasure>,
    pub mouse: Point,
    /// An item is held on the cursor.
    pub holding: bool,
    /// The item-moving shortcuts switched on (their tip lines).
    pub shortcuts: ShortcutSet,
    /// Items the market holds in escrow (posted from the Market
    /// category): left out of the page.
    pub skip: &'a [u32],
    /// Items on the market's sell slot or picked up for it: drawn faded.
    pub faded: &'a [u32],
}

pub(crate) fn fill(r: Rect, color: u8, out: &mut dyn UiDrawSink) {
    out.push(UiDraw::Rect(RectRequest::sized(
        r.x,
        r.y,
        i32::from(r.w),
        i32::from(r.h),
        color,
        OPAQUE,
    )));
}

/// A one-pixel frame inside `r`.
pub(crate) fn frame(r: Rect, color: u8, out: &mut dyn UiDrawSink) {
    let (x0, y0) = (r.x, r.y);
    let (x1, y1) = (r.x + i32::from(r.w) - 1, r.y + i32::from(r.h) - 1);
    for (a, b, c, d) in [
        (x0, y0, x1, y0),
        (x1, y0, x1, y1),
        (x1, y1, x0, y1),
        (x0, y1, x0, y0),
    ] {
        out.push(UiDraw::Line(LineRequest {
            x0: a,
            y0: b,
            x1: c,
            y1: d,
            color,
        }));
    }
}

fn units(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

/// `s` centred in `r` (the pen on the text's bottom row).
fn label(i: &DrawIn, r: Rect, s: &str, font: u16, color: u16, out: &mut dyn UiDrawSink) {
    let t = units(s);
    let w = i
        .fonts
        .and_then(|f| f.width_a(font, &t))
        .unwrap_or(t.len() as i32 * 6);
    let h = if font == SMALL { 7 } else { 10 };
    let x = r.x + (i32::from(r.w) - w) / 2;
    let y = r.y + (i32::from(r.h) + h) / 2;
    out.push(text(t, x, y, font, color));
}

/// A button: filled, framed in gold when on, its label centred.
fn button(i: &DrawIn, r: Rect, s: &str, on: bool, font: u16, out: &mut dyn UiDrawSink) {
    let k = i.kit;
    fill(r, if on { k.on } else { k.panel }, out);
    frame(r, if on { k.gold } else { k.edge }, out);
    let hover = r.contains(i.mouse);
    let color = if on || hover { GOLD } else { TAN };
    label(i, r, s, font, color, out);
}

/// The shared stash's parts: the tabs and the category buttons always, the
/// shown shared page and the bottom rows when a shared page is shown.
pub fn draw(i: &DrawIn, out: &mut dyn UiDrawSink) {
    let st = i.st;
    if !st.on {
        return;
    }
    let p = i.places;
    let shown = st.shown();
    for n in 0..TABS {
        let Some(t) = Tab::of_index(n) else { continue };
        let on = st.tab.0 == t && matches!(st.category.0, Category::Stash | Category::Market);
        button(i, p.tab(t), t.label(), on, SMALL, out);
    }
    for (n, c) in CATEGORIES.into_iter().enumerate() {
        button(i, p.category(n), c.label(), st.category.0 == c, LABEL, out);
    }
    if !st.covers() {
        return;
    }
    for c in p.cover() {
        fill(c, i.kit.panel, out);
    }
    for c in p.cover() {
        frame(c, i.kit.edge, out);
    }
    match shown {
        Shown::Page(page) => item_page(i, page, out),
        Shown::Stacks(page) => stack_page(i, page, out),
        Shown::Personal => {}
    }
    bottom(i, shown, out);
    count_box(i, out);
}

fn item_page(i: &DrawIn, page: u8, out: &mut dyn UiDrawSink) {
    let p = i.places;
    let k = i.kit;
    for y in 0..d2_shared_stash::ROWS {
        for x in 0..d2_shared_stash::COLS {
            let r = p.cell(x, y);
            fill(r, k.field, out);
            frame(r, k.edge, out);
        }
    }
    let searching = !i.st.search.trim().is_empty();
    let shown = |it: &&super::state::ShownItem| it.cell.page == page && !i.skip.contains(&it.id.0);
    for it in i.st.items.iter().filter(shown) {
        let r = p.cell(it.cell.x, it.cell.y);
        let foot = Rect::new(
            r.x,
            r.y,
            (CELL * i32::from(it.size.0)) as u16,
            (CELL * i32::from(it.size.1)) as u16,
        );
        if searching && i.st.hit(it.id) {
            fill(foot, k.hit, out);
        }
        let code = code_key(&it.code);
        match i.items.picture(
            i.files,
            it.look.as_deref(),
            code,
            (r.x, r.y),
            (CELL, CELL),
            (searching && !i.st.hit(it.id)) || i.faded.contains(&it.id.0),
        ) {
            Some((d, _)) => out.push(d),
            None => label(i, foot, &it.code, SMALL, WHITE, out),
        }
    }
}

pub fn code_key(code: &str) -> [u8; 4] {
    let mut k = [b' '; 4];
    for (d, s) in k.iter_mut().zip(code.bytes()) {
        *d = s;
    }
    k
}

fn stack_page(i: &DrawIn, page: StackPage, out: &mut dyn UiDrawSink) {
    let Some(cat) = i.st.catalog.as_ref() else {
        return;
    };
    let p = i.places;
    let k = i.kit;
    let cols = page.cols();
    for slot in cat.page(page) {
        let (x, y) = slot.at;
        let r = p.slot(cols, x, y);
        let n = i.st.stacks.get(&slot.code).copied().unwrap_or(0);
        let pic = Rect::new(r.x, r.y, CELL as u16, CELL as u16);
        fill(pic, k.field, out);
        frame(pic, if n > 0 { k.edge } else { k.panel }, out);
        if let Some((d, _)) = i.items.picture(
            i.files,
            None,
            code_key(&slot.code),
            (pic.x, pic.y),
            (CELL, CELL),
            n == 0,
        ) {
            out.push(d);
        }
        let line = Rect::new(r.x, r.y + CELL, r.w, (r.h as i32 - CELL) as u16);
        let color = if n > 0 { WHITE } else { GREY };
        label(i, line, &n.to_string(), SMALL, color, out);
    }
}

fn bottom(i: &DrawIn, shown: Shown, out: &mut dyn UiDrawSink) {
    let p = i.places;
    let k = i.kit;
    let st = i.st;
    let g = p.gold();
    let line = format!("Gold: {}", st.gold);
    let t = units(&line);
    out.push(text(t, g.x + 2, g.y + 13, LABEL, GOLD));
    button(i, p.gold_in(), "In", false, LABEL, out);
    button(i, p.gold_out(), "Out", false, LABEL, out);
    let s = p.search();
    fill(s, k.field, out);
    frame(s, if st.search_focus { k.gold } else { k.edge }, out);
    let (shown_text, color) = if st.search.is_empty() && !st.search_focus {
        ("Search".to_owned(), GREY)
    } else if st.search_focus {
        (format!("{}_", st.search), WHITE)
    } else {
        (st.search.clone(), WHITE)
    };
    out.push(text(units(&shown_text), s.x + 4, s.y + 13, LABEL, color));
    if let Shown::Page(_) = shown {
        button(i, p.sort(), "Sort", false, LABEL, out);
    }
}

fn count_box(i: &DrawIn, out: &mut dyn UiDrawSink) {
    let Some(b) = i.st.count_box.as_ref() else {
        return;
    };
    let p = i.places;
    let a = p.area();
    let r = Rect::new(a.x + 20, a.y + i32::from(a.h) / 2 - 20, a.w - 40, 40);
    fill(r, i.kit.on, out);
    frame(r, i.kit.gold, out);
    let have = i.st.stacks.get(&b.code).copied().unwrap_or(0);
    label(
        i,
        Rect::new(r.x, r.y, r.w, 20),
        &format!("How many? (1-{have})"),
        LABEL,
        TAN,
        out,
    );
    label(
        i,
        Rect::new(r.x, r.y + 18, r.w, 20),
        &format!("{}_", b.text),
        LABEL,
        WHITE,
        out,
    );
}

/// The tip of the shared item or slot under the mouse (none while an
/// item is held).
pub fn hover_tip(i: &DrawIn) -> Option<(Vec<TipLine>, TipAnchor)> {
    let st = i.st;
    if !st.covers() || i.holding || st.count_box.is_some() {
        return None;
    }
    let p = i.places;
    match st.shown() {
        Shown::Page(page) => {
            let it = st
                .items
                .iter()
                .filter(|it| it.cell.page == page && !i.skip.contains(&it.id.0))
                .find(|it| {
                    let r = p.cell(it.cell.x, it.cell.y);
                    let foot = Rect::new(
                        r.x,
                        r.y,
                        (CELL * i32::from(it.size.0)) as u16,
                        (CELL * i32::from(it.size.1)) as u16,
                    );
                    foot.contains(i.mouse)
                })?;
            let mut lines = match (&i.items.tips, it.look.as_deref()) {
                (Some(t), Some(look)) => t.lines_with_fillers(look, &it.fillers),
                _ => Vec::new(),
            };
            if lines.is_empty() {
                lines.push(TipLine::new(&it.name, WHITE));
            }
            if st.selling() {
                lines.push(TipLine::new("Left: pick up for the Sell slot", GREY));
                lines.push(TipLine::new("Ctrl+Left: onto the Sell slot", GREY));
            } else if i.shortcuts.has(Shortcut::Stash) {
                lines.push(hint_line(Shortcut::Stash, "Move to inventory"));
            }
            let a = p.area();
            let anchor = TipAnchor::of_cell(
                (a.x, a.y),
                (CELL, CELL),
                (i32::from(it.cell.x), i32::from(it.cell.y)),
                (i32::from(it.size.0), i32::from(it.size.1)),
            );
            Some((lines, anchor))
        }
        Shown::Stacks(page) => {
            let cat = st.catalog.as_ref()?;
            let slot = cat
                .page(page)
                .find(|s| p.slot(page.cols(), s.at.0, s.at.1).contains(i.mouse))?;
            let n = st.stacks.get(&slot.code).copied().unwrap_or(0);
            let mut lines = vec![
                TipLine::new(&slot.name, GOLD),
                TipLine::new(format!("In the stash: {n}"), WHITE),
            ];
            if n > 0 {
                lines.push(TipLine::new("Left: Take one", GREY));
                for (s, words) in [
                    (Shortcut::TakeCount, "Take some"),
                    (Shortcut::TakeAll, "Take all that fit"),
                ] {
                    if i.shortcuts.has(s) {
                        lines.push(hint_line(s, words));
                    }
                }
            }
            let r = p.slot(page.cols(), slot.at.0, slot.at.1);
            let anchor = TipAnchor {
                x: r.x + i32::from(r.w) / 2,
                top: r.y,
                bottom: r.y + i32::from(r.h),
            };
            Some((lines, anchor))
        }
        Shown::Personal => None,
    }
}
