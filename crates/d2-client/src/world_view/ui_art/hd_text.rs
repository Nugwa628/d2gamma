//! Text glyphs at the output resolution, in one of two looks:
//! - **HD Text off** (the default): the game's own bitmap glyphs, enhanced.
//!   Scale2x rounds diagonal edges without inventing colours; repeated
//!   passes produce an integer prescale above the output scale and the UI
//!   pass smooths the final edges.
//! - **HD Text on** (`[video] hd_text`): every glyph drawn from D2RUST's
//!   shipped outline font, measured against the bitmap font so the text's
//!   layout stays the same ([`super::hd_fonts`]). A character the outline
//!   font lacks keeps its enhanced bitmap glyph.
//!
//! Either way the atlas texels hold palette indices that go through the
//! item's palette maps before filtering, preserving colour codes and
//! multicolour glyphs. Only the glyph image changes: advances, baselines,
//! clipping and layout remain in frame pixels. Images are cached in memory
//! for the active scale and look.

use super::hd_fonts::{self, OutlineGlyph, SheetStyle};
use super::{CelRef, UiArt};
use crate::frames::{FrameAnchor, FrameSetKey, IndexFrame};
use crate::gpu_compositor::ui_pass::{CelImage, RepAtlas, REP_PAGE};
use d2_formats::font::FontTable;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

pub const MAX_PAGES: usize = 8;

/// `[video] hd_text`: whether text is drawn from the outline font.
static HD_FONTS: AtomicBool = AtomicBool::new(false);

/// Switches HD Text on or off (from the settings); glyphs already cached
/// for the other look are dropped on the next draw.
pub fn set_hd_fonts(on: bool) {
    HD_FONTS.store(on, Ordering::Relaxed);
}

/// Whether HD Text is on.
pub fn hd_fonts() -> bool {
    HD_FONTS.load(Ordering::Relaxed)
}

/// Edge-aware 2x expansion. Every output index comes from the input;
/// transparent index zero participates in edge detection like any colour.
pub(crate) fn scale2x(src: &[u8], w: usize, h: usize) -> Vec<u8> {
    let mut dst = vec![0; w * h * 4];
    let pixel = |x: usize, y: usize| src[y * w + x];
    for y in 0..h {
        for x in 0..w {
            let e = pixel(x, y);
            let b = pixel(x, y.saturating_sub(1));
            let d = pixel(x.saturating_sub(1), y);
            let f = pixel((x + 1).min(w - 1), y);
            let g = pixel(x, (y + 1).min(h - 1));
            let out = if b != g && d != f {
                [
                    if d == b { d } else { e },
                    if b == f { f } else { e },
                    if d == g { d } else { e },
                    if g == f { f } else { e },
                ]
            } else {
                [e; 4]
            };
            let at = y * 2 * w * 2 + x * 2;
            dst[at..at + 2].copy_from_slice(&out[..2]);
            dst[at + w * 2..at + w * 2 + 2].copy_from_slice(&out[2..]);
        }
    }
    dst
}

#[derive(Debug, Clone, Copy)]
pub struct GlyphCel<'a> {
    pub width: u32,
    pub height: u32,
    /// The cel's left column and top row, frame pixels from the pen.
    pub left: i32,
    pub top: i32,
    pub pixels: &'a [u8],
}

impl<'a> GlyphCel<'a> {
    pub fn of_frame(frame: &'a IndexFrame) -> Self {
        let top = match frame.anchor {
            FrameAnchor::Bottom => frame.y_off - frame.height as i32 + 1,
            FrameAnchor::Top | FrameAnchor::TopDown => frame.y_off,
        };
        Self {
            width: frame.width,
            height: frame.height,
            left: frame.x_off,
            top,
            pixels: &frame.pixels,
        }
    }
}

pub trait FontSheet {
    fn path(&self) -> &str;
    fn table(&self) -> Option<&FontTable>;
    fn cel(&self, index: usize) -> Option<GlyphCel<'_>>;
}

struct StoreSheet<'a, 'b> {
    key: &'b FrameSetKey,
    cel: &'b CelRef<'a>,
}
impl FontSheet for StoreSheet<'_, '_> {
    fn path(&self) -> &str {
        self.key.path()
    }
    fn table(&self) -> Option<&FontTable> {
        // The sheet's glyph table: the `.tbl` beside the `.dc6`.
        let tbl = self.key.path().strip_suffix(".dc6")?.to_owned() + ".tbl";
        let path = crate::assets::path::CanonicalPath::new(&tbl).ok()?;
        self.cel.fonts.get(&path)
    }
    fn cel(&self, index: usize) -> Option<GlyphCel<'_>> {
        let frames = self.cel.frames;
        Some(GlyphCel::of_frame(
            frames.frame(frames.id(self.key, index).ok()?)?,
        ))
    }
}

fn is_font_sheet(path: &str) -> bool {
    path.ends_with(".dc6") && path.contains("/font/")
}

#[derive(Default)]
struct State {
    scale_bits: u32,
    hd: bool,
    cels: BTreeMap<String, BTreeMap<usize, CelImage>>,
    /// Each sheet's measured HD style (`None`: it can't be measured, so it
    /// keeps its bitmap glyphs).
    styles: BTreeMap<String, Option<SheetStyle>>,
    /// Each sheet's number for the run layout ([`OutlineGlyph::sheet`]).
    sheet_ids: BTreeMap<String, u32>,
    /// The outline glyphs by their place in the atlas (page, x, y).
    outlines: BTreeMap<(u32, u32, u32), OutlineGlyph>,
    /// Outline glyphs drawn at a phase between output pixels, by sheet,
    /// character and phase step ([`hd_fonts::PHASES`]).
    phased: BTreeMap<(u32, u16, u8), CelImage>,
    atlas: Arc<RepAtlas>,
    x: u32,
    y: u32,
    row_h: u32,
}

#[derive(Default)]
pub struct HdTextArt {
    state: Mutex<State>,
    /// The look this art draws regardless of the setting (tests and
    /// captures); `None`: the setting's ([`hd_fonts`]).
    look: Option<bool>,
}

fn next_atlas_id() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

impl HdTextArt {
    pub fn new() -> Self {
        Self::default()
    }

    /// An art that always draws one look: HD Text on (`true`) or off.
    pub fn with_hd_fonts(on: bool) -> Self {
        Self {
            look: Some(on),
            ..Self::default()
        }
    }

    /// Puts a `w` × `h` image into the atlas, `texel(x, y)` giving each of
    /// its texels (palette index, coverage). Its page and top-left.
    fn place(
        st: &mut State,
        w: u32,
        h: u32,
        texel: impl Fn(u32, u32) -> [u8; 2],
    ) -> Option<(u32, u32, u32)> {
        if w > REP_PAGE || h > REP_PAGE {
            return None;
        }
        if st.x + w > REP_PAGE {
            st.x = 0;
            st.y += st.row_h;
            st.row_h = 0;
        }
        let atlas = Arc::make_mut(&mut st.atlas);
        if atlas.pages.is_empty() || st.y + h > REP_PAGE {
            if atlas.pages.len() >= MAX_PAGES {
                return None;
            }
            atlas.pages.push(RepAtlas::blank_page());
            st.x = 0;
            st.y = 0;
            st.row_h = 0;
        }
        let page = atlas.pages.len() - 1;
        let (x, y) = (st.x, st.y);
        let dst = &mut atlas.pages[page];
        for row in 0..h {
            for col in 0..w {
                let [index, cover] = texel(col, row);
                let at = (((y + row) * REP_PAGE + x + col) * 4) as usize;
                dst[at..at + 4].copy_from_slice(&[index, 0, 0, cover]);
            }
        }
        st.x += w;
        st.row_h = st.row_h.max(h);
        atlas.id = next_atlas_id();
        Some((page as u32, x, y))
    }

    /// Glyph `index` of `sheet` drawn from the outline font, when the font
    /// has its character and the sheet can be measured.
    fn render_hd(
        st: &mut State,
        sheet: &dyn FontSheet,
        index: usize,
        cel: GlyphCel<'_>,
        scale: f32,
    ) -> Option<CelImage> {
        let font = hd_fonts::font()?;
        let table = sheet.table()?;
        if !st.styles.contains_key(sheet.path()) {
            let style = SheetStyle::measure(sheet, table, font);
            st.styles.insert(sheet.path().to_owned(), style);
        }
        st.styles.get(sheet.path())?.as_ref()?;
        let record = table
            .glyphs
            .iter()
            .find(|g| usize::from(g.frame) == index)?;
        let space = table
            .glyphs
            .iter()
            .find(|g| g.code == u16::from(b' '))
            .map_or(0, |g| g.width);
        let next_id = st.sheet_ids.len() as u32;
        let sheet_id = *st
            .sheet_ids
            .entry(sheet.path().to_owned())
            .or_insert(next_id);
        let style = st.styles.get(sheet.path())?.as_ref()?;
        let image = hd_fonts::glyph_image(
            font,
            style,
            record.code,
            record.width,
            (cel.left, cel.top),
            scale,
            sheet_id,
            space,
            0.0,
        )?;
        let (w, h) = (image.width, image.height);
        let (page, x, y) =
            Self::place(st, w, h, |col, row| image.texels[(row * w + col) as usize])?;
        st.outlines.insert((page, x, y), image.outline);
        Some(CelImage::IndexedGlyph {
            page,
            rect: [x, y, w, h],
            scale,
            origin: image.origin,
            nudge: 0.0,
        })
    }

    fn render(st: &mut State, cel: GlyphCel<'_>, scale: f32) -> Option<CelImage> {
        let (mut w, mut h) = (cel.width as usize, cel.height as usize);
        if w == 0
            || h == 0
            || cel.pixels.len() != w.checked_mul(h)?
            || !cel.pixels.iter().any(|&p| p != 0)
        {
            return None;
        }
        let mut pixels = cel.pixels.to_vec();
        let mut prescale = 1u32;
        while (prescale as f32) <= scale.max(1.0) {
            if w * 2 > REP_PAGE as usize || h * 2 > REP_PAGE as usize {
                return None;
            }
            pixels = scale2x(&pixels, w, h);
            w *= 2;
            h *= 2;
            prescale *= 2;
        }
        let (w, h) = (w as u32, h as u32);
        let (page, x, y) = Self::place(st, w, h, |col, row| {
            let index = pixels[(row * w + col) as usize];
            [index, if index == 0 { 0 } else { 255 }]
        })?;
        Some(CelImage::IndexedGlyph {
            page,
            rect: [x, y, w, h],
            scale: prescale as f32,
            origin: [0, 0],
            nudge: 0.0,
        })
    }

    pub fn glyph(&self, sheet: &dyn FontSheet, index: usize, scale: f32) -> CelImage {
        if !is_font_sheet(sheet.path()) || !scale.is_finite() || scale <= 0.0 {
            return CelImage::Original;
        }
        let hd = self.look.unwrap_or_else(hd_fonts);
        let mut st = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if st.scale_bits != scale.to_bits() || st.hd != hd {
            *st = State {
                scale_bits: scale.to_bits(),
                hd,
                ..State::default()
            };
        }
        if let Some(image) = st.cels.get(sheet.path()).and_then(|c| c.get(&index)) {
            return *image;
        }
        let Some(cel) = sheet.cel(index) else {
            return CelImage::Original;
        };
        let outline = if hd {
            Self::render_hd(&mut st, sheet, index, cel, scale)
        } else {
            None
        };
        let image = outline
            .or_else(|| Self::render(&mut st, cel, scale))
            .unwrap_or(CelImage::Original);
        st.cels
            .entry(sheet.path().to_owned())
            .or_default()
            .insert(index, image);
        image
    }

    /// `image` moved `shift` frame pixels across at `scale`: an outline glyph
    /// goes to the nearest of [`hd_fonts::PHASES`] steps per output pixel,
    /// drawn again at that phase (cached), the rest moving it by whole output
    /// pixels; any other image as [`CelImage::shifted`] moves it.
    pub fn shift_glyph(&self, image: CelImage, shift: f32, scale: f32) -> CelImage {
        let Some(o) = self.outline_of(&image) else {
            return image.shifted(shift, scale);
        };
        if !shift.is_finite() || !scale.is_finite() || scale <= 0.0 {
            return image;
        }
        let whole = shift.round();
        let rest = (shift - whole) * scale;
        let mut pixels = rest.floor();
        let mut step = ((rest - pixels) * f32::from(hd_fonts::PHASES)).round() as u8;
        if step >= hd_fonts::PHASES {
            pixels += 1.0;
            step = 0;
        }
        let base = if step == 0 {
            Some(image)
        } else {
            self.phased(o, step, scale)
        };
        match base {
            Some(CelImage::IndexedGlyph {
                page,
                rect,
                scale: s,
                origin,
                ..
            }) => CelImage::IndexedGlyph {
                page,
                rect,
                scale: s,
                origin: [origin[0] + whole as i32, origin[1]],
                nudge: pixels,
            },
            _ => image.shifted(shift, scale),
        }
    }

    /// Outline glyph `o` drawn at phase `step` of [`hd_fonts::PHASES`].
    fn phased(&self, o: OutlineGlyph, step: u8, scale: f32) -> Option<CelImage> {
        let font = hd_fonts::font()?;
        let mut st = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if st.scale_bits != scale.to_bits() {
            return None;
        }
        let key = (o.sheet, o.code, step);
        if let Some(image) = st.phased.get(&key) {
            return Some(*image);
        }
        let path = st
            .sheet_ids
            .iter()
            .find(|(_, &id)| id == o.sheet)
            .map(|(p, _)| p.clone())?;
        let style = st.styles.get(&path)?.clone()?;
        let image = hd_fonts::glyph_image(
            font,
            &style,
            o.code,
            u8::try_from(o.bitmap_advance).ok()?,
            o.cel,
            scale,
            o.sheet,
            u8::try_from(o.space_bitmap).unwrap_or(0),
            f32::from(step) / f32::from(hd_fonts::PHASES),
        )?;
        let (w, h) = (image.width, image.height);
        let (page, x, y) = Self::place(&mut st, w, h, |col, row| {
            image.texels[(row * w + col) as usize]
        })?;
        let out = CelImage::IndexedGlyph {
            page,
            rect: [x, y, w, h],
            scale,
            origin: image.origin,
            nudge: 0.0,
        };
        st.phased.insert(key, out);
        Some(out)
    }

    /// The run-layout data of an outline glyph image this art gave.
    pub fn outline_of(&self, image: &CelImage) -> Option<OutlineGlyph> {
        let CelImage::IndexedGlyph { page, rect, .. } = *image else {
            return None;
        };
        let st = self.state.lock().unwrap_or_else(|e| e.into_inner());
        st.outlines.get(&(page, rect[0], rect[1])).copied()
    }

    pub fn glyph_atlas(&self) -> Option<Arc<RepAtlas>> {
        let st = self.state.lock().unwrap_or_else(|e| e.into_inner());
        (!st.atlas.pages.is_empty()).then(|| st.atlas.clone())
    }
}

/// The one glyph cache the game and the front end share.
pub fn art() -> &'static HdTextArt {
    static ART: OnceLock<HdTextArt> = OnceLock::new();
    ART.get_or_init(HdTextArt::new)
}

impl UiArt for HdTextArt {
    fn cel(&self, cel: &CelRef<'_>, scale: f32) -> CelImage {
        let Some((key, index)) = cel.file else {
            return CelImage::Original;
        };
        self.glyph(&StoreSheet { key, cel }, index, scale)
    }
    fn atlas(&self) -> Option<Arc<RepAtlas>> {
        self.glyph_atlas()
    }
    fn outline(&self, image: &CelImage) -> Option<OutlineGlyph> {
        self.outline_of(image)
    }
    fn shift_glyph(&self, image: CelImage, shift: f32, scale: f32) -> CelImage {
        HdTextArt::shift_glyph(self, image, shift, scale)
    }
}

#[cfg(test)]
#[path = "hd_text_tests.rs"]
pub(crate) mod tests;
