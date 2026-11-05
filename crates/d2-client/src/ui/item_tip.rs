//! Client item text hooks and tooltip drawing over the shared engine builder.
use super::draw::UiDrawSink;
use super::geom::Point;
use super::item_tip_build::{Build, PriceText, TipCtx};
use super::original::FontMeasure;
use super::panel::StringLookup;
use super::panels::UiFiles;
use crate::bridge::world::ClientWorld;
use crate::mods::{ClientMods, ItemLabel};
use d2_data::bin::BinSet;
pub use d2_item_text::item_tip::{color, text_lines, TipLine};
use d2_item_text::lookup::TablesLookup;
use d2_proto::item_bits::{decode, ItemBits};
use d2_sim::items::ItemTables;
use std::sync::Arc;
const FONT: u16 = 1;
#[derive(Clone, Debug)]
pub struct ItemTips {
    pub core: d2_item_text::item_tip::ItemTips,
    pub(crate) mods: ClientMods,
}
impl std::ops::Deref for ItemTips {
    type Target = d2_item_text::item_tip::ItemTips;
    fn deref(&self) -> &Self::Target {
        &self.core
    }
}
impl std::ops::DerefMut for ItemTips {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.core
    }
}
impl ItemTips {
    pub fn new(
        lookup: Arc<ItemTables>,
        set: &BinSet,
        strings: Arc<dyn StringLookup + Send + Sync>,
    ) -> Result<Self, String> {
        Ok(Self {
            core: d2_item_text::item_tip::ItemTips::new(lookup, set, strings)?,
            mods: ClientMods::default(),
        })
    }
    /// The ground-item label of the item with last stream `stream`: the
    /// item's name as the client shows it and the label's text colour,
    /// after the mod game's label filters (`crate::mods::ItemNameFilter`).
    /// `None` when the stream does not decode or a filter hides the label.
    /// The filters read `world`.
    pub fn label_name(
        &self,
        stream: &[u8],
        ctx: &TipCtx<'_>,
        world: &ClientWorld,
    ) -> Option<(Vec<u16>, u16)> {
        let b = decode(stream, &TablesLookup(&self.lookup)).ok()?;
        self.label_of(&b, ctx, world)
    }

    /// [`Self::label_name`] of decoded item `b`.
    pub fn label_of(
        &self,
        b: &ItemBits,
        ctx: &TipCtx<'_>,
        world: &ClientWorld,
    ) -> Option<(Vec<u16>, u16)> {
        let built = d2_item_text::item_tip::ItemTips::as_built(b);
        let build = Build::new(self, &built, ctx);
        let (text, color) = (build.name(), build.label_color());
        if !self.mods.has_item_name_filter() {
            return Some((text, color));
        }
        let mut label = ItemLabel {
            text,
            color,
            shown: true,
        };
        self.mods.filter_item_name(world, b, &mut label);
        label.shown.then_some((label.text, label.color))
    }

    pub fn bits(&self, stream: &[u8]) -> Option<ItemBits> {
        self.core.bits(stream)
    }
    pub fn is_socket_filler(&self, code: [u8; 4]) -> bool {
        self.core.is_socket_filler(code)
    }
    pub fn tip_lines(&self, stream: &[u8], ctx: &TipCtx<'_>) -> Vec<TipLine> {
        match self.bits(stream) {
            Some(b) => self.tip_lines_of(&b, ctx),
            None => Vec::new(),
        }
    }

    /// The box lines of the tip of decoded item `b` with `ctx`, top line
    /// first, after the mod game's item hooks (`crate::mods::ItemLines`)
    /// ran over them.
    pub fn tip_lines_of(&self, b: &ItemBits, ctx: &TipCtx<'_>) -> Vec<TipLine> {
        let t = self.tip_of(b, ctx);
        let mut lines = if t.text.is_empty() {
            Vec::new()
        } else {
            text_lines(&t.text, t.color)
        };
        if self.mods.has_item_lines() {
            self.mods.item_lines(b, &mut lines);
        }
        lines
    }

    pub fn lines(&self, stream: &[u8]) -> Vec<TipLine> {
        self.tip_lines(stream, &TipCtx::default())
    }

    /// The tip lines of `stream` whose sockets hold `fillers` (their item
    /// message streams, socket order), as the inventory shows a socketed
    /// item: the fillers' properties are listed with its own.
    pub fn lines_with_fillers(&self, stream: &[u8], fillers: &[Vec<u8>]) -> Vec<TipLine> {
        let ctx = TipCtx {
            fillers: fillers.iter().filter_map(|f| self.bits(f)).collect(),
            ..TipCtx::default()
        };
        self.tip_lines(stream, &ctx)
    }

    pub fn shop_lines(&self, stream: &[u8], price: u32, usable: bool) -> Vec<TipLine> {
        let _ = usable;
        let ctx = TipCtx {
            mode: 1,
            price: PriceText::Price {
                label: super::item_tip_build::tid::COST,
                price: price as i32,
            },
            ..TipCtx::default()
        };
        self.tip_lines(stream, &ctx)
    }
}

/// The hover anchor of an item (X; top; bottom).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TipAnchor {
    pub x: i32,
    pub top: i32,
    pub bottom: i32,
}

impl TipAnchor {
    /// The anchor of an item at grid cell (`c`, `r`) of size `w` × `h` cells in
    /// a grid at (`left`, `top`) with cells `cell_w` × `cell_h`: x = left +
    /// cellW·c + (w·cellW)/2, top = top + cellH·r, bottom = top + cellH·(r +
    /// h).
    pub fn of_cell(
        (left, top): (i32, i32),
        (cell_w, cell_h): (i32, i32),
        (c, r): (i32, i32),
        (w, h): (i32, i32),
    ) -> Self {
        TipAnchor {
            x: left + cell_w * c + (w * cell_w) / 2,
            top: top + cell_h * r,
            bottom: top + cell_h * (r + h),
        }
    }

    /// PROVISIONAL (the store-tip check does not reach the tip yet, so it does
    /// not test the anchor; the inventory item tip agrees): a bare point
    /// (no item rectangle known to the caller) is a zero-height anchor.
    pub fn at_point(p: Point) -> Self {
        TipAnchor {
            x: p.x,
            top: p.y,
            bottom: p.y,
        }
    }
}

/// The one pop-up text of `lines` (top line first), bottom line first
/// with a colour code where the colour changes; the
/// colour of the first (bottom) line is the pop-up's.
fn popup_text(lines: &[TipLine]) -> (Vec<u16>, u16) {
    let mut out: Vec<u16> = Vec::new();
    let mut cur = None;
    for (i, l) in lines.iter().rev().enumerate() {
        if i > 0 {
            out.push(u16::from(b'\n'));
        }
        if cur != Some(l.color) {
            out.extend([0xFF, u16::from(b'c'), u16::from(b'0') + l.color]);
            cur = Some(l.color);
        }
        out.extend_from_slice(&l.text);
    }
    out.truncate(1023);
    (out, lines.last().map_or(0, |l| l.color))
}

/// Queues the tip of `lines` as the hover pop-up `(text, x, y,
/// colour, 1)`: x:= the anchor's x; y:= the anchor's top when top − H > 0, else
/// bottom + H, H the text height; the box and text are the pop-up draw of
/// [`super::hud_tips::push_popup`].
pub fn draw_tip_at(
    lines: &[TipLine],
    anchor: TipAnchor,
    screen: (i32, i32),
    fonts: Option<&FontMeasure>,
    out: &mut dyn UiDrawSink,
) {
    if lines.is_empty() {
        return;
    }
    let (text, color) = popup_text(lines);
    // H: the pop-up's rectangle height away from every clamp.
    let height = fonts
        .and_then(|f| f.popup(FONT, &text, Point::new(0, 5000), true, (10_000, 10_000)))
        .map_or(0, |fr| fr.rect.1.y - fr.rect.0.y);
    let y = if anchor.top - height > 0 {
        anchor.top
    } else {
        anchor.bottom + height
    };
    super::original::hud_tips::push_popup(
        text,
        Point::new(anchor.x, y),
        color,
        true,
        screen,
        fonts,
        out,
    );
}

/// [`draw_tip_at`] for a caller that knows only the mouse (the anchor is
/// the point; `files` is no longer read).
pub fn draw_tip(
    lines: &[TipLine],
    mouse: Point,
    screen: (i32, i32),
    fonts: Option<&FontMeasure>,
    files: &UiFiles,
    out: &mut dyn UiDrawSink,
) {
    let _ = files;
    draw_tip_at(lines, TipAnchor::at_point(mouse), screen, fonts, out);
}

#[cfg(test)]
pub(crate) mod tests;
