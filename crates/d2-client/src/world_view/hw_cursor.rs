//! The mouse pointer at display rate: the game's cursor cel shown as the
//! window's hardware cursor.
//!
//! The game frame is drawn only on display frames where a game tick ran,
//! at the game's tick rate. A cursor drawn into that frame would move only
//! that often, which feels sluggish on a fast monitor. Instead, on play
//! windows driven by the real mouse, the cursor cel the UI draws last
//! ([`crate::ui::original::cursor_ui`]) is taken out of the frame's draw
//! list, turned into an RGBA image with the frame's palette, scaled
//! (nearest) to the player's Cursor Size ([`set_size`]: Auto is the
//! picture's scale rounded to a whole number), and handed to the window as
//! a custom cursor. The operating system then moves it at the monitor's refresh rate with no added
//! latency. Which cel and frame the cursor shows (its type and
//! animation) is still decided by the UI's cursor machine on each drawn
//! frame; only where it is shown changes.
//!
//! An item held on the cursor is the cursor: the UI draws the item's
//! graphic centred on the mouse instead of a pointer cel, so that draw
//! becomes the hardware cursor the same way, with the item's colour
//! remap applied and the pointer on the graphic's centre. The window only
//! gets a new cursor image when the picture changes (another item, other
//! cursor art, a new palette), not on every frame. An item drawn
//! translucent or lit by a gradient blends with the frame under it, which a
//! window cursor cannot do, so such an item stays in the frame with the
//! hardware cursor hidden. Scripted or scheduled runs (`play --input`,
//! check runs) keep the cursor and the item in the frame, so their frames
//! are unchanged, unless an offscreen run asks for the window cursor in
//! its shots ([`CursorInShots`]). Hit-testing maps the window pointer to the frame
//! through [`crate::ui::Presentation`], whichever way the cursor is drawn.

use std::sync::atomic::{AtomicU8, Ordering};

use bevy::asset::RenderAssetUsages;
use bevy::image::Image;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::window::{CursorIcon, CursorOptions, CustomCursor, CustomCursorImage, PrimaryWindow};

use super::present::WorldViewUi;
use super::ui_bind::UiRules;
use super::ViewAssets;
use crate::app::config::CursorSize;
use crate::frames::IndexFrame;
use crate::scene::{BlendOp, MapTable, ShadeChain};
use crate::ui::draw::ImageRequest;
use crate::ui::panels::UiFiles;
use crate::ui::UiDraw;

/// `[video] cursor_size`, as its index in [`CursorSize::ALL`].
static SIZE: AtomicU8 = AtomicU8::new(0);

/// Sets the pointer's size (from the settings; the next drawn frame shows
/// it).
pub fn set_size(size: CursorSize) {
    let i = CursorSize::ALL.iter().position(|s| *s == size).unwrap_or(0);
    SIZE.store(i as u8, Ordering::Relaxed);
}

/// The pointer's size in force.
pub fn size() -> CursorSize {
    CursorSize::ALL
        .get(usize::from(SIZE.load(Ordering::Relaxed)))
        .copied()
        .unwrap_or_default()
}

/// The whole scale the pointer is drawn at under presentation `p` (none
/// known: Auto is 1).
pub fn pointer_scale(size: CursorSize, p: Option<&crate::ui::Presentation>) -> u32 {
    let (sx, sy) = p.map_or((1.0, 1.0), |p| (p.sx, p.sy));
    size.scale(sx, sy)
}

/// The archive-name prefix of the cursor cels (`cursor\<name>`).
const CURSOR_PREFIX: &str = "cursor\\";

/// The cursor as an unscaled RGBA image (frame pixels) and the point
/// inside it that sits on the pointer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CursorImage {
    pub width: u32,
    pub height: u32,
    /// Row-major, top row first, 4 bytes per pixel; palette index 0 is
    /// transparent, every other index opaque.
    pub rgba: Vec<u8>,
    pub hotspot: (u32, u32),
}

/// What the last drawn frame asks of the hardware cursor; the
/// [`apply`] system takes it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Pending {
    /// Nothing new: no frame was drawn, or the cursor stays in the frame.
    #[default]
    None,
    /// Show this image as the window cursor.
    Show(CursorImage),
    /// Hide the window cursor (an item is held, or no cursor is drawn).
    Hide,
}

/// Whether the hardware cursor is showing (the dev overlay reads it to
/// leave the pointer visible when it closes), and the image it shows.
#[derive(Resource, Clone, Debug, Default, PartialEq, Eq)]
pub struct HardwareCursorShown(pub bool, Option<CursorImage>);

/// `play --offscreen --shot-cursor`: the pointer a player would see, for
/// the run's shots. A scripted run has no window cursor, so its shots never
/// show one; with this resource the cursor leaves the frame as it does for
/// the real mouse, [`apply`] keeps its image (at the Cursor Size in force)
/// and the pointer's window pixel here without touching the window, and
/// each shot draws it on top ([`Self::draw_on`]), where the operating
/// system would.
#[derive(Resource, Clone, Debug, Default, PartialEq, Eq)]
pub struct CursorInShots {
    /// The window cursor's image; `None`: hidden (no cursor, or an item
    /// that stays in the frame).
    image: Option<CursorImage>,
    /// The pointer's window pixel (physical); `None`: off the frame.
    at: Option<(i64, i64)>,
}

impl CursorInShots {
    /// Takes one pass's cursor: `pending` from the drawn frame (nothing new
    /// keeps the last image, as the window keeps its cursor) at Cursor
    /// Size `size`, the pointer at frame pixel `pointer`, presented by `p`.
    pub fn update(
        &mut self,
        pending: &Pending,
        pointer: Option<crate::ui::Point>,
        p: Option<&crate::ui::Presentation>,
        size: CursorSize,
    ) {
        match pending {
            Pending::None => {}
            Pending::Hide => self.image = None,
            Pending::Show(image) => {
                let n = pointer_scale(size, p) as f32;
                self.image = Some(scaled(image, n, n));
            }
        }
        self.at = pointer.zip(p).map(|(m, p)| p.from_frame(m.x, m.y));
    }

    /// Draws the cursor on a shot: its hotspot on the pointer's pixel, the
    /// transparent pixels left alone, clipped to the picture.
    pub fn draw_on(&self, shot: &mut image::RgbaImage) {
        let (Some(c), Some((x, y))) = (&self.image, self.at) else {
            return;
        };
        let (left, top) = (x - i64::from(c.hotspot.0), y - i64::from(c.hotspot.1));
        let (w, h) = (i64::from(shot.width()), i64::from(shot.height()));
        for cy in 0..c.height {
            let sy = top + i64::from(cy);
            if !(0..h).contains(&sy) {
                continue;
            }
            for cx in 0..c.width {
                let sx = left + i64::from(cx);
                let i = ((cy * c.width + cx) * 4) as usize;
                if (0..w).contains(&sx) && c.rgba[i + 3] != 0 {
                    let px = image::Rgba([c.rgba[i], c.rgba[i + 1], c.rgba[i + 2], 255]);
                    shot.put_pixel(sx as u32, sy as u32, px);
                }
            }
        }
    }
}

/// Drops the game's cursor image from the window when a game ends, so the
/// next game sets its cursor afresh. Whether the pointer shows is left to
/// the caller (the front end draws its own cursor and keeps it hidden).
pub fn reset(world: &mut World) {
    if let Some(mut shown) = world.get_resource_mut::<HardwareCursorShown>() {
        *shown = HardwareCursorShown::default();
    }
    let mut q = world.query_filtered::<Entity, With<PrimaryWindow>>();
    let windows: Vec<Entity> = q.iter(world).collect();
    for w in windows {
        world.entity_mut(w).insert(CursorIcon::default());
    }
}

/// Takes the cursor out of a drawn frame's UI draws and resolves it to an
/// image through `palette`, the palette the frame is presented with
/// ([`super::ViewAssets::palette`]), so the pointer's colours
/// match the frame's. The UI draws the cursor last, so it is the last draw
/// when it is a cursor cel, or, with `item_drawn` (the UI's
/// [`crate::ui::original::OriginalUi::cursor_item_drawn`]), the held
/// item's graphic. An item keeps its colour remap; one drawn translucent or
/// gradient-lit stays in the frame. Anything else there (no cursor at all)
/// leaves the draws alone and hides the hardware cursor.
pub fn take_cursor<R: UiRules + ?Sized>(
    draws: &mut Vec<UiDraw>,
    files: &UiFiles,
    rules: &R,
    assets: &ViewAssets,
    palette: &d2_formats::palette::Palette,
    mouse: (i32, i32),
    item_drawn: bool,
) -> Pending {
    let Some(UiDraw::Image(req)) = draws.last() else {
        return Pending::Hide;
    };
    let cel = is_cursor_cel(files, req);
    if !cel && !item_drawn {
        return Pending::Hide;
    }
    let Ok(sprite) = rules.ui_image(req, assets) else {
        // Unresolved art: the cel stays drawn in the frame.
        return Pending::None;
    };
    let Ok(frame) = assets.frame(&sprite.frame.set, sprite.frame.index) else {
        return Pending::None;
    };
    let remap = if cel {
        None
    } else {
        match item_remap(&sprite.shade, sprite.blend, &assets.maps) {
            Some(remap) => remap,
            // Blends with the frame: the item stays drawn there.
            None => return Pending::Hide,
        }
    };
    let image = cursor_image(frame, remap, palette, (sprite.x, sprite.y), mouse);
    draws.pop();
    Pending::Show(image)
}

/// The index map a held item's graphic is drawn through: `Some(None)` for
/// none, `Some(Some(map))` for its colour remap folded into one table, and
/// `None` when the draw blends with the frame under it (translucent, or
/// lit per pixel), which a window cursor cannot show.
fn item_remap(shade: &ShadeChain, blend: BlendOp, maps: &MapTable) -> Option<Option<[u8; 256]>> {
    if blend != BlendOp::Opaque || shade.gradient().is_some() {
        return None;
    }
    let chain = shade.maps();
    if chain.is_empty() {
        return Some(None);
    }
    let mut table = [0u8; 256];
    for (i, out) in table.iter_mut().enumerate() {
        let mut v = i as u8;
        for &m in chain {
            v = maps.get(m)?[usize::from(v)];
        }
        *out = v;
    }
    Some(Some(table))
}

fn is_cursor_cel(files: &UiFiles, req: &ImageRequest) -> bool {
    files
        .name(req.image.file)
        .is_some_and(|n| n.to_ascii_lowercase().starts_with(CURSOR_PREFIX))
}

/// The RGBA image of `frame` drawn with its top-left at `top_left`, each
/// non-transparent index first passed through `remap` when there is one,
/// and its hotspot: the pointer position `mouse` relative to that corner,
/// kept inside the image (the window system requires it).
pub fn cursor_image(
    frame: &IndexFrame,
    remap: Option<[u8; 256]>,
    palette: &d2_formats::palette::Palette,
    top_left: (i32, i32),
    mouse: (i32, i32),
) -> CursorImage {
    let mut rgba = Vec::with_capacity(frame.pixels.len() * 4);
    for &i in &frame.pixels {
        let shown = match (i, &remap) {
            (0, _) | (_, None) => i,
            (_, Some(map)) => map[usize::from(i)],
        };
        let c = palette.colors[usize::from(shown)];
        rgba.extend_from_slice(&[c.r, c.g, c.b, if i == 0 { 0 } else { 255 }]);
    }
    let clamp = |v: i32, len: u32| v.clamp(0, len.saturating_sub(1) as i32) as u32;
    CursorImage {
        width: frame.width,
        height: frame.height,
        rgba,
        hotspot: (
            clamp(mouse.0 - top_left.0, frame.width),
            clamp(mouse.1 - top_left.1, frame.height),
        ),
    }
}

/// `image` scaled by `sx` × `sy` (nearest neighbour), the scale the frame
/// itself is presented at, with its hotspot scaled to match. A scale
/// below 1 counts as 1, so the cursor never shrinks below its art.
pub fn scaled(image: &CursorImage, sx: f32, sy: f32) -> CursorImage {
    let (sx, sy) = (sx.max(1.0), sy.max(1.0));
    let size = |len: u32, s: f32| ((len as f32 * s).round() as u32).max(1);
    let (w, h) = (size(image.width, sx), size(image.height, sy));
    // The source pixel under the centre of each output pixel.
    let src = |out: u32, s: f32, len: u32| (((out as f32 + 0.5) / s) as u32).min(len - 1);
    let mut rgba = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        let row = src(y, sy, image.height) * image.width;
        for x in 0..w {
            let p = ((row + src(x, sx, image.width)) * 4) as usize;
            rgba.extend_from_slice(&image.rgba[p..p + 4]);
        }
    }
    let spot = |v: u32, s: f32, len: u32| ((v as f32 * s) as u32).min(len - 1);
    CursorImage {
        width: w,
        height: h,
        rgba,
        hotspot: (spot(image.hotspot.0, sx, w), spot(image.hotspot.1, sy, h)),
    }
}

/// Applies the pending cursor to the primary window: a new image becomes
/// its custom cursor (only when it differs from the one shown), and the
/// pointer is shown or hidden.
pub fn apply(
    mut commands: Commands,
    ui: Option<NonSendMut<WorldViewUi>>,
    mut images: ResMut<Assets<Image>>,
    mut windows: Query<(Entity, &Window, &mut CursorOptions), With<PrimaryWindow>>,
    mut shown: ResMut<HardwareCursorShown>,
    in_shots: Option<ResMut<CursorInShots>>,
) {
    let Some(mut ui) = ui else {
        return;
    };
    let pending = std::mem::take(&mut ui.hw_cursor);
    let Ok((entity, window, mut options)) = windows.single_mut() else {
        return;
    };
    let p = super::super::ui::edge::presentation(window).ok();
    if let Some(mut in_shots) = in_shots {
        in_shots.update(&pending, ui.pointer(), p.as_ref(), size());
        // A scripted run's cursor is for its shots only: the window's own
        // pointer stays as it is.
        if ui.scripted() {
            return;
        }
    }
    match pending {
        Pending::None => return,
        Pending::Hide => shown.0 = false,
        Pending::Show(image) => {
            let n = pointer_scale(size(), p.as_ref()) as f32;
            let image = scaled(&image, n, n);
            if shown.1.as_ref() != Some(&image) {
                let handle = images.add(Image::new(
                    Extent3d {
                        width: image.width,
                        height: image.height,
                        depth_or_array_layers: 1,
                    },
                    TextureDimension::D2,
                    image.rgba.clone(),
                    TextureFormat::Rgba8UnormSrgb,
                    RenderAssetUsages::MAIN_WORLD,
                ));
                commands
                    .entity(entity)
                    .insert(CursorIcon::Custom(CustomCursor::Image(CustomCursorImage {
                        handle,
                        hotspot: (image.hotspot.0 as u16, image.hotspot.1 as u16),
                        ..default()
                    })));
                shown.1 = Some(image);
            }
            shown.0 = true;
        }
    }
    if options.visible != shown.0 {
        options.visible = shown.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use d2_formats::palette::{Palette, Rgb};

    fn palette() -> Palette {
        let mut colors = [Rgb { r: 0, g: 0, b: 0 }; 256];
        for (i, c) in colors.iter_mut().enumerate() {
            *c = Rgb {
                r: i as u8,
                g: 255 - i as u8,
                b: 7,
            };
        }
        Palette { colors }
    }

    fn frame(w: u32, h: u32, pixels: Vec<u8>) -> IndexFrame {
        IndexFrame::new(w, h, 0, 0, pixels).expect("frame")
    }

    #[test]
    fn index_0_is_transparent_and_others_take_the_palette() {
        let img = cursor_image(&frame(2, 1, vec![0, 9]), None, &palette(), (0, 0), (0, 0));
        assert_eq!(img.rgba, vec![0, 255, 7, 0, 9, 246, 7, 255]);
        assert_eq!((img.width, img.height), (2, 1));
    }

    #[test]
    fn the_hotspot_is_the_pointer_relative_to_the_cel() {
        let f = frame(4, 3, vec![1; 12]);
        let img = cursor_image(&f, None, &palette(), (100, 50), (101, 52));
        assert_eq!(img.hotspot, (1, 2));
    }

    #[test]
    fn the_hotspot_stays_inside_the_image() {
        let f = frame(4, 3, vec![1; 12]);
        assert_eq!(
            cursor_image(&f, None, &palette(), (100, 50), (90, 40)).hotspot,
            (0, 0)
        );
        assert_eq!(
            cursor_image(&f, None, &palette(), (100, 50), (200, 99)).hotspot,
            (3, 2)
        );
    }

    #[test]
    fn a_held_items_remap_recolours_it_and_keeps_index_0_transparent() {
        use crate::scene::MapId;
        let mut maps = MapTable::new();
        // Two maps in a chain: +1, then ×2 (wrapping), folded into one.
        let plus = maps.push(std::array::from_fn(|i| (i as u8).wrapping_add(1)));
        let twice = maps.push(std::array::from_fn(|i| (i as u8).wrapping_mul(2)));
        let chain = ShadeChain::new(&[plus, twice]).unwrap();
        let table = item_remap(&chain, BlendOp::Opaque, &maps)
            .expect("an opaque item shows")
            .expect("it has a remap");
        assert_eq!(table[9], 20);
        // 127 maps to 0: drawn as opaque index 0, as the frame draws it.
        let img = cursor_image(
            &frame(3, 1, vec![0, 9, 127]),
            Some(table),
            &palette(),
            (0, 0),
            (1, 0),
        );
        assert_eq!(
            img.rgba,
            vec![0, 255, 7, 0, 20, 235, 7, 255, 0, 255, 7, 255]
        );
        // No chain: no remap.
        assert_eq!(
            item_remap(&ShadeChain::EMPTY, BlendOp::Opaque, &maps),
            Some(None)
        );
        // A map the table does not hold: the item stays in the frame.
        let missing = ShadeChain::new(&[MapId(99)]).unwrap();
        assert_eq!(item_remap(&missing, BlendOp::Opaque, &maps), None);
    }

    #[test]
    fn a_translucent_held_item_stays_in_the_frame() {
        let maps = MapTable::new();
        let blend = BlendOp::IndexTable(crate::scene::MapId(0));
        assert_eq!(item_remap(&ShadeChain::EMPTY, blend, &maps), None);
    }

    #[test]
    fn scaling_repeats_pixels_and_scales_the_hotspot() {
        let img = CursorImage {
            width: 2,
            height: 1,
            rgba: vec![1, 1, 1, 255, 2, 2, 2, 0],
            hotspot: (1, 0),
        };
        let s = scaled(&img, 2.0, 2.0);
        assert_eq!((s.width, s.height), (4, 2));
        assert_eq!(s.hotspot, (2, 0));
        let row = [1, 1, 1, 255, 1, 1, 1, 255, 2, 2, 2, 0, 2, 2, 2, 0];
        assert_eq!(s.rgba, [row, row].concat());
        assert_eq!(scaled(&img, 1.0, 1.0), img);
        assert_eq!(scaled(&img, 0.5, 0.5), img);
    }

    #[test]
    fn auto_follows_the_whole_picture_scale_and_fixed_sizes_ignore_it() {
        let at =
            |w, h, fw, fh, mode| crate::ui::Presentation::with_mode(w, h, fw, fh, mode).unwrap();
        use crate::ui::ScaleMode::{Fit, Integer};
        // The 1280 × 720 view: 3x at 2160p, 2x at 1440p, 1.5x → 2 at 1080p.
        for ((w, h), mode, n) in [
            ((3840, 2160), Integer, 3),
            ((3840, 2160), Fit, 3),
            ((3840, 2097), Integer, 2),
            ((2560, 1440), Fit, 2),
            ((1920, 1080), Fit, 2),
            ((1920, 1080), Integer, 1),
            ((1366, 768), Fit, 1),
        ] {
            let p = at(w, h, 1280, 720, mode);
            assert_eq!(pointer_scale(CursorSize::Auto, Some(&p)), n, "{w}x{h}");
        }
        let p = at(3840, 2160, 1280, 720, Integer);
        for (size, n) in CursorSize::ALL.into_iter().skip(1).zip(1..) {
            assert_eq!(pointer_scale(size, Some(&p)), n);
            assert_eq!(pointer_scale(size, None), n);
        }
        assert_eq!(pointer_scale(CursorSize::Auto, None), 1);
        // A frame shown smaller than itself still gets a 1x pointer.
        let small = at(640, 480, 1280, 720, Fit);
        assert_eq!(pointer_scale(CursorSize::Auto, Some(&small)), 1);
    }

    /// `--shot-cursor`: a shot shows the window cursor at each Cursor
    /// Size, its hotspot on the pointer's first window pixel, its
    /// transparent pixels showing the picture.
    #[test]
    fn a_shot_shows_the_window_cursor_at_its_size_on_the_pointer() {
        use crate::ui::{Point, Presentation, ScaleMode};
        use image::{Rgba, RgbaImage};
        // One red pixel on the hotspot, one transparent beside it.
        let img = CursorImage {
            width: 2,
            height: 1,
            rgba: vec![255, 0, 0, 255, 0, 255, 0, 0],
            hotspot: (0, 0),
        };
        let p = Presentation::with_mode(3840, 2160, 1280, 720, ScaleMode::Integer).unwrap();
        let grey = Rgba([9, 9, 9, 255]);
        let red = |shot: &RgbaImage| -> Vec<(u32, u32)> {
            shot.enumerate_pixels()
                .filter(|(_, _, px)| **px != grey)
                .map(|(x, y, px)| {
                    assert_eq!(*px, Rgba([255, 0, 0, 255]));
                    (x, y)
                })
                .collect()
        };
        let at = Point::new(100, 50);
        let (x0, y0) = p.from_frame(at.x, at.y);
        assert_eq!((x0, y0), (300, 150));
        for (size, n) in CursorSize::ALL.into_iter().zip([3u32, 1, 2, 3, 4]) {
            let mut c = CursorInShots::default();
            c.update(&Pending::Show(img.clone()), Some(at), Some(&p), size);
            let mut shot = RgbaImage::from_pixel(3840, 2160, grey);
            c.draw_on(&mut shot);
            let block: Vec<(u32, u32)> = (0..n)
                .flat_map(|y| (0..n).map(move |x| (300 + x, 150 + y)))
                .collect();
            let mut got = red(&shot);
            got.sort_by_key(|&(x, y)| (y, x));
            assert_eq!(got, block, "{size:?}");
        }
        // Nothing new keeps the image and follows the pointer; a hidden
        // cursor or a pointer off the frame draws nothing.
        let mut c = CursorInShots::default();
        c.update(
            &Pending::Show(img.clone()),
            Some(at),
            Some(&p),
            CursorSize::X1,
        );
        c.update(
            &Pending::None,
            Some(Point::new(0, 0)),
            Some(&p),
            CursorSize::X1,
        );
        let mut shot = RgbaImage::from_pixel(3840, 2160, grey);
        c.draw_on(&mut shot);
        assert_eq!(red(&shot), vec![(0, 0)]);
        c.update(&Pending::None, None, Some(&p), CursorSize::X1);
        let mut shot = RgbaImage::from_pixel(3840, 2160, grey);
        c.draw_on(&mut shot);
        assert!(red(&shot).is_empty());
        c.update(&Pending::Hide, Some(at), Some(&p), CursorSize::X1);
        c.draw_on(&mut shot);
        assert!(red(&shot).is_empty());
        // A cursor at the picture's corner is cut to it.
        let mut c = CursorInShots::default();
        c.update(
            &Pending::Show(img),
            Some(Point::new(1279, 719)),
            Some(&p),
            CursorSize::X4,
        );
        c.draw_on(&mut shot);
        assert_eq!(red(&shot).len(), 3 * 3);
    }

    #[test]
    fn set_size_is_what_size_reads() {
        for s in CursorSize::ALL {
            set_size(s);
            assert_eq!(size(), s);
        }
        set_size(CursorSize::Auto);
    }

    /// The window system puts the pointer's position on the hotspot pixel;
    /// at every size it is the first pixel of the source hotspot's block,
    /// so the pointer's tip stays where the mouse maps into the frame.
    #[test]
    fn every_cursor_size_keeps_the_hotspot_on_the_tip_pixel() {
        let (w, h) = (5u32, 4u32);
        let rgba: Vec<u8> = (0..w * h)
            .flat_map(|i| [i as u8, 0, 0, if i == 0 { 0 } else { 255 }])
            .collect();
        for spot in [(0, 0), (2, 1), (4, 3)] {
            let img = CursorImage {
                width: w,
                height: h,
                rgba: rgba.clone(),
                hotspot: spot,
            };
            for n in 1..=4u32 {
                let s = scaled(&img, n as f32, n as f32);
                assert_eq!((s.width, s.height), (w * n, h * n));
                assert_eq!(s.hotspot, (spot.0 * n, spot.1 * n));
                let src = ((spot.1 * w + spot.0) * 4) as usize;
                for dy in 0..n {
                    for dx in 0..n {
                        let (x, y) = (s.hotspot.0 + dx, s.hotspot.1 + dy);
                        let p = ((y * s.width + x) * 4) as usize;
                        assert_eq!(s.rgba[p..p + 4], img.rgba[src..src + 4]);
                    }
                }
            }
        }
    }

    #[test]
    fn a_fractional_scale_rounds_the_size_and_keeps_the_hotspot_inside() {
        let img = CursorImage {
            width: 2,
            height: 2,
            rgba: [
                [1u8, 1, 1, 255],
                [2, 2, 2, 255],
                [3, 3, 3, 255],
                [4, 4, 4, 255],
            ]
            .concat(),
            hotspot: (1, 1),
        };
        let s = scaled(&img, 1.5, 1.25);
        assert_eq!((s.width, s.height), (3, 3));
        assert_eq!(s.hotspot, (1, 1));
        // Columns 0,1 come from source column 0 and 1; column 2 from 1.
        assert_eq!(&s.rgba[..12], &[1, 1, 1, 255, 2, 2, 2, 255, 2, 2, 2, 255]);
        assert!(s.hotspot.0 < s.width && s.hotspot.1 < s.height);
    }
}
