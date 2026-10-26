//! Text images in both looks, from a made-up bitmap font and the shipped
//! outline font: no game files.
use super::*;
use d2_formats::font::Glyph;
use d2_formats::palette::Rgb;

/// A grey ramp: index i is (i, i, i).
fn grey() -> Palette {
    Palette {
        colors: std::array::from_fn(|i| Rgb {
            r: i as u8,
            g: i as u8,
            b: i as u8,
        }),
    }
}

/// A bitmap font of 6 × 10 white boxes (index 255) on 7-pixel advances,
/// the pen at the bottom of a 12-row cel; space has no ink.
fn boxes() -> BitmapFont {
    let mut glyphs = Vec::new();
    let mut cels = Vec::new();
    for (frame, code) in (u16::from(b' ')..=u16::from(b'z')).enumerate() {
        let mut pixels = vec![0u8; 6 * 12];
        if code != u16::from(b' ') {
            for y in 2..12 {
                for x in 0..6 {
                    pixels[y * 6 + x] = 255;
                }
            }
        }
        cels.push(BitmapCel {
            left: 0,
            top: -11,
            width: 6,
            height: 12,
            pixels,
        });
        glyphs.push(Glyph {
            code,
            unknown1: 0,
            width: 7,
            height: 12,
            unknown2: 0,
            unknown3: 0,
            frame: frame as u16,
            unknown5: 0,
        });
    }
    BitmapFont {
        table: FontTable {
            version: 1,
            unknown: 0,
            count: glyphs.len() as u16,
            height: 10,
            width: 7,
            glyphs,
        },
        cels,
    }
}

fn bitmap_images() -> TextImages {
    let fonts = TextRole::ALL
        .iter()
        .map(|r| (r.game_font(), boxes()))
        .collect();
    TextImages::with_fonts(fonts, grey()).with_look(false)
}

#[test]
fn every_role_has_a_game_font_and_an_hd_size() {
    for r in TextRole::ALL {
        let id = r.game_font();
        assert!(crate::ui::text::FONTS.iter().any(|f| f.id == id), "{r:?}");
        assert!(r.hd_em() > 10.0);
    }
    assert!(TextRole::Title.hd_em() > TextRole::Label.hd_em());
    // Only stone buttons keep the bitmap font's own colours.
    assert_eq!(TextRole::ALL.iter().filter(|r| r.own_colours()).count(), 1);
}

#[test]
fn hd_text_on_draws_the_shipped_font_in_the_colour_given() {
    let t = TextImages::without_game_files().with_look(true);
    let gold = [212, 176, 106];
    let img = t
        .render(TextRole::Label, "Scaling Filter", 3.0, gold)
        .expect("image");
    assert_eq!(img.rgba.len(), (img.width * img.height * 4) as usize);
    let w = t.measure(TextRole::Label, "Scaling Filter", 3.0).unwrap();
    assert!((img.width as f32 - w).abs() <= 3.0, "{} vs {w}", img.width);
    assert!(img.baseline > 0 && img.baseline < img.height);
    // Every inked pixel is the colour given; nothing below the baseline
    // but descenders (none here).
    for (i, p) in img.rgba.chunks(4).enumerate() {
        if p[3] > 0 {
            assert_eq!(&p[..3], &gold, "pixel {i}");
            assert!(((i as u32) / img.width) <= img.baseline + 1);
        }
    }
    // Twice the scale, about twice the width; a title is larger.
    let w6 = t.measure(TextRole::Label, "Scaling Filter", 6.0).unwrap();
    assert!((w6 / w - 2.0).abs() < 0.02);
    assert!(t.measure(TextRole::Title, "Scaling Filter", 3.0).unwrap() > w);
    assert!(t.render(TextRole::Body, "   ", 3.0, gold).is_none());
}

#[test]
fn hd_text_off_draws_the_game_font_at_its_advances() {
    let t = bitmap_images();
    let white = [255, 255, 255];
    for scale in [1.5f32, 2.0, 3.0] {
        let img = t
            .render(TextRole::Label, "AB", scale, white)
            .expect("image");
        // Two 7-pixel advances, 10 inked rows plus the empty top of the cel.
        assert_eq!(img.width, (14.0 * scale).round() as u32);
        assert_eq!(t.measure(TextRole::Label, "AB", scale), Some(14.0 * scale));
        assert_eq!(img.baseline, (12.0 * scale).round() as u32);
        // Opaque white inside the first letter.
        let at = |x: u32, y: u32| &img.rgba[((y * img.width + x) * 4) as usize..][..4];
        let mid_y = (7.0 * scale) as u32;
        assert_eq!(at((3.0 * scale) as u32, mid_y), &[255, 255, 255, 255]);
        // The gap between the letters is clear.
        assert_eq!(at((6.5 * scale) as u32, mid_y)[3], 0);
    }
}

#[test]
fn hd_text_off_tints_by_brightness_and_buttons_keep_their_colours() {
    let mut font = boxes();
    for c in &mut font.cels {
        for p in &mut c.pixels {
            if *p != 0 {
                *p = 128;
            }
        }
    }
    let fonts = TextRole::ALL
        .iter()
        .map(|r| (r.game_font(), font.clone()))
        .collect();
    let t = TextImages::with_fonts(fonts, grey()).with_look(false);
    let label = t.render(TextRole::Label, "A", 2.0, [200, 100, 0]).unwrap();
    let button = t.render(TextRole::Button, "A", 2.0, [200, 100, 0]).unwrap();
    let mid = |img: &TextImage| {
        let (x, y) = (img.width / 3, img.height / 2);
        img.rgba[((y * img.width + x) * 4) as usize..][..4].to_vec()
    };
    // Half brightness times the colour.
    assert_eq!(mid(&label), vec![100, 50, 0, 255]);
    // The palette's own colour.
    assert_eq!(mid(&button), vec![128, 128, 128, 255]);
}

#[test]
fn without_game_files_hd_text_off_has_no_image() {
    let t = TextImages::without_game_files().with_look(false);
    assert!(t.render(TextRole::Label, "A", 2.0, [255; 3]).is_none());
    assert!(t.measure(TextRole::Label, "A", 2.0).is_none());
}
