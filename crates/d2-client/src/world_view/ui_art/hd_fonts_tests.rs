//! HD Text's outline glyphs against a made-up bitmap font: no game files.
use super::*;
use crate::world_view::ui_art::hd_text::{GlyphCel, HdTextArt};
use crate::world_view::ui_art::CelImage;
use d2_formats::font::Glyph;

/// Rim and fill indices of the made-up font.
const RIM: u8 = 15;
const FILL: u8 = 31;
/// Its cels: 12 × 16, the pen at the bottom-left (`top` = -15).
const CEL_W: u32 = 12;
const CEL_H: u32 = 16;
const TOP: i32 = -15;
/// Its capital: rows 3..=12 of the cel (10 rows, rim included).
const CAP_TOP: usize = 3;
const CAP_BOTTOM: usize = 12;

/// A bitmap font whose every letter is a filled box with a one-pixel rim,
/// `advance` pixels apart; `rim` false gives a plain box.
struct Bitmap {
    table: FontTable,
    cels: Vec<Vec<u8>>,
    path: &'static str,
}

impl Bitmap {
    fn new(advance: u8, rim: bool) -> Self {
        Self::with_advances(|_| advance, rim)
    }

    /// A font whose letter `c` advances `advance(c)` pixels (at most the
    /// cel width).
    fn with_advances(advance: impl Fn(u8) -> u8, rim: bool) -> Self {
        // One record per code, in code order, as the game's tables have.
        let codes: Vec<u16> = (0..=255).collect();
        let mut glyphs = Vec::new();
        let mut cels = Vec::new();
        for (frame, &code) in codes.iter().enumerate() {
            let advance = advance(code as u8).clamp(3, CEL_W as u8);
            let mut px = vec![0u8; (CEL_W * CEL_H) as usize];
            if code > u16::from(b' ') && code <= u16::from(b'z') {
                let (x0, x1) = (0usize, usize::from(advance) - 2);
                for y in CAP_TOP..=CAP_BOTTOM {
                    for x in x0..=x1 {
                        let edge = y == CAP_TOP || y == CAP_BOTTOM || x == x0 || x == x1;
                        px[y * CEL_W as usize + x] = if edge && rim { RIM } else { FILL };
                    }
                }
            }
            cels.push(px);
            glyphs.push(Glyph {
                code,
                unknown1: 0,
                width: advance,
                height: CEL_H as u8,
                unknown2: 0,
                unknown3: 0,
                frame: frame as u16,
                unknown5: 0,
            });
        }
        Bitmap {
            table: FontTable {
                version: 1,
                unknown: 0,
                count: glyphs.len() as u16,
                height: 10,
                width: 9,
                glyphs,
            },
            cels,
            path: "data/local/font/latin/font16.dc6",
        }
    }

    fn frame_of(&self, c: char) -> usize {
        usize::from(
            self.table
                .glyphs
                .iter()
                .find(|g| g.code == c as u16)
                .unwrap()
                .frame,
        )
    }
}

impl FontSheet for Bitmap {
    fn path(&self) -> &str {
        self.path
    }
    fn table(&self) -> Option<&FontTable> {
        Some(&self.table)
    }
    fn cel(&self, index: usize) -> Option<GlyphCel<'_>> {
        Some(GlyphCel {
            width: CEL_W,
            height: CEL_H,
            left: 0,
            top: TOP,
            pixels: self.cels.get(index)?,
        })
    }
}

#[test]
fn the_shipped_font_loads_and_has_the_game_characters() {
    let f = font().expect("the shipped font parses");
    for c in ('A'..='Z').chain('a'..='z').chain('0'..='9') {
        assert!(glyph_for(f, c as u16).is_some(), "no glyph for {c}");
    }
    for c in "!?.,:;'-+%()/".chars() {
        assert!(glyph_for(f, c as u16).is_some(), "no glyph for {c}");
    }
    // Control codes and the colour-code lead draw nothing.
    assert!(glyph_for(f, 0x0A).is_none());
    assert_eq!(HD_FONT.name, "Marcellus SC");
}

#[test]
fn a_bitmap_font_is_measured_from_its_capital_and_colours() {
    let f = font().unwrap();
    let rimmed = SheetStyle::measure(&Bitmap::new(9, true), &Bitmap::new(9, true).table, f)
        .expect("measured");
    // The baseline is the row below the "H", pen-relative.
    assert_eq!(rimmed.baseline, TOP + CAP_BOTTOM as i32 + 1);
    assert!(rimmed.rim, "a dark rim around light letters is a rim");
    // The rim is drawn around the outline glyph: its capital is 2 less.
    assert!(rimmed.cap <= 8.0 && rimmed.cap > 6.0, "cap {}", rimmed.cap);
    assert_eq!((rimmed.fill, rimmed.edge), (FILL, RIM));

    let plain = SheetStyle::measure(&Bitmap::new(9, false), &Bitmap::new(9, false).table, f)
        .expect("measured");
    assert!(!plain.rim);
    assert!(plain.cap > rimmed.cap);
    assert_eq!(plain.fill, FILL);
}

#[test]
fn a_narrow_bitmap_font_squeezes_the_outline_font() {
    let f = font().unwrap();
    let wide = Bitmap::new(11, false);
    let narrow = Bitmap::new(6, false);
    let w = SheetStyle::measure(&wide, &wide.table, f).unwrap();
    let n = SheetStyle::measure(&narrow, &narrow.table, f).unwrap();
    assert!(
        w.squeeze >= 1.0 && w.squeeze <= MAX_STRETCH,
        "{}",
        w.squeeze
    );
    assert!(n.squeeze < 1.0 && n.squeeze >= MIN_SQUEEZE);
    assert!(n.cap <= w.cap);
}

/// The inked texels (coverage above half) of an image: their box in frame
/// pixels from the pen, (x0, y0, x1, y1).
fn ink_box(image: &GlyphImage, cel: (i32, i32), scale: f32) -> (f32, f32, f32, f32) {
    let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for (i, t) in image.texels.iter().enumerate() {
        if t[1] < 128 {
            continue;
        }
        let (x, y) = (
            (i as u32 % image.width) as f32,
            (i as u32 / image.width) as f32,
        );
        x0 = x0.min(x);
        y0 = y0.min(y);
        x1 = x1.max(x + 1.0);
        y1 = y1.max(y + 1.0);
    }
    let ox = (image.origin[0] + cel.0) as f32;
    let oy = (image.origin[1] + cel.1) as f32;
    (
        ox + x0 / scale,
        oy + y0 / scale,
        ox + x1 / scale,
        oy + y1 / scale,
    )
}

#[test]
fn an_outline_glyph_sits_on_the_bitmap_baseline_inside_its_advance() {
    let f = font().unwrap();
    for rim in [false, true] {
        let sheet = Bitmap::new(10, rim);
        let style = SheetStyle::measure(&sheet, &sheet.table, f).unwrap();
        for scale in [1.5f32, 2.0, 3.0] {
            let image =
                glyph_image(f, &style, u16::from(b'H'), 10, (0, TOP), scale, 0, 10, 0.0).unwrap();
            let (x0, y0, x1, y1) = ink_box(&image, (0, TOP), scale);
            // Bottom on the baseline and top at the capital, within a pixel.
            let base = style.baseline as f32;
            assert!(
                (y1 - base).abs() <= 1.0,
                "rim {rim} at {scale}: bottom {y1} vs {base}"
            );
            let cap = (CAP_BOTTOM - CAP_TOP + 1) as f32;
            assert!(
                ((y1 - y0) - cap).abs() <= 1.5,
                "rim {rim} at {scale}: {} vs {cap}",
                y1 - y0
            );
            // Centred in the advance.
            let centre = (x0 + x1) / 2.0;
            assert!(
                (centre - 5.0).abs() <= 1.0,
                "rim {rim} at {scale}: centre {centre}"
            );
            // Only the bitmap font's own indices.
            assert!(image
                .texels
                .iter()
                .all(|t| t[1] == 0 || t[0] == FILL || (rim && t[0] == RIM)));
            if rim {
                assert!(image.texels.iter().any(|t| t[0] == RIM && t[1] > 128));
            }
        }
    }
}

#[test]
fn hd_text_draws_outline_glyphs_and_off_keeps_the_bitmap_look() {
    let sheet = Bitmap::new(10, true);
    let h = sheet.frame_of('H');
    let on = HdTextArt::with_hd_fonts(true);
    let off = HdTextArt::with_hd_fonts(false);
    let a = on.glyph(&sheet, h, 3.0);
    let b = off.glyph(&sheet, h, 3.0);
    let CelImage::IndexedGlyph {
        scale: on_scale,
        origin,
        ..
    } = a
    else {
        panic!("HD Text on draws an indexed outline glyph: {a:?}");
    };
    assert_eq!(on_scale, 3.0, "drawn at the output scale");
    assert_ne!(origin, [0, 0], "an outline glyph has its own box");
    let CelImage::IndexedGlyph {
        scale: off_scale,
        origin: off_origin,
        ..
    } = b
    else {
        panic!("HD Text off enhances the bitmap glyph: {b:?}");
    };
    assert_eq!((off_scale, off_origin), (4.0, [0, 0]));
    // Cached: the same image again.
    assert_eq!(on.glyph(&sheet, h, 3.0), a);
    // A space draws nothing either way.
    assert_eq!(
        on.glyph(&sheet, sheet.frame_of(' '), 3.0),
        CelImage::Original
    );
}

#[test]
fn a_sheet_without_a_glyph_table_keeps_its_bitmap_glyphs() {
    struct NoTable(Bitmap);
    impl FontSheet for NoTable {
        fn path(&self) -> &str {
            self.0.path
        }
        fn table(&self) -> Option<&FontTable> {
            None
        }
        fn cel(&self, index: usize) -> Option<GlyphCel<'_>> {
            self.0.cel(index)
        }
    }
    let sheet = NoTable(Bitmap::new(10, true));
    let h = sheet.0.frame_of('H');
    let image = HdTextArt::with_hd_fonts(true).glyph(&sheet, h, 3.0);
    assert!(matches!(
        image,
        CelImage::IndexedGlyph {
            origin: [0, 0],
            scale: 4.0,
            ..
        }
    ));
}

/// The shipped font's licence goes wherever the font goes: beside the file,
/// in the notices, and in the README every release carries.
#[test]
fn the_shipped_font_carries_its_licence() {
    let ofl = include_str!("../../../fonts/marcellus-sc/OFL.txt").replace("\r\n", "\n");
    assert!(ofl.contains("SIL OPEN FONT LICENSE Version 1.1"));
    assert!(HD_FONT.licence.contains("Open Font License 1.1"));
    let readme = include_str!("../../../../../tools/windows/README-PLAY.txt").replace("\r\n", "\n");
    assert!(
        readme.contains(ofl.trim_end()),
        "README-PLAY.txt lacks the OFL text"
    );
    let notices = include_str!("../../../../../THIRD_PARTY_NOTICES.md");
    assert!(notices.contains(HD_FONT.name) && notices.contains("Open Font License"));
}

/// Bitmap advances as uneven as the game fonts' (a light-text font of
/// about this size: wide U, narrow I and S), letters only; space 8.
fn uneven(c: u8) -> u8 {
    match c {
        b'S' | b'B' | b's' | b'b' | b'l' => 7,
        b'U' | b'u' => 12,
        b'R' | b'D' | b'o' => 10,
        b'P' | b'r' | b't' => 9,
        b'I' | b'i' => 5,
        b'E' | b'L' | b'd' => 8,
        b'T' | b'O' => 11,
        b' ' => 8,
        _ => 9,
    }
}

/// A set line: each letter's ink across, and the bitmap line's extent.
type SetLine = (Vec<(f32, f32)>, (i32, i32));

/// Lays `lines` (each: its text and its row) out as the bitmap layout
/// does, one glyph per letter at its bitmap pen (spaces draw nothing), and
/// returns per line each letter's ink box across (frame pixels) with the
/// run shifts applied (`shifted`) or not, and the bitmap line's extent.
fn set_lines(sheet: &Bitmap, lines: &[(&str, i32)], scale: f32, shifted: bool) -> Vec<SetLine> {
    let f = font().unwrap();
    let style = SheetStyle::measure(sheet, &sheet.table, f).unwrap();
    let adv = |c: u8| {
        i32::from(
            sheet
                .table
                .glyphs
                .iter()
                .find(|g| g.code == u16::from(c))
                .unwrap()
                .width,
        )
    };
    let mut placed = Vec::new();
    let mut extents = Vec::new();
    for (k, &(text, y)) in lines.iter().enumerate() {
        let mut pen = 0;
        for c in text.bytes() {
            if c != b' ' {
                let image = glyph_image(
                    f,
                    &style,
                    u16::from(c),
                    adv(c) as u8,
                    (0, TOP),
                    scale,
                    0,
                    8,
                    0.0,
                )
                .unwrap();
                placed.push((k, pen, y, image));
            }
            pen += adv(c);
        }
        extents.push((0, pen));
    }
    let runs: Vec<_> = placed
        .iter()
        .map(|(_, x, y, i)| (*x, *y, i.outline))
        .collect();
    let shifts = run_shifts(f, &runs);
    let mut out: Vec<SetLine> = extents.into_iter().map(|e| (Vec::new(), e)).collect();
    for ((k, x, _, image), shift) in placed.iter().zip(shifts) {
        // The letter's ink across, from its outline (exact, not texels).
        let g = image.outline;
        let b = f.outline(g.glyph).unwrap().bounds;
        let pen = *x as f32
            + (g.bitmap_advance as f32 - g.advance) / 2.0
            + if shifted { shift } else { 0.0 };
        out[*k]
            .0
            .push((pen + b.min.x * g.kern_scale, pen + b.max.x * g.kern_scale));
    }
    out
}

#[test]
fn a_run_covers_its_bitmap_line_and_never_crosses_a_line_break() {
    let sheet = Bitmap::with_advances(uneven, false);
    let f = font().unwrap();
    let lines = [("SURPRISED TO SEE", 0), ("THE RECENT TROUBLES", 16)];
    for scale in [2.0f32, 3.0] {
        // Run by run: each line on its own gives the same shifts as the two
        // together (a run ends at the line break)...
        let style = SheetStyle::measure(&sheet, &sheet.table, f).unwrap();
        let glyphs = |text: &str, y: i32| -> Vec<(i32, i32, OutlineGlyph)> {
            let mut pen = 0;
            let mut out = Vec::new();
            for c in text.bytes() {
                let a = uneven(c);
                if c != b' ' {
                    let g = glyph_image(f, &style, u16::from(c), a, (0, TOP), scale, 0, 8, 0.0)
                        .unwrap();
                    out.push((pen, y, g.outline));
                }
                pen += i32::from(a);
            }
            out
        };
        let (a, b) = (glyphs(lines[0].0, 0), glyphs(lines[1].0, 16));
        let both: Vec<_> = a.iter().chain(&b).copied().collect();
        let mut apart = run_shifts(f, &a);
        apart.extend(run_shifts(f, &b));
        assert_eq!(run_shifts(f, &both), apart);
        // ...and the run's letters, at their own advances plus the even
        // share, span exactly the bitmap line: the same margin before the
        // first letter's advance as after the last one's.
        for run in [&a, &b] {
            let s = run_shifts(f, run);
            let pen = |i: usize| {
                let (x, _, g) = run[i];
                x as f32 + (g.bitmap_advance as f32 - g.advance) / 2.0 + s[i]
            };
            let (first, last) = (run[0], run[run.len() - 1]);
            let n = run.len() - 1;
            let before = pen(0) - first.0 as f32;
            let after = (last.0 + last.2.bitmap_advance) as f32 - (pen(n) + last.2.advance);
            assert!((before - after).abs() < 1e-3, "{before} vs {after}");
        }
        // The ink of every line stays inside the bitmap line, give or take
        // the outline letters' side bearings.
        for (ink, (x0, x1)) in set_lines(&sheet, &lines, scale, true) {
            assert!(
                ink[0].0 >= x0 as f32 - 1.0,
                "starts {} before {x0}",
                ink[0].0
            );
            assert!(
                ink[ink.len() - 1].1 <= x1 as f32 + 1.0,
                "ends {} after {x1}",
                ink[ink.len() - 1].1
            );
        }
    }
}

/// The ink across of each letter of `word` set with the outline font's own
/// advances and kerning alone (its designer's spacing), at the sizes
/// `sheet` gives it.
fn natural_ink(sheet: &Bitmap, word: &str, scale: f32) -> Vec<(f32, f32)> {
    let f = font().unwrap();
    let style = SheetStyle::measure(sheet, &sheet.table, f).unwrap();
    let mut pen = 0f32;
    let mut prev: Option<OutlineGlyph> = None;
    let mut out = Vec::new();
    for c in word.bytes() {
        let g = glyph_image(
            f,
            &style,
            u16::from(c),
            uneven(c),
            (0, TOP),
            scale,
            0,
            8,
            0.0,
        )
        .unwrap()
        .outline;
        if let Some(p) = prev {
            pen += p.advance + f.kern_unscaled(p.glyph, g.glyph) * g.kern_scale;
        }
        let b = f.outline(g.glyph).unwrap().bounds;
        out.push((pen + b.min.x * g.kern_scale, pen + b.max.x * g.kern_scale));
        prev = Some(g);
    }
    out
}

/// The gaps between the letters of one word.
fn gaps(ink: &[(f32, f32)]) -> Vec<f32> {
    ink.windows(2).map(|w| w[1].0 - w[0].1).collect()
}

/// The spread (standard deviation) of `v`.
fn spread(v: &[f32]) -> f32 {
    let mean = v.iter().sum::<f32>() / v.len() as f32;
    (v.iter().map(|g| (g - mean) * (g - mean)).sum::<f32>() / v.len() as f32).sqrt()
}

#[test]
fn letters_in_a_run_have_even_gaps() {
    let sheet = Bitmap::with_advances(uneven, false);
    for scale in [2.0f32, 3.0] {
        for word in ["SURPRISED", "TROUBLES"] {
            let laid = gaps(&set_lines(&sheet, &[(word, 0)], scale, true)[0].0);
            let alone = gaps(&set_lines(&sheet, &[(word, 0)], scale, false)[0].0);
            let natural = gaps(&natural_ink(&sheet, word, scale));
            // Against the font's own spacing, every gap of the run differs by
            // the same share: the even spread leaves no letter out of step.
            let off =
                |g: &[f32]| -> Vec<f32> { g.iter().zip(&natural).map(|(a, b)| a - b).collect() };
            let (laid_off, centred_off) = (spread(&off(&laid)), spread(&off(&alone)));
            assert!(
                laid_off < 0.01,
                "{word} at {scale}: {laid_off} frame px out of step"
            );
            // Centring letters in their bitmap advances put them out of step
            // by a large part of a pixel.
            assert!(
                centred_off > 0.3,
                "{word} at {scale}: centred {centred_off}"
            );
            // And the gaps themselves are no less even than the font's own.
            assert!(
                spread(&laid) <= spread(&natural) + 1e-3,
                "{word} at {scale}: {} vs the font's {}",
                spread(&laid),
                spread(&natural)
            );
        }
    }
}

#[test]
fn a_shift_moves_whole_pixels_by_the_origin_and_the_rest_by_output_pixels() {
    let image = CelImage::IndexedGlyph {
        page: 0,
        rect: [0, 0, 30, 30],
        scale: 3.0,
        origin: [-2, -1],
        nudge: 0.0,
    };
    let CelImage::IndexedGlyph { origin, nudge, .. } = image.shifted(2.4, 3.0) else {
        panic!()
    };
    assert_eq!((origin, nudge), ([0, -1], 1.0));
    let CelImage::IndexedGlyph { origin, nudge, .. } = image.shifted(-1.6, 3.0) else {
        panic!()
    };
    assert_eq!((origin, nudge), ([-4, -1], 1.0));
    assert_eq!(CelImage::Original.shifted(2.0, 3.0), CelImage::Original);
}

/// The coverage-weighted centre across of a glyph image, output pixels from
/// its cel's left at `scale` (its origin and nudge included).
fn centre_out(art: &HdTextArt, image: CelImage, scale: f32) -> f32 {
    let CelImage::IndexedGlyph {
        page,
        rect,
        origin,
        nudge,
        ..
    } = image
    else {
        panic!("not a glyph image: {image:?}");
    };
    let atlas = art.glyph_atlas().unwrap();
    let (mut sum, mut weight) = (0f32, 0f32);
    for y in 0..rect[3] {
        for x in 0..rect[2] {
            let a = atlas.texel(page, i64::from(rect[0] + x), i64::from(rect[1] + y))[3];
            sum += a * (x as f32 + 0.5);
            weight += a;
        }
    }
    origin[0] as f32 * scale + nudge + sum / weight
}

#[test]
fn a_letter_moved_in_its_run_lands_within_an_eighth_of_an_output_pixel() {
    let sheet = Bitmap::new(10, false);
    let art = HdTextArt::with_hd_fonts(true);
    let scale = 3.0;
    let base = art.glyph(&sheet, sheet.frame_of('H'), scale);
    let at = centre_out(&art, base, scale);
    for shift in [0.1f32, 0.37, -0.6, 1.25, -2.9] {
        let moved = art.shift_glyph(base, shift, scale);
        let got = centre_out(&art, moved, scale) - at;
        // An eighth of a pixel from the phase step, plus the rasterizer's
        // own rounding of the edges.
        assert!(
            (got - shift * scale).abs() < 0.2,
            "shift {shift}: moved {got}"
        );
    }
    // Drawn again at most once per phase step.
    let again = art.shift_glyph(base, 0.37, scale);
    assert_eq!(again, art.shift_glyph(base, 0.37, scale));
}

/// A paragraph wrapped by the game's word wrap (bitmap advances) keeps its
/// breaks with HD Text: every wrapped line is one run of its own, and each
/// run covers exactly its line's bitmap width.
#[test]
fn wrapped_lines_keep_their_breaks_and_widths() {
    use crate::ui::text::{line_width, wrap, GlyphLookup};
    let sheet = Bitmap::with_advances(uneven, false);
    let f = font().unwrap();
    let style = SheetStyle::measure(&sheet, &sheet.table, f).unwrap();
    let text: Vec<u16> = "Greetings, stranger. I'm not surprised to see your kind here. \
                          Many adventurers have traveled this way since the recent troubles began."
        .encode_utf16()
        .collect();
    let g = GlyphLookup::new(&sheet.table);
    let lines = wrap(&g, &text, 180).unwrap();
    assert!(lines.len() >= 3, "{} lines", lines.len());
    let scale = 3.0;
    let mut all = Vec::new();
    let mut per_line = Vec::new();
    for (k, line) in lines.iter().enumerate() {
        let y = 16 * k as i32;
        let mut pen = 0;
        let mut glyphs = Vec::new();
        for &u in *line {
            let c = u as u8;
            if c != b' ' {
                let img = glyph_image(f, &style, u, uneven(c), (0, TOP), scale, 0, 8, 0.0).unwrap();
                glyphs.push((pen, y, img.outline));
            }
            pen += i32::from(uneven(c));
        }
        assert_eq!(
            pen,
            line_width(&g, line, 0).unwrap(),
            "line {k}: bitmap width"
        );
        all.extend(glyphs.iter().copied());
        per_line.push(glyphs);
    }
    // The breaks: the shifts of the whole paragraph are those of each line
    // on its own (no run reaches across a break).
    let shifts = run_shifts(f, &all);
    let mut at = 0;
    for glyphs in &per_line {
        let own = run_shifts(f, glyphs);
        assert_eq!(&shifts[at..at + glyphs.len()], &own[..]);
        at += glyphs.len();
        // The width: the line's letters, at their own advances plus the
        // even share, span from its first letter's pen to its last letter's
        // end, as the bitmap letters do.
        let pen = |i: usize| {
            let (x, _, g) = glyphs[i];
            x as f32 + (g.bitmap_advance as f32 - g.advance) / 2.0 + own[i]
        };
        let (first, last) = (glyphs[0], glyphs[glyphs.len() - 1]);
        let before = pen(0) - first.0 as f32;
        let after =
            (last.0 + last.2.bitmap_advance) as f32 - (pen(glyphs.len() - 1) + last.2.advance);
        assert!((before - after).abs() < 1e-3, "{before} vs {after}");
    }
}

/// Two strings of one row, a list's columns ("cows", then "Norm"), whose
/// gap happens to be a whole number of spaces: each is laid out as its own
/// run, exactly as when drawn alone. Laid out as one run, the outline
/// letters (narrower than the bitmap ones) were spread over the gap, "c o
/// w s   N o r m".
#[test]
fn two_strings_of_one_row_are_never_one_run() {
    use crate::world_view::ui_art::{lay_out_runs, UiArt};
    let sheet = Bitmap::with_advances(uneven, false);
    let art = HdTextArt::with_hd_fonts(true);
    let scale = 3.0;
    // The strings' glyph images at their pens: "cows" from 0, "Norm" four
    // spaces after its end.
    let word = |text: &str, from: i32| -> Vec<(i32, CelImage)> {
        let mut pen = from;
        let mut out = Vec::new();
        for c in text.bytes() {
            out.push((pen, art.glyph(&sheet, sheet.frame_of(c as char), scale)));
            pen += i32::from(uneven(c));
        }
        out
    };
    let cows = word("cows", 0);
    let end: i32 = "cows".bytes().map(|c| i32::from(uneven(c))).sum();
    let norm = word("Norm", end + 4 * i32::from(uneven(b' ')));
    let laid = |strings: &[(u64, &Vec<(i32, CelImage)>)]| -> Vec<CelImage> {
        let mut images: Vec<CelImage> = strings
            .iter()
            .flat_map(|(_, w)| w.iter().map(|g| g.1))
            .collect();
        let at: Vec<(u64, i32)> = strings
            .iter()
            .flat_map(|(t, w)| w.iter().map(move |g| (*t, g.0)))
            .collect();
        let mut glyphs: Vec<_> = at
            .iter()
            .zip(images.iter_mut())
            .map(|(&(t, x), i)| (t, x, 0, i))
            .collect();
        lay_out_runs(&art as &dyn UiArt, &mut glyphs, scale);
        images
    };
    let mut apart = laid(&[(0, &cows)]);
    apart.extend(laid(&[(1, &norm)]));
    assert_eq!(laid(&[(0, &cows), (1, &norm)]), apart);
    // The gap alone would join them: as one string they are one run, set
    // differently.
    assert_ne!(laid(&[(0, &cows), (0, &norm)]), apart);
}
