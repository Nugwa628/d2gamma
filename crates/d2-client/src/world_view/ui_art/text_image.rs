//! Text as finished images, for screens drawn outside the game frame (the
//! settings menu and the Ctrl+O overlay): a string in a text role, at the
//! screen's scale, in a colour, as straight RGBA.
//!
//! The look follows HD Text (`[video] hd_text`, [`super::hd_text::hd_fonts`]),
//! like all other text:
//! - **On:** D2RUST's shipped outline font ([`super::hd_fonts::HD_FONT`]) at
//!   the role's size.
//! - **Off:** the game's own bitmap font for the role, read from the
//!   player's game files, enhanced like in-game text (Scale2x up to the
//!   scale, then smoothed down to it).
//!
//! Sizes are in 1280 × 720 view pixels and multiplied by the scale (output
//! pixels per view pixel: 3 at 2160p, 2 at 1440p, 1.5 at 1080p).

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, OnceLock};

use ab_glyph::{point, Font, PxScale, ScaleFont};
use d2_formats::font::FontTable;
use d2_formats::palette::Palette;

use super::hd_fonts;
use super::hd_text::{hd_fonts as hd_text_on, scale2x};
use crate::assets::path::FileSource;

/// What a piece of text is, which sets its font and size.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum TextRole {
    /// Screen titles.
    Title,
    /// Labels, tabs, values and key caps.
    Label,
    /// Descriptions and long text.
    Body,
    /// Labels on stone buttons.
    Button,
}

impl TextRole {
    pub const ALL: [TextRole; 4] = [
        TextRole::Title,
        TextRole::Label,
        TextRole::Body,
        TextRole::Button,
    ];

    /// The game's bitmap font for the role with HD Text off (an id of
    /// `ui::text::FONTS`): Font30, Font16, FontFormal10, FontExocet10.
    pub fn game_font(self) -> u16 {
        match self {
            TextRole::Title => 2,
            TextRole::Label => 1,
            TextRole::Body => 4,
            TextRole::Button => 9,
        }
    }

    /// The outline font's em size for the role with HD Text on, view pixels.
    pub fn hd_em(self) -> f32 {
        match self {
            TextRole::Title => 70.0 / 3.0,
            TextRole::Label | TextRole::Body | TextRole::Button => 40.0 / 3.0,
        }
    }

    /// Whether the bitmap font keeps its own colours (engraved button
    /// labels) instead of taking the caller's.
    pub fn own_colours(self) -> bool {
        self == TextRole::Button
    }
}

/// A rendered string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextImage {
    pub width: u32,
    pub height: u32,
    /// Rows of the image above the baseline: strings of different roles on
    /// one line align on it.
    pub baseline: u32,
    /// Straight (not premultiplied) RGBA, top row first.
    pub rgba: Vec<u8>,
}

/// A bitmap font: its glyph table and cels (left, top from the pen;
/// width, height; indices top row first).
#[derive(Debug, Clone)]
pub struct BitmapFont {
    pub table: FontTable,
    pub cels: Vec<BitmapCel>,
}

#[derive(Debug, Clone)]
pub struct BitmapCel {
    pub left: i32,
    pub top: i32,
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

impl BitmapFont {
    /// The font of a glyph table and its DC6 sheet.
    pub fn from_dc6(table: FontTable, dc6: &d2_formats::dc6::Dc6) -> Self {
        let cels = dc6
            .frames
            .iter()
            .map(|f| BitmapCel {
                left: f.offset_x,
                top: f.offset_y - f.height as i32 + 1,
                width: f.width,
                height: f.height,
                pixels: f.pixels.clone(),
            })
            .collect();
        BitmapFont { table, cels }
    }

    /// Character `code`'s advance and cel.
    fn glyph(&self, code: u16) -> Option<(u8, &BitmapCel)> {
        let g = self.table.glyphs.iter().find(|g| g.code == code)?;
        Some((g.width, self.cels.get(usize::from(g.frame))?))
    }

    /// The baseline: the row below "H", from the pen.
    fn baseline(&self) -> i32 {
        let Some((_, h)) = self.glyph(u16::from(b'H')) else {
            return 0;
        };
        let w = h.width as usize;
        let last = (0..h.height as usize).rev().find(|&y| {
            h.pixels
                .get(y * w..(y + 1) * w)
                .is_some_and(|r| r.iter().any(|&p| p != 0))
        });
        last.map_or(0, |y| h.top + y as i32 + 1)
    }
}

/// The game's character code for `c` (`?` for one the fonts lack).
fn code_of(c: char) -> u16 {
    u16::try_from(u32::from(c))
        .ok()
        .filter(|&v| v < 256)
        .unwrap_or(u16::from(b'?'))
}

/// Where the role fonts come from with HD Text off.
enum Fonts {
    /// The player's game files, read on first use.
    Files(Arc<dyn FileSource>),
    /// Given fonts and palette (tests and tools).
    Given,
    /// None: HD Text off draws nothing.
    None,
}

/// Renders text images by role ([`TextRole`]) in the look HD Text sets.
pub struct TextImages {
    fonts: Fonts,
    loaded: Mutex<BTreeMap<u16, Option<Arc<BitmapFont>>>>,
    palette: OnceLock<Option<Palette>>,
    /// The look regardless of the setting (`None`: the setting's).
    look: Option<bool>,
}

/// The palette the bitmap fonts' own colours are shown in.
const PALETTE: &str = r"data\global\palette\act1\pal.dat";

impl TextImages {
    /// Text images from the player's game files (`source`).
    pub fn new(source: Arc<dyn FileSource>) -> Self {
        Self {
            fonts: Fonts::Files(source),
            loaded: Mutex::default(),
            palette: OnceLock::new(),
            look: None,
        }
    }

    /// Text images without game files: HD Text off gives nothing.
    pub fn without_game_files() -> Self {
        Self {
            fonts: Fonts::None,
            loaded: Mutex::default(),
            palette: OnceLock::new(),
            look: None,
        }
    }

    /// Text images from given bitmap fonts (by `TextRole::game_font` id)
    /// and palette.
    pub fn with_fonts(fonts: Vec<(u16, BitmapFont)>, palette: Palette) -> Self {
        let loaded = fonts
            .into_iter()
            .map(|(id, f)| (id, Some(Arc::new(f))))
            .collect();
        let p = OnceLock::new();
        let _ = p.set(Some(palette));
        Self {
            fonts: Fonts::Given,
            loaded: Mutex::new(loaded),
            palette: p,
            look: None,
        }
    }

    /// Always draws one look: HD Text on (`true`) or off.
    pub fn with_look(mut self, hd: bool) -> Self {
        self.look = Some(hd);
        self
    }

    fn hd(&self) -> bool {
        self.look.unwrap_or_else(hd_text_on)
    }

    fn bitmap(&self, role: TextRole) -> Option<Arc<BitmapFont>> {
        let id = role.game_font();
        let mut loaded = self.loaded.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(f) = loaded.get(&id) {
            return f.clone();
        }
        let Fonts::Files(source) = &self.fonts else {
            return None;
        };
        let info = crate::ui::text::FONTS.iter().find(|f| f.id == id)?;
        let font = (|| {
            let table = crate::assets::path::read_font_table(&**source, info.tbl_path)?.ok()?;
            let dc6 = crate::assets::path::read_dc6(&**source, info.dc6_path)?.ok()?;
            Some(Arc::new(BitmapFont::from_dc6(table, &dc6)))
        })();
        loaded.insert(id, font.clone());
        font
    }

    fn palette(&self) -> Option<&Palette> {
        self.palette
            .get_or_init(|| match &self.fonts {
                Fonts::Files(source) => crate::assets::path::read_palette(&**source, PALETTE)?.ok(),
                _ => None,
            })
            .as_ref()
    }

    /// `text` in `role` at `scale` output pixels per view pixel, coloured
    /// `colour` (a button label with HD Text off keeps its own colours).
    /// `None` when there is no font for the role or `text` draws nothing.
    pub fn render(
        &self,
        role: TextRole,
        text: &str,
        scale: f32,
        colour: [u8; 3],
    ) -> Option<TextImage> {
        if !scale.is_finite() || scale <= 0.0 {
            return None;
        }
        if self.hd() {
            render_outline(role, text, scale, colour)
        } else {
            let font = self.bitmap(role)?;
            let tint = (!role.own_colours()).then_some(colour);
            render_bitmap(&font, self.palette()?, text, scale, tint)
        }
    }

    /// The width `render` gives `text`, output pixels.
    pub fn measure(&self, role: TextRole, text: &str, scale: f32) -> Option<f32> {
        if self.hd() {
            let font = hd_fonts::font()?;
            let px = outline_scale(font, role, scale)?;
            Some(outline_width(font, px, text))
        } else {
            let font = self.bitmap(role)?;
            let w: u32 = text
                .chars()
                .filter_map(|c| font.glyph(code_of(c)).map(|g| u32::from(g.0)))
                .sum();
            Some(w as f32 * scale)
        }
    }
}

/// The outline font's pixel scale for `role` at `scale`.
fn outline_scale(font: &ab_glyph::FontRef<'_>, role: TextRole, scale: f32) -> Option<PxScale> {
    let em = role.hd_em() * scale;
    Some(PxScale::from(
        em * font.height_unscaled() / font.units_per_em()?,
    ))
}

/// The pen advance of `text` in the outline font, kerning included.
fn outline_width(font: &ab_glyph::FontRef<'_>, px: PxScale, text: &str) -> f32 {
    let f = font.as_scaled(px);
    let mut x = 0.0;
    let mut last = None;
    for c in text.chars() {
        let id = f.glyph_id(c);
        if let Some(prev) = last {
            x += f.kern(prev, id);
        }
        x += f.h_advance(id);
        last = Some(id);
    }
    x
}

fn render_outline(role: TextRole, text: &str, scale: f32, colour: [u8; 3]) -> Option<TextImage> {
    let font = hd_fonts::font()?;
    let px = outline_scale(font, role, scale)?;
    let f = font.as_scaled(px);
    let pad = 1.0;
    let width = (outline_width(font, px, text) + 2.0 * pad).ceil() as u32;
    let baseline = (f.ascent() + pad).ceil() as u32;
    let height = baseline + (-f.descent() + pad).ceil() as u32;
    let mut cover = vec![0f32; (width * height) as usize];
    let mut x = pad;
    let mut last = None;
    for c in text.chars() {
        let id = f.glyph_id(c);
        if let Some(prev) = last {
            x += f.kern(prev, id);
        }
        last = Some(id);
        let glyph = id.with_scale_and_position(px, point(x, baseline as f32));
        x += f.h_advance(id);
        let Some(o) = font.outline_glyph(glyph) else {
            continue;
        };
        let b = o.px_bounds();
        o.draw(|gx, gy, c| {
            let (ix, iy) = (gx as i64 + b.min.x as i64, gy as i64 + b.min.y as i64);
            if (0..i64::from(width)).contains(&ix) && (0..i64::from(height)).contains(&iy) {
                let at = (iy as u32 * width + ix as u32) as usize;
                cover[at] = (cover[at] + c).min(1.0);
            }
        });
    }
    if !cover.iter().any(|&c| c > 0.0) {
        return None;
    }
    let rgba = cover
        .iter()
        .flat_map(|&c| {
            let a = (c * 255.0).round() as u8;
            if a == 0 {
                [0; 4]
            } else {
                [colour[0], colour[1], colour[2], a]
            }
        })
        .collect();
    Some(TextImage {
        width,
        height,
        baseline,
        rgba,
    })
}

/// `text` in a bitmap font, enhanced to `scale`: the glyphs' own colours
/// under `palette`, or (`tint`) their brightness times that colour.
fn render_bitmap(
    font: &BitmapFont,
    palette: &Palette,
    text: &str,
    scale: f32,
    tint: Option<[u8; 3]>,
) -> Option<TextImage> {
    // The line at 1x: each cel at its pen position, the highest cel top at
    // row 0.
    let glyphs: Vec<(i32, &BitmapCel)> = {
        let mut pen = 0i32;
        let mut out = Vec::new();
        for c in text.chars() {
            if let Some((adv, cel)) = font.glyph(code_of(c)) {
                out.push((pen, cel));
                pen += i32::from(adv);
            }
        }
        out
    };
    let advance: i32 = text
        .chars()
        .filter_map(|c| font.glyph(code_of(c)).map(|g| i32::from(g.0)))
        .sum();
    let top = glyphs.iter().map(|(_, c)| c.top).min()?;
    let bottom = glyphs.iter().map(|(_, c)| c.top + c.height as i32).max()?;
    let right = glyphs
        .iter()
        .map(|(x, c)| x + c.left + c.width as i32)
        .max()?
        .max(advance);
    let (w, h) = (right.max(1) as usize, (bottom - top).max(1) as usize);
    let mut line = vec![0u8; w * h];
    for (x, cel) in &glyphs {
        let (cw, ch) = (cel.width as usize, cel.height as usize);
        for row in 0..ch {
            for col in 0..cw {
                let p = cel.pixels.get(row * cw + col).copied().unwrap_or(0);
                let (lx, ly) = (x + cel.left + col as i32, cel.top - top + row as i32);
                if p != 0 && (0..w as i32).contains(&lx) && (0..h as i32).contains(&ly) {
                    line[ly as usize * w + lx as usize] = p;
                }
            }
        }
    }
    if !line.iter().any(|&p| p != 0) {
        return None;
    }
    // Enhanced to a whole prescale above the scale.
    let (mut pw, mut ph, mut px) = (w, h, line);
    let mut prescale = 1usize;
    while (prescale as f32) <= scale.max(1.0) && pw * ph < 1 << 22 {
        px = scale2x(&px, pw, ph);
        pw *= 2;
        ph *= 2;
        prescale *= 2;
    }
    // Colours, premultiplied, at the prescale.
    let rgba_of = |i: u8| -> [f32; 4] {
        if i == 0 {
            return [0.0; 4];
        }
        let c = palette.colors[usize::from(i)];
        let (r, g, b) = match tint {
            None => (c.r, c.g, c.b),
            Some(t) => {
                // The letters are drawn in the palette's greys: their
                // brightness scales the colour.
                let l = f32::from(c.r.max(c.g).max(c.b)) / 255.0;
                let f = |v: u8| (f32::from(v) * l).round() as u8;
                (f(t[0]), f(t[1]), f(t[2]))
            }
        };
        [f32::from(r), f32::from(g), f32::from(b), 255.0]
    };
    // Box-filtered down to the output size.
    let (ow, oh) = (
        (w as f32 * scale).round().max(1.0) as u32,
        (h as f32 * scale).round().max(1.0) as u32,
    );
    let k = prescale as f32 / scale;
    let mut rgba = Vec::with_capacity((ow * oh * 4) as usize);
    for oy in 0..oh {
        for ox in 0..ow {
            let (sx0, sy0) = (ox as f32 * k, oy as f32 * k);
            let (sx1, sy1) = (sx0 + k, sy0 + k);
            let mut acc = [0f32; 4];
            let mut area = 0f32;
            for sy in sy0.floor() as usize..(sy1.ceil() as usize).min(ph) {
                let wy = (sy1.min(sy as f32 + 1.0) - sy0.max(sy as f32)).max(0.0);
                for sx in sx0.floor() as usize..(sx1.ceil() as usize).min(pw) {
                    let wx = (sx1.min(sx as f32 + 1.0) - sx0.max(sx as f32)).max(0.0);
                    let c = rgba_of(px[sy * pw + sx]);
                    let a = c[3] / 255.0;
                    for ch in 0..3 {
                        acc[ch] += c[ch] * a * wx * wy;
                    }
                    acc[3] += c[3] * wx * wy;
                    area += wx * wy;
                }
            }
            if area <= 0.0 || acc[3] <= 0.0 {
                rgba.extend_from_slice(&[0; 4]);
                continue;
            }
            let a = acc[3] / area;
            let un = |v: f32| (v / area / (a / 255.0)).round().clamp(0.0, 255.0) as u8;
            rgba.extend_from_slice(&[un(acc[0]), un(acc[1]), un(acc[2]), a.round() as u8]);
        }
    }
    let baseline = ((font.baseline() - top).max(0) as f32 * scale).round() as u32;
    Some(TextImage {
        width: ow,
        height: oh,
        baseline,
        rgba,
    })
}

#[cfg(test)]
#[path = "text_image_tests.rs"]
mod tests;
