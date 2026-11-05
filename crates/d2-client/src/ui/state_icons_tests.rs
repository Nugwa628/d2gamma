//! The state icon row: not there without state rules, one icon per state the
//! local player has, the stack count only above one, the art from the mod's
//! `files/icons`.

use d2_sim::rules::{DataMod, DataModFiles, Ruleset};

use super::*;
use crate::bridge::world::{ClientUnit, ClientWorld, UnitKey, PLAYER};
use crate::mods::ClientMods;
use crate::ui::layout::Screen;
use crate::ui::original::{OriginalUi, UiConfig};
use crate::ui::{NoPanelRules, NoStrings, UiRoot};

const STATES: &str = r#"
[[state]]
id = 380
icon = "smolder"
max_stacks = 5
reapply = "add_stack"

[[state]]
id = 381
icon = "chill"

[[state]]
id = 382
"#;

fn ruleset(states: &str) -> Ruleset {
    let files = DataModFiles {
        states: Some(("states.toml", states)),
        ..DataModFiles::default()
    };
    DataMod::from_files("test", &files).unwrap().into_ruleset()
}

fn root_with(mods: Option<ClientMods>) -> UiRoot {
    let config = UiConfig {
        screen: Screen::R800,
        expansion_installed: true,
    };
    let mut ui = OriginalUi::new(config, None).unwrap();
    if let Some(m) = mods {
        ui.set_mods(m);
    }
    let mut root = UiRoot::new(Box::new(NoPanelRules));
    ui.install(&mut root).unwrap();
    root
}

/// A world whose local player has `states` with `stacks`.
fn world(states: &[u16], stacks: &[(u16, u16)]) -> ClientWorld {
    let mut w = ClientWorld::default();
    let key = UnitKey::new(PLAYER, 1);
    let mut u = ClientUnit::new(key);
    u.states.extend(states.iter().copied());
    u.state_stacks.extend(stacks.iter().copied());
    w.units.insert(key, u);
    w.local_player = Some(key);
    w
}

fn draws(root: &UiRoot, w: &ClientWorld) -> Vec<UiDraw> {
    let ctx = UiCtx {
        tick: 0,
        world: w,
        strings: &NoStrings,
    };
    let mut out: Vec<UiDraw> = Vec::new();
    root.draw(&ctx, &mut out);
    out
}

fn panel_draws(ui: &StateIconsUi, w: &ClientWorld) -> Vec<UiDraw> {
    let ctx = UiCtx {
        tick: 0,
        world: w,
        strings: &NoStrings,
    };
    let mut out: Vec<UiDraw> = Vec::new();
    ui.draw(&ctx, &mut out);
    out
}

fn strings(d: &[UiDraw]) -> Vec<String> {
    d.iter()
        .filter_map(|d| match d {
            UiDraw::Text(t) => Some(String::from_utf16_lossy(&t.text)),
            _ => None,
        })
        .collect()
}

fn rects(d: &[UiDraw]) -> Vec<RectRequest> {
    d.iter()
        .filter_map(|d| match d {
            UiDraw::Rect(r) => Some(*r),
            _ => None,
        })
        .collect()
}

fn defs() -> Vec<StateDef> {
    ruleset(STATES).state_rules().unwrap().defs().to_vec()
}

/// A panel with no icon files.
fn bare() -> StateIconsUi {
    StateIconsUi::new(&defs(), &[])
}

// No state rules, no panel: D2Gamma's root and draws are unchanged.
#[test]
fn without_state_rules_the_root_is_unchanged() {
    let plain = root_with(None);
    let empty = root_with(Some(ClientMods::new()));
    let no_states = root_with(Some(ClientMods::new().with_rules(ruleset(""))));
    assert_eq!(plain.open_panels(), empty.open_panels());
    assert_eq!(plain.open_panels(), no_states.open_panels());
    assert!(!plain.open_panels().contains(&STATE_ICONS_PANEL));
    let w = world(&[380], &[(380, 3)]);
    assert_eq!(draws(&plain, &w), draws(&no_states, &w));
}

#[test]
fn with_state_rules_the_panel_is_open_and_takes_no_input() {
    let root = root_with(Some(ClientMods::new().with_rules(ruleset(STATES))));
    assert!(root.open_panels().contains(&STATE_ICONS_PANEL));
    assert_ne!(
        root.hit(Point::new(10, 470)).map(|h| h.panel),
        Some(STATE_ICONS_PANEL)
    );
}

#[test]
fn a_state_with_stacks_draws_its_icon_and_count() {
    let root = root_with(Some(ClientMods::new().with_rules(ruleset(STATES))));
    let w = world(&[380], &[(380, 3)]);
    let d = draws(&root, &w);
    // The icon (a square with the letter) and the count.
    let ours = panel_draws(&bare(), &w);
    assert_eq!(strings(&ours), ["S", "3"]);
    assert_eq!(rects(&ours).len(), 1);
    assert!(ours.iter().all(|o| d.contains(o)));
}

#[test]
fn a_count_of_one_or_none_draws_no_number() {
    let one = world(&[380], &[(380, 1)]);
    assert_eq!(strings(&panel_draws(&bare(), &one)), ["S"]);
    let none = world(&[380], &[]);
    assert_eq!(strings(&panel_draws(&bare(), &none)), ["S"]);
}

#[test]
fn two_states_draw_two_icons_in_file_order() {
    // The player got 381 first; the row follows the file, not the order.
    let w = world(&[381, 380], &[(380, 2)]);
    let d = panel_draws(&bare(), &w);
    assert_eq!(strings(&d), ["S", "2", "C"]);
    let squares = rects(&d);
    assert_eq!(squares.len(), 2);
    assert!(squares[0].x1 < squares[1].x0, "{squares:?}");
    assert_eq!(squares[0].y1, squares[1].y1);
}

#[test]
fn states_without_an_icon_or_gone_draw_nothing() {
    // 382 has no icon; 380 is not on the player.
    let w = world(&[382], &[(382, 4)]);
    assert!(panel_draws(&bare(), &w).is_empty());
    let mut w = world(&[380], &[(380, 3)]);
    assert!(!panel_draws(&bare(), &w).is_empty());
    let u = w.units.get_mut(&UnitKey::new(PLAYER, 1)).unwrap();
    u.states.remove(&380);
    assert!(panel_draws(&bare(), &w).is_empty());
    // No local player: nothing.
    assert!(panel_draws(&bare(), &ClientWorld::default()).is_empty());
}

/// A greyscale PNG of `w` × `h` palette indices.
fn png_of(w: u32, h: u32, pixels: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut enc = png::Encoder::new(&mut out, w, h);
    enc.set_color(png::ColorType::Grayscale);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header()
        .unwrap()
        .write_image_data(pixels)
        .unwrap();
    out
}

#[test]
fn icon_files_decode_and_odd_ones_are_refused() {
    let art = decode_icon(&png_of(2, 2, &[0, 9, 9, 9])).unwrap();
    assert_eq!(
        (art.width, art.height, art.pixels),
        (2, 2, vec![0, 9, 9, 9])
    );
    assert!(decode_icon(b"not a png").is_none());
    // Bigger than 32 on a side.
    assert!(decode_icon(&png_of(33, 1, &[1; 33])).is_none());
    // A true-colour PNG is not a palette image.
    let mut rgb = Vec::new();
    let mut enc = png::Encoder::new(&mut rgb, 1, 1);
    enc.set_color(png::ColorType::Rgb);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header()
        .unwrap()
        .write_image_data(&[1, 2, 3])
        .unwrap();
    assert!(decode_icon(&rgb).is_none());
}

#[test]
fn the_icon_file_of_the_mod_is_drawn_as_runs() {
    let dir = std::env::temp_dir().join(format!("d2-state-icons-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("files/icons")).unwrap();
    // Row 0: two opaque pixels of one value; row 1: a gap, then two values.
    std::fs::write(
        dir.join("files/icons/smolder.png"),
        png_of(3, 2, &[7, 7, 0, 0, 8, 9]),
    )
    .unwrap();
    let ui = StateIconsUi::new(&defs(), std::slice::from_ref(&dir));
    let w = world(&[380], &[(380, 2)]);
    let d = panel_draws(&ui, &w);
    let r = rects(&d);
    assert_eq!(
        r.iter().map(|r| (r.x1 - r.x0, r.color)).collect::<Vec<_>>(),
        [(2, 7), (1, 8), (1, 9)]
    );
    // The art stands on the row's bottom; the count follows it.
    assert_eq!(r[1].y1, ROW_BOTTOM);
    assert_eq!(strings(&d), ["2"]);
    // A missing file is the fallback; a path in the name is refused.
    assert!(load_icon(std::slice::from_ref(&dir), "chill").is_none());
    assert!(load_icon(std::slice::from_ref(&dir), "../smolder").is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

/// 3×5 glyphs of the characters the row draws in the test below, one row of
/// three bits (the high bit is the left pixel) per line.
fn glyph(c: char) -> [u8; 5] {
    match c {
        '0' => [7, 5, 5, 5, 7],
        '1' => [2, 6, 2, 2, 7],
        '2' => [7, 1, 7, 4, 7],
        '3' => [7, 1, 7, 1, 7],
        '4' => [5, 5, 7, 1, 1],
        '5' => [7, 4, 7, 1, 7],
        'C' => [7, 4, 4, 4, 7],
        'S' => [7, 4, 7, 1, 7],
        _ => [0; 5],
    }
}

/// A colour for palette value `i`: the game's palette is not needed to see
/// that the runs of one value stand together.
fn shade(i: u8) -> [u8; 3] {
    [i.wrapping_mul(53), i.wrapping_mul(97), i.wrapping_mul(29)]
}

/// The row's draws as a picture of the area around it, `SCALE` times the
/// size, the way the game draws it (rectangles by palette value; the text the
/// game sets in its font is set in a 3×5 pixel font here).
fn picture(d: &[UiDraw]) -> image::RgbImage {
    const SCALE: u32 = 8;
    const LEFT: i32 = 0;
    const TOP: i32 = 450;
    const W: u32 = 100;
    const H: u32 = 40;
    let mut img = image::RgbImage::from_pixel(W * SCALE, H * SCALE, image::Rgb([22, 22, 28]));
    let mut dot = |x: i32, y: i32, c: [u8; 3]| {
        let (x, y) = (x - LEFT, y - TOP);
        if (0..W as i32).contains(&x) && (0..H as i32).contains(&y) {
            for dy in 0..SCALE {
                for dx in 0..SCALE {
                    img.put_pixel(x as u32 * SCALE + dx, y as u32 * SCALE + dy, image::Rgb(c));
                }
            }
        }
    };
    for draw in d {
        match draw {
            UiDraw::Rect(r) => {
                for y in r.y0..r.y1 {
                    for x in r.x0..r.x1 {
                        dot(x, y, shade(r.color));
                    }
                }
            }
            UiDraw::Text(t) => {
                // The text's point is its baseline.
                for (n, c) in String::from_utf16_lossy(&t.text).chars().enumerate() {
                    for (row, bits) in glyph(c).into_iter().enumerate() {
                        for col in 0..3 {
                            if bits & (4 >> col) != 0 {
                                let x = t.at.x + n as i32 * 4 + col;
                                dot(x, t.at.y - 5 + row as i32, [255, 255, 255]);
                            }
                        }
                    }
                }
            }
            UiDraw::Image(_) | UiDraw::Line(_) => {}
        }
    }
    img
}

// The row as a picture: one state with the mod's own icon file and four
// stacks, one with the fallback square and no number. When the run names a
// folder (`D2_K6_RENDER_DIR`) the picture is written there.
#[test]
fn the_row_renders_to_a_picture() {
    let dir = std::env::temp_dir().join(format!("d2-state-render-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("files/icons")).unwrap();
    // A 20 × 20 flame: a rim of one value around a body of two others.
    let mut flame = Vec::new();
    for y in 0..20i32 {
        for x in 0..20i32 {
            let (dx, dy) = ((x - 10).abs(), y - 4);
            let inside = dy >= 0 && dx * 2 <= dy + 2 && dx * dx + (dy - 10) * (dy - 10) < 140;
            flame.push(match (inside, dx + (dy - 12).abs() < 8) {
                (false, _) => 0,
                (true, true) => 9,
                (true, false) => 5,
            });
        }
    }
    std::fs::write(dir.join("files/icons/smolder.png"), png_of(20, 20, &flame)).unwrap();
    let ui = StateIconsUi::new(&defs(), std::slice::from_ref(&dir));
    let w = world(&[380, 381], &[(380, 4)]);
    let img = picture(&panel_draws(&ui, &w));
    // The icon and the fallback square are there, and so is the number.
    let lit = |x0: u32, x1: u32| {
        (0..img.height())
            .flat_map(|y| (x0 * 8..x1 * 8).map(move |x| (x, y)))
            .filter(|&(x, y)| img.get_pixel(x, y).0 != [22, 22, 28])
            .count()
    };
    assert!(lit(6, 26) > 0, "the icon art");
    assert!(lit(29, 49) > 0, "the fallback square");
    let white = img.pixels().filter(|p| p.0 == [255, 255, 255]).count();
    assert!(white > 0, "the stack number and the letter");
    if let Some(out) = std::env::var_os("D2_K6_RENDER_DIR") {
        let out = std::path::PathBuf::from(out);
        std::fs::create_dir_all(&out).unwrap();
        img.save(out.join("state-icons-row.png")).unwrap();
        let single = picture(&panel_draws(&ui, &world(&[380], &[(380, 1)])));
        single.save(out.join("state-icons-one-stack.png")).unwrap();
        let none = picture(&panel_draws(&ui, &world(&[], &[])));
        none.save(out.join("state-icons-none.png")).unwrap();
    }
    let _ = std::fs::remove_dir_all(&dir);
}
