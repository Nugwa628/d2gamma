//! HD Text's font: the outline font D2RUST ships, and how each of the
//! game's bitmap fonts is drawn with it at the screen's resolution.
//!
//! D2RUST ships one openly licensed font ([`HD_FONT`]: Marcellus SC, SIL
//! Open Font License 1.1, in `crates/d2-client/fonts/` with its licence
//! and an entry in `THIRD_PARTY_NOTICES.md`). It is the one place that
//! names the font: everything else asks for it here.
//!
//! With HD Text on, every glyph of the game's 14 bitmap fonts is drawn
//! from that font instead of from its bitmap cel, and nothing about the
//! text's layout changes:
//! - **Line breaks, widths and clicks** stay the bitmap font's: the text
//!   is laid out with the bitmap advances as always, so a string wraps at
//!   the same words, centres the same way and every click lands where it
//!   did. Only the letters' images move: each run of text on a line is set
//!   with the outline font's own advances and kerning, and the width left
//!   over (or missing) against the bitmap run is spread evenly over its
//!   letters ([`run_shifts`]), so the run starts and ends where the bitmap
//!   run does and the gaps between its letters are even. A letter alone is
//!   centred in its bitmap advance.
//! - **Size and baseline** are measured from the bitmap font itself
//!   ([`SheetStyle::measure`]): the outline font's capital height matches
//!   the bitmap's "H", its baseline sits on the bitmap's baseline, and it
//!   is narrowed when the bitmap font is narrower than the outline font.
//! - **Colour** comes from the bitmap font's own palette indices: the
//!   outline glyph is filled with the index the bitmap font's letters use
//!   most inside, so the text colour codes and blends, which remap those
//!   indices, apply unchanged. A font whose letters have a rim of another
//!   colour (a dark outline, or the light rim of an engraved font) gets a
//!   one-pixel rim in the index its letters' edges use most. One index each,
//!   not one per row: the bitmap fonts' rows vary in shade, and per-row
//!   indices would stripe the smooth outline glyphs.
//!
//! The glyph images are coverage images at the output scale with one
//! palette index per texel (`CelImage::IndexedGlyph`), drawn by the native
//! UI pass. A character the font does not have keeps its bitmap glyph.

use std::sync::OnceLock;

use ab_glyph::{point, Font, FontRef, GlyphId, PxScale, ScaleFont};
use d2_formats::font::FontTable;

use super::hd_text::{FontSheet, GlyphCel};

/// A font file D2RUST ships.
#[derive(Debug, Clone, Copy)]
pub struct ShippedFont {
    /// The family name.
    pub name: &'static str,
    /// Its licence, as the notices name it.
    pub licence: &'static str,
    /// The font file (TrueType).
    pub bytes: &'static [u8],
}

/// The HD Text font: every HD glyph, in game and in the settings screens.
pub const HD_FONT: ShippedFont = ShippedFont {
    name: "Marcellus SC",
    licence: "SIL Open Font License 1.1",
    bytes: include_bytes!("../../../fonts/marcellus-sc/MarcellusSC-Regular.ttf"),
};

/// The parsed HD font.
pub fn font() -> Option<&'static FontRef<'static>> {
    static FONT: OnceLock<Option<FontRef<'static>>> = OnceLock::new();
    FONT.get_or_init(|| FontRef::try_from_slice(HD_FONT.bytes).ok())
        .as_ref()
}

/// The outline font's glyph for a game character code, `None` when the
/// font has none.
pub fn glyph_for(font: &FontRef<'_>, code: u16) -> Option<GlyphId> {
    let c = char::from_u32(u32::from(code))?;
    if c.is_control() {
        return None;
    }
    let id = font.glyph_id(c);
    (id.0 != 0).then_some(id)
}

/// The font's capital height in its own units (the top of "H").
fn cap_unscaled(font: &FontRef<'_>) -> Option<f32> {
    let h = font.outline(font.glyph_id('H'))?;
    // The outline's box keeps its top in `min.y` (font units, y up).
    let top = h.bounds.min.y.max(h.bounds.max.y);
    (top > 0.0).then_some(top)
}

/// The narrowest the outline font is squeezed to fit a bitmap font's
/// advances, as a share of its natural width.
const MIN_SQUEEZE: f32 = 0.82;
/// The widest it is stretched for a bitmap font with wide advances, so the
/// letters fill their cells a little more without changing their shapes
/// much.
const MAX_STRETCH: f32 = 1.10;
/// Below [`MIN_SQUEEZE`], the size gives way too, down to this share of the
/// bitmap's capital height.
const MIN_SHRINK: f32 = 0.85;

/// The characters whose advances set the outline font's width.
const WIDTH_SAMPLE: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
/// The letters whose pixels set the fill and rim colours.
const COLOUR_SAMPLE: &str = "ABCDEFGHIKLMNOPRSTUVWXYZabcdeghknopsuxz";

/// How one bitmap font's text is drawn with the HD font, measured from the
/// bitmap font. The baseline is in frame pixels relative to the pen (the
/// glyph's anchor: [`GlyphCel::top`] is in the same terms).
#[derive(Debug, Clone, PartialEq)]
pub struct SheetStyle {
    /// The baseline: the row below the bitmap "H".
    pub baseline: i32,
    /// The outline font's capital height, frame pixels (the rim, when
    /// there is one, comes on top).
    pub cap: f32,
    /// Horizontal scale against the vertical one (1: the font's own
    /// proportions).
    pub squeeze: f32,
    /// Whether the letters have a rim of another colour.
    pub rim: bool,
    /// The palette index of the letters' insides.
    pub fill: u8,
    /// The palette index of the letters' edges (the rim's colour).
    pub edge: u8,
}

/// The most frequent index of a histogram (ties: the higher index); `None`
/// when empty.
fn mode(h: &[u32; 256]) -> Option<u8> {
    let (i, n) = h.iter().enumerate().skip(1).max_by_key(|&(i, &n)| (n, i))?;
    (*n > 0).then_some(i as u8)
}

/// The glyph record of character `code` in `table`.
fn record(table: &FontTable, code: u16) -> Option<&d2_formats::font::Glyph> {
    table.glyphs.iter().find(|g| g.code == code)
}

impl SheetStyle {
    /// Measures `sheet`'s bitmap font (its glyph table `table`) against
    /// the HD font. `None` when the sheet has no usable "H".
    pub fn measure(sheet: &dyn FontSheet, table: &FontTable, font: &FontRef<'_>) -> Option<Self> {
        let cel_of = |code: u16| -> Option<GlyphCel<'_>> {
            let cel = sheet.cel(usize::from(record(table, code)?.frame))?;
            (cel.pixels.len() == (cel.width * cel.height) as usize).then_some(cel)
        };
        // The capital height and baseline: the inked rows of "H".
        let h = cel_of(u16::from(b'H'))?;
        let w = h.width as usize;
        let inked: Vec<i32> = (0..h.height as usize)
            .filter(|&y| h.pixels[y * w..(y + 1) * w].iter().any(|&p| p != 0))
            .map(|y| y as i32)
            .collect();
        let (first, last) = (*inked.first()?, *inked.last()?);
        let baseline = h.top + last + 1;
        let ink_cap = (last - first + 1) as f32;

        // The colours: the index the letters use most inside, and on their
        // edges (a pixel beside a transparent one).
        let (mut all_fill, mut all_edge) = ([0u32; 256], [0u32; 256]);
        for c in COLOUR_SAMPLE.bytes() {
            let Some(cel) = cel_of(u16::from(c)) else {
                continue;
            };
            let (cw, ch) = (cel.width as i32, cel.height as i32);
            let at = |x: i32, y: i32| -> u8 {
                if x < 0 || y < 0 || x >= cw || y >= ch {
                    0
                } else {
                    cel.pixels[(y * cw + x) as usize]
                }
            };
            for y in 0..ch {
                for x in 0..cw {
                    let p = at(x, y);
                    if p == 0 {
                        continue;
                    }
                    let edge = [(1, 0), (-1, 0), (0, 1), (0, -1)]
                        .iter()
                        .any(|&(dx, dy)| at(x + dx, y + dy) == 0);
                    if edge {
                        all_edge[usize::from(p)] += 1;
                    } else {
                        all_fill[usize::from(p)] += 1;
                    }
                }
            }
        }
        let fill = mode(&all_fill).or(mode(&all_edge))?;
        let edge = mode(&all_edge).unwrap_or(fill);
        let rim = edge != fill;

        // The outline font's size: its capital height is the bitmap's, less
        // the rim drawn around it.
        let mut cap = ink_cap - if rim { 2.0 } else { 0.0 };
        if cap < 3.0 {
            cap = ink_cap;
        }
        let cap_u = cap_unscaled(font)?;
        let scaled = font.as_scaled(PxScale::from(cap * font.height_unscaled() / cap_u));
        let (mut bitmap_w, mut outline_w) = (0f32, 0f32);
        for c in WIDTH_SAMPLE.chars() {
            let (Some(r), id) = (record(table, c as u16), font.glyph_id(c)) else {
                continue;
            };
            if id.0 == 0 || r.width == 0 {
                continue;
            }
            bitmap_w += f32::from(r.width);
            outline_w += scaled.h_advance(id) + if rim { 1.0 } else { 0.0 };
        }
        let fit = if outline_w > 0.0 {
            bitmap_w / outline_w
        } else {
            1.0
        };
        let mut squeeze = fit.clamp(MIN_SQUEEZE, MAX_STRETCH);
        if fit < MIN_SQUEEZE {
            let shrink = (fit / MIN_SQUEEZE).max(MIN_SHRINK);
            cap *= shrink;
            squeeze = (fit / shrink).clamp(MIN_SQUEEZE, 1.0);
        }
        Some(SheetStyle {
            baseline,
            cap,
            squeeze,
            rim,
            fill,
            edge,
        })
    }
}

/// One glyph drawn with the HD font: palette index and coverage per texel
/// at `scale` texels per frame pixel, top row first. The letter is centred
/// in its bitmap advance; [`run_shifts`] moves it within its run.
#[derive(Debug, Clone, PartialEq)]
pub struct GlyphImage {
    pub width: u32,
    pub height: u32,
    /// The image's top-left in frame pixels from the cel's top-left.
    pub origin: [i32; 2],
    /// (palette index, coverage 0..=255) per texel.
    pub texels: Vec<[u8; 2]>,
    /// What laying the glyph out in a run needs.
    pub outline: OutlineGlyph,
}

/// An outline glyph as a run layout sees it. Lengths are frame pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OutlineGlyph {
    /// Which bitmap font it stands for (glyphs of one run share it).
    pub sheet: u32,
    /// The character, and its cel's left and top from the pen (to draw
    /// it again at another phase).
    pub code: u16,
    pub cel: (i32, i32),
    /// The outline font's glyph (for kerning).
    pub glyph: GlyphId,
    /// The bitmap glyph's advance, and the outline glyph's.
    pub bitmap_advance: i32,
    pub advance: f32,
    /// The bitmap font's space advance (0: it has none) and the outline
    /// font's at this size.
    pub space_bitmap: i32,
    pub space_advance: f32,
    /// Frame pixels per font unit across (kerning values are in font units).
    pub kern_scale: f32,
}

/// Steps per output pixel at which a letter of a run is drawn: it lands
/// within an eighth of an output pixel of its place.
pub const PHASES: u8 = 4;

/// The most spaces a gap between two letters of one run may hold; a wider
/// gap starts a new run.
const MAX_RUN_SPACES: i32 = 16;

/// How far each glyph of the text moves from where it is drawn alone
/// (centred in its bitmap advance), frame pixels across. `glyphs` are the
/// outline glyphs of a frame in draw order, with the frame position of
/// their cels (each cel sits a fixed distance from its pen, so cel
/// positions are pen positions).
///
/// A run is a sequence of glyphs of one bitmap font on one row, each
/// following the one before at its bitmap advance plus any number of
/// spaces (spaces draw nothing, so they are not in `glyphs`). A run is set
/// with the outline font's advances, kerning between neighbours and its
/// space advance; the difference against the bitmap run's width is spread
/// evenly, half a share before the first letter, one share between each
/// two, half after the last, so the run covers exactly the bitmap run.
pub fn run_shifts(font: &FontRef<'_>, glyphs: &[(i32, i32, OutlineGlyph)]) -> Vec<f32> {
    let mut out = vec![0f32; glyphs.len()];
    let mut start = 0;
    while start < glyphs.len() {
        // The run's glyphs, and the spaces before each.
        let mut spaces = vec![0i32];
        let mut end = start + 1;
        while end < glyphs.len() {
            let (px, py, p) = glyphs[end - 1];
            let (x, y, g) = glyphs[end];
            if g.sheet != p.sheet || y != py {
                break;
            }
            let gap = x - px - p.bitmap_advance;
            let m = match gap {
                0 => 0,
                _ if p.space_bitmap > 0 && gap > 0 && gap % p.space_bitmap == 0 => {
                    gap / p.space_bitmap
                }
                _ => break,
            };
            if m > MAX_RUN_SPACES {
                break;
            }
            spaces.push(m);
            end += 1;
        }
        let run = &glyphs[start..end];
        // Pen positions with the outline font's advances.
        let mut pens = Vec::with_capacity(run.len());
        let mut pen = 0f32;
        for (i, &(_, _, g)) in run.iter().enumerate() {
            if i > 0 {
                let p = run[i - 1].2;
                pen += p.advance;
                if spaces[i] == 0 {
                    pen += font.kern_unscaled(p.glyph, g.glyph) * g.kern_scale;
                } else {
                    pen += spaces[i] as f32 * g.space_advance;
                }
            }
            pens.push(pen);
        }
        let last = run[run.len() - 1];
        let outline_w = pen + last.2.advance;
        let bitmap_w = (last.0 - run[0].0 + last.2.bitmap_advance) as f32;
        let share = (bitmap_w - outline_w) / run.len() as f32;
        for (i, &(x, _, g)) in run.iter().enumerate() {
            let want = run[0].0 as f32 + pens[i] + (i as f32 + 0.5) * share;
            let alone = x as f32 + (g.bitmap_advance as f32 - g.advance) / 2.0;
            out[start + i] = want - alone;
        }
        start = end;
    }
    out
}

/// Draws character `code`, whose bitmap glyph advances `advance` frame
/// pixels and whose cel is `cel` (`left` and `top` pen-relative), in the
/// HD font with `style` at `scale` output pixels per frame pixel, moved
/// `phase` output pixels across (0 to 1: a run places letters between
/// output pixels, [`PHASES`]). `sheet` names the bitmap font and `space` is
/// its space advance. `None` when the font has no such character or it
/// draws nothing.
#[allow(clippy::too_many_arguments)]
pub fn glyph_image(
    font: &FontRef<'_>,
    style: &SheetStyle,
    code: u16,
    advance: u8,
    cel: (i32, i32),
    scale: f32,
    sheet: u32,
    space: u8,
    phase: f32,
) -> Option<GlyphImage> {
    if !scale.is_finite() || scale <= 0.0 {
        return None;
    }
    let id = glyph_for(font, code)?;
    let cap_u = cap_unscaled(font)?;
    let height = style.cap * font.height_unscaled() / cap_u * scale;
    let px = PxScale {
        x: height * style.squeeze,
        y: height,
    };
    let scaled = font.as_scaled(px);
    let own_advance = scaled.h_advance(id) / scale;
    // Pen-relative frame pixels: centred in the bitmap advance, on the
    // bitmap baseline; a rimmed glyph's fill sits a pixel higher, so its rim
    // ends on the baseline as the bitmap letters' rim does.
    let pen_x = (f32::from(advance) - own_advance) / 2.0 + phase / scale;
    let pen_y = style.baseline as f32 - if style.rim { 1.0 } else { 0.0 };
    let outlined = font.outline_glyph(id.with_scale_and_position(px, point(0.0, 0.0)))?;
    let b = outlined.px_bounds();
    let rim = if style.rim { scale } else { 0.0 };
    // The image's box in frame pixels (pen-relative), whole pixels, with a
    // frame pixel to spare across for the part of a run shift smaller than
    // a pixel (`CelImage::shifted`).
    let x0 = ((pen_x * scale + b.min.x - rim - 1.0) / scale).floor() as i32 - 1;
    let y0 = ((pen_y * scale + b.min.y - rim - 1.0) / scale).floor() as i32;
    let x1 = ((pen_x * scale + b.max.x + rim + 1.0) / scale).ceil() as i32 + 1;
    let y1 = ((pen_y * scale + b.max.y + rim + 1.0) / scale).ceil() as i32;
    let (w, h) = (
        ((x1 - x0) as f32 * scale).ceil() as usize,
        ((y1 - y0) as f32 * scale).ceil() as usize,
    );
    if w == 0 || h == 0 {
        return None;
    }
    // The outline's coverage, placed in the image.
    let at = point(
        pen_x * scale - x0 as f32 * scale,
        pen_y * scale - y0 as f32 * scale,
    );
    let placed = font.outline_glyph(id.with_scale_and_position(px, at))?;
    let pb = placed.px_bounds();
    let mut cover = vec![0f32; w * h];
    placed.draw(|x, y, c| {
        let (x, y) = (x as i64 + pb.min.x as i64, y as i64 + pb.min.y as i64);
        if (0..w as i64).contains(&x) && (0..h as i64).contains(&y) {
            cover[y as usize * w + x as usize] = c.min(1.0);
        }
    });
    if !cover.iter().any(|&c| c > 0.0) {
        return None;
    }
    let edge = if style.rim {
        dilate(&cover, w, h, rim)
    } else {
        cover.clone()
    };
    let texels = (0..w * h)
        .map(|i| {
            let (f, e) = (cover[i], edge[i]);
            let a = f.max(e);
            if a <= 0.5 / 255.0 {
                return [0, 0];
            }
            let index = if !style.rim || f >= 0.5 {
                style.fill
            } else {
                style.edge
            };
            [index, (a * 255.0).round() as u8]
        })
        .collect();
    Some(GlyphImage {
        width: w as u32,
        height: h as u32,
        origin: [x0 - cel.0, y0 - cel.1],
        texels,
        outline: OutlineGlyph {
            sheet,
            code,
            cel,
            glyph: id,
            bitmap_advance: i32::from(advance),
            advance: own_advance,
            space_bitmap: i32::from(space),
            space_advance: scaled.h_advance(font.glyph_id(' ')) / scale,
            kern_scale: scaled.h_scale_factor() / scale,
        },
    })
}

/// `cover` grown by `r` texels in every direction, with a soft last texel.
fn dilate(cover: &[f32], w: usize, h: usize, r: f32) -> Vec<f32> {
    let reach = r.ceil() as i64 + 1;
    let mut out = vec![0f32; w * h];
    for y in 0..h as i64 {
        for x in 0..w as i64 {
            let mut best = 0f32;
            for dy in -reach..=reach {
                for dx in -reach..=reach {
                    let (sx, sy) = (x + dx, y + dy);
                    if sx < 0 || sy < 0 || sx >= w as i64 || sy >= h as i64 {
                        continue;
                    }
                    let d = ((dx * dx + dy * dy) as f32).sqrt();
                    let weight = (r + 1.0 - d).clamp(0.0, 1.0);
                    best = best.max(cover[sy as usize * w + sx as usize] * weight);
                }
            }
            out[y as usize * w + x as usize] = best;
        }
    }
    out
}

#[cfg(test)]
#[path = "hd_fonts_tests.rs"]
mod tests;
