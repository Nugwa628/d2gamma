//! The realm lobby's background: a still picture under the lobby's panels
//! on the 16:9 front-end area ([`crate::ui::front_end::FrontEnd::wide`]).
//! Nothing of a game runs behind the lobby.
//!
//! - **Still** ([`Shows::Still`]): the chosen background's baked picture
//!   (`super::backdrop_bake`, the place drawn once from the player's own
//!   files), cut to the area's shape ([`crop_to`]: a line or so of the
//!   16:9 picture) and shown at the background's brightness
//!   (`backdrop_bake::exponent`). A background picked in the picker swaps
//!   the still.
//! - **Menu art** ([`Shows::MenuArt`]), while the chosen still is missing
//!   (not baked yet, or the bake failed): the main menu's own art from the
//!   player's files (`FrontEnd\gameselectscreenEXP`, 800 × 600, in the
//!   Sky palette), scaled to the area's height and centred ([`fit_rect`]).
//!   The width beside it is always filled with a dim blurred copy of the
//!   same art stretched to cover the area ([`cover_rect`]), as the 4:3
//!   menus' `"blur"` sides are (`super::front_fill`), whatever
//!   `[video] menu_sides` says (owner 2026-10-25: no black sides here).
//!   Without that art (a mod's own files) the picture is black.
//! - **Bake**: while the front end is up, the stills bake is started once
//!   per launch if a still is missing ([`StillsBake`]); when it ends the
//!   lobby takes the new stills.
//! - **Shown** ([`LobbyPicture`]): the front end lays its UI over the
//!   picture (`front_host::front_ui_present`); made again when the choice
//!   or the canvas changes, gone when no 16:9 screen is up.

use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

use super::backdrop_bake::{self, StillsBake};
use super::front_fill::{self, FILL_H, FILL_W};
use super::front_host::{FrontHost, HEIGHT, WIDTH};
use crate::ui::front_end::screens::main_menu::BACKGROUND;
use crate::ui::front_end::screens::online::backdrop::Backdrop;
use crate::ui::front_end::SKY_PALETTE;

/// What the lobby's picture shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shows {
    /// The background's baked still.
    Still(Backdrop),
    /// The main menu's art on its blurred cover.
    MenuArt,
}

/// The lobby's picture as the front end shows it: its image (sRGB), the
/// exponent on it (1: as it is), what it shows and the canvas size it
/// was made for.
#[derive(Resource, Clone, Debug, PartialEq)]
pub struct LobbyPicture {
    pub image: Handle<Image>,
    pub exponent: f32,
    pub shows: Shows,
    pub size: (u32, u32),
}

/// What the lobby shows on a screen that is `wide` (16:9) with `chosen`
/// picked, its still present or not; `None`: no picture (a 4:3 screen
/// draws its own art).
pub fn wanted(wide: bool, chosen: Backdrop, still: bool) -> Option<Shows> {
    wide.then_some(if still {
        Shows::Still(chosen)
    } else {
        Shows::MenuArt
    })
}

/// Keeps [`LobbyPicture`] in step with the screen drawn and the stills
/// bake going (module docs). Runs after the front end's frame.
pub fn drive(world: &mut World) {
    let Some(host) = world.get_non_send::<FrontHost>() else {
        return;
    };
    let canvas = host.front.canvas();
    let size = (canvas.width.max(1) as u32, canvas.height.max(1) as u32);
    let (wide, chosen) = (host.front.wide(), host.front.settings().lobby_background);
    let still = match world.get_resource_mut::<StillsBake>() {
        Some(mut bake) => {
            bake.poll();
            wide.then(|| bake.still(chosen)).flatten()
        }
        None => None,
    };
    let Some(shows) = wanted(wide, chosen, still.is_some()) else {
        world.remove_resource::<LobbyPicture>();
        return;
    };
    let current = world.get_resource::<LobbyPicture>();
    if current.is_some_and(|p| p.shows == shows && p.size == size) {
        return;
    }
    let made = match (shows, still) {
        (Shows::Still(b), Some(path)) => match backdrop_bake::read_png(&path) {
            Ok((rgba, from)) => {
                let (rgba, cut) = crop(&rgba, from, crop_to(from, size));
                Some((rgba, cut, backdrop_bake::exponent(b.place().gamma), shows))
            }
            Err(e) => {
                warn!("lobby picture: {e}");
                if let Some(mut bake) = world.get_resource_mut::<StillsBake>() {
                    bake.unreadable(b);
                }
                None
            }
        },
        _ => None,
    };
    let (rgba, image_size, exponent, shows) = made.unwrap_or_else(|| {
        let art = world
            .non_send_mut::<FrontHost>()
            .art
            .as_mut()
            .and_then(|a| a.backdrop_rgb(BACKGROUND, SKY_PALETTE));
        let rgba = match art {
            Some(art) => compose(&art, (WIDTH, HEIGHT), size),
            None => black(size),
        };
        (rgba, size, 1.0, Shows::MenuArt)
    });
    let Some(mut images) = world.get_resource_mut::<Assets<Image>>() else {
        return;
    };
    let image = images.add(picture_image(rgba, image_size));
    world.insert_resource(LobbyPicture {
        image,
        exponent,
        shows,
        size,
    });
}

/// The part of a `src` picture with `dst`'s shape, centred: left, top,
/// width, height in `src`'s pixels.
pub fn crop_to(src: (u32, u32), dst: (u32, u32)) -> (u32, u32, u32, u32) {
    let (sw, sh) = (u64::from(src.0), u64::from(src.1));
    let (dw, dh) = (u64::from(dst.0.max(1)), u64::from(dst.1.max(1)));
    let (w, h) = if sw * dh > sh * dw {
        ((sh * dw + dh / 2) / dh, sh)
    } else {
        (sw, (sw * dh + dw / 2) / dw)
    };
    let (w, h) = (w.clamp(1, sw.max(1)), h.clamp(1, sh.max(1)));
    (
        ((sw - w) / 2) as u32,
        ((sh - h) / 2) as u32,
        w as u32,
        h as u32,
    )
}

/// The `rect` of `rgba` (`size`), and its size.
fn crop(rgba: &[u8], size: (u32, u32), rect: (u32, u32, u32, u32)) -> (Vec<u8>, (u32, u32)) {
    let (x, y, w, h) = rect;
    if (x, y, w, h) == (0, 0, size.0, size.1) {
        return (rgba.to_vec(), size);
    }
    let row = size.0 as usize * 4;
    let mut out = Vec::with_capacity(w as usize * h as usize * 4);
    for r in y..y + h {
        let from = r as usize * row + x as usize * 4;
        out.extend_from_slice(&rgba[from..from + w as usize * 4]);
    }
    (out, (w, h))
}

/// An sRGB image of `rgba` at `size`, read by the nearest texel as the
/// world view's picture is (the presentation's own filter scales it).
pub fn picture_image(rgba: Vec<u8>, size: (u32, u32)) -> Image {
    let mut image = Image::new(
        Extent3d {
            width: size.0,
            height: size.1,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        rgba,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    image.sampler = bevy::image::ImageSampler::nearest();
    image
}

fn black(size: (u32, u32)) -> Vec<u8> {
    [0, 0, 0, 255].repeat((size.0 * size.1) as usize)
}

/// A rect inside an area: left, top, width, height (any may overflow).
pub type Placed = (i64, i64, i64, i64);

/// `src` scaled to fit inside `dst` at its own shape, centred (the art:
/// bars on the longer axis).
pub fn fit_rect(src: (u32, u32), dst: (u32, u32)) -> Placed {
    place(src, dst, false)
}

/// `src` scaled to cover all of `dst` at its own shape, centred (the
/// blurred sides: the rest is cut off).
pub fn cover_rect(src: (u32, u32), dst: (u32, u32)) -> Placed {
    place(src, dst, true)
}

fn place(src: (u32, u32), dst: (u32, u32), cover: bool) -> Placed {
    let (sw, sh) = (i64::from(src.0.max(1)), i64::from(src.1.max(1)));
    let (dw, dh) = (i64::from(dst.0), i64::from(dst.1));
    // Width-bound when dst is the narrower shape (fit) or the wider (cover).
    let by_width = (dw * sh < dh * sw) != cover;
    let (w, h) = if by_width {
        (dw, (sh * dw + sw / 2) / sw)
    } else {
        ((sw * dh + sh / 2) / sh, dh)
    };
    ((dw - w) / 2, (dh - h) / 2, w, h)
}

/// The lobby's picture: `art` (RGB, `art_size`) fitted and centred in a
/// `size` canvas over the art's blurred, dimmed grid
/// ([`front_fill::blur`]) stretched to cover the canvas, filtered.
pub fn compose(art: &[[u8; 3]], art_size: (u32, u32), size: (u32, u32)) -> Vec<u8> {
    let (w, h) = (size.0 as usize, size.1 as usize);
    let at = |x: u32, y: u32| {
        art.get((y.min(art_size.1 - 1) * art_size.0 + x.min(art_size.0 - 1)) as usize)
            .copied()
            .unwrap_or_default()
    };
    let grid = front_fill::blur(at);
    let (gx, gy, gw, gh) = cover_rect((FILL_W, FILL_H), size);
    let (ax, ay, aw, ah) = fit_rect(art_size, size);
    let mut out = Vec::with_capacity(w * h * 4);
    for y in 0..h as i64 {
        for x in 0..w as i64 {
            let c = if (ax..ax + aw).contains(&x) && (ay..ay + ah).contains(&y) {
                let sx = (x - ax) * i64::from(art_size.0) / aw.max(1);
                let sy = (y - ay) * i64::from(art_size.1) / ah.max(1);
                at(sx as u32, sy as u32)
            } else {
                // The grid cell under the pixel's centre, bilinear.
                let fx = ((x - gx) as f32 + 0.5) * FILL_W as f32 / gw.max(1) as f32 - 0.5;
                let fy = ((y - gy) as f32 + 0.5) * FILL_H as f32 / gh.max(1) as f32 - 0.5;
                bilinear(&grid, fx, fy)
            };
            out.extend_from_slice(&[c[0], c[1], c[2], 255]);
        }
    }
    out
}

/// The [`FILL_W`] × [`FILL_H`] RGBA grid at cell coordinates (`fx`, `fy`),
/// filtered between the four nearest cells (edges repeated).
fn bilinear(grid: &[u8], fx: f32, fy: f32) -> [u8; 3] {
    let (gw, gh) = (FILL_W as i32, FILL_H as i32);
    let (x0, y0) = (fx.floor(), fy.floor());
    let (tx, ty) = (fx - x0, fy - y0);
    let cell = |x: i32, y: i32, k: usize| {
        let (x, y) = (x.clamp(0, gw - 1), y.clamp(0, gh - 1));
        f32::from(grid[((y * gw + x) * 4) as usize + k])
    };
    let (x0, y0) = (x0 as i32, y0 as i32);
    std::array::from_fn(|k| {
        let top = cell(x0, y0, k) * (1.0 - tx) + cell(x0 + 1, y0, k) * tx;
        let bottom = cell(x0, y0 + 1, k) * (1.0 - tx) + cell(x0 + 1, y0 + 1, k) * tx;
        (top * (1.0 - ty) + bottom * ty).round().clamp(0.0, 255.0) as u8
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_still_shows_when_present_else_the_menu_art() {
        let b = Backdrop::Tristram;
        assert_eq!(wanted(false, b, true), None, "a 4:3 screen has none");
        assert_eq!(wanted(false, b, false), None);
        assert_eq!(wanted(true, b, true), Some(Shows::Still(b)));
        assert_eq!(wanted(true, b, false), Some(Shows::MenuArt));
    }

    #[test]
    fn a_still_is_cut_to_the_canvas_shape() {
        // The 16:9 still on the 1068 x 600 canvas: a line off.
        assert_eq!(crop_to((1280, 720), (1068, 600)), (0, 0, 1280, 719));
        assert_eq!(crop_to((1280, 720), (1280, 720)), (0, 0, 1280, 720));
        // A 4:3 area: the middle of the width.
        assert_eq!(crop_to((1280, 720), (800, 600)), (160, 0, 960, 720));
        // A taller still on a wide area: the middle rows.
        assert_eq!(crop_to((100, 100), (200, 100)), (0, 25, 100, 50));
        let rgba: Vec<u8> = (0..4 * 3).flat_map(|i| [i as u8, 0, 0, 255]).collect();
        let (cut, size) = crop(&rgba, (4, 3), (1, 1, 2, 1));
        assert_eq!(size, (2, 1));
        assert_eq!(cut, vec![5, 0, 0, 255, 6, 0, 0, 255]);
    }

    #[test]
    fn the_art_fits_centred_and_the_blur_covers() {
        // The lobby's 16:9 canvas: the 800 x 600 art at its height, 134
        // columns each side.
        assert_eq!(fit_rect((800, 600), (1068, 600)), (134, 0, 800, 600));
        assert_eq!(fit_rect((800, 600), (3840, 2160)), (480, 0, 2880, 2160));
        // A taller area: bars above and below.
        assert_eq!(fit_rect((800, 600), (800, 800)), (0, 100, 800, 600));
        // Same shape: the whole area.
        assert_eq!(fit_rect((800, 600), (1600, 1200)), (0, 0, 1600, 1200));
        assert_eq!(cover_rect((800, 600), (1600, 1200)), (0, 0, 1600, 1200));
        // The 4:3 grid over 16:9: as wide as the area, cut above and below.
        let (x, y, w, h) = cover_rect((40, 30), (1068, 600));
        assert_eq!((x, w), (0, 1068));
        assert_eq!(h, 801);
        assert_eq!(y, (600 - 801) / 2);
        // A 16:9 still over the 1068 x 600 canvas: cut by a line or so.
        let (x, y, w, h) = cover_rect((2560, 1440), (1068, 600));
        assert!(x <= 0 && y <= 0 && w >= 1068 && h >= 600);
        assert!(w * 600 - h * 1068 < 1068, "the shape kept");
    }

    #[test]
    fn the_picture_centres_the_art_on_its_blurred_cover() {
        let art: Vec<[u8; 3]> = (0..800 * 600)
            .map(|i| {
                if i % 800 < 400 {
                    [200, 40, 40]
                } else {
                    [40, 40, 200]
                }
            })
            .collect();
        let rgba = compose(&art, (800, 600), (1068, 600));
        assert_eq!(rgba.len(), 1068 * 600 * 4);
        let px = |x: usize, y: usize| {
            let i = (y * 1068 + x) * 4;
            [rgba[i], rgba[i + 1], rgba[i + 2], rgba[i + 3]]
        };
        assert_eq!(px(134, 300), [200, 40, 40, 255], "the art's left");
        assert_eq!(px(933, 0), [40, 40, 200, 255], "the art's right");
        // The sides carry the art's colours on, dimmed: never black.
        let left = px(10, 300);
        let right = px(1060, 300);
        assert!(left[0] > left[2] && left[0] < 200, "{left:?}");
        assert!(right[2] > right[0] && right[2] < 200, "{right:?}");
        assert!(
            (0..600).all(|y| px(0, y)[..3] != [0, 0, 0]),
            "no black edge"
        );
    }
}
