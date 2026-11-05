//! CPU reference renderer for the map preview: the ground truth the GPU output is compared
//! to.
//!
//! Items are painted in layout order, so later items cover earlier ones. Index 0 is
//! transparent in an image and black where nothing was drawn; every other index is shown as
//! its palette colour exactly, with no lighting.

use d2_formats::palette::Palette;

use super::layout::Layout;
use super::tiles::TileLibrary;

/// A screen-space rectangle to render (y down).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct View {
    pub left: i32,
    pub top: i32,
    pub width: u32,
    pub height: u32,
}

/// Paints the layout into palette indices (0 = background), clipped to `view`.
pub fn render_indexed(layout: &Layout, lib: &TileLibrary, view: View) -> Vec<u8> {
    let (vw, vh) = (view.width as i32, view.height as i32);
    let mut out = vec![0u8; view.width as usize * view.height as usize];
    for item in &layout.items {
        let img = &lib.images[item.image];
        let (iw, ih) = (img.width as i32, img.height as i32);
        let (ox, oy) = (item.x - view.left, item.y - view.top);
        let (cx0, cy0) = (ox.max(0), oy.max(0));
        let (cx1, cy1) = ((ox + iw).min(vw), (oy + ih).min(vh));
        for y in cy0..cy1 {
            let src = ((y - oy) * iw) as usize;
            let dst = (y * vw) as usize;
            for x in cx0..cx1 {
                let p = img.pixels[src + (x - ox) as usize];
                if p != 0 {
                    out[dst + x as usize] = p;
                }
            }
        }
    }
    out
}

/// Maps indices to opaque RGBA; index 0 is the black background.
pub fn to_rgba(indexed: &[u8], palette: &Palette) -> Vec<u8> {
    let mut rgba = Vec::with_capacity(indexed.len() * 4);
    for &p in indexed {
        if p == 0 {
            rgba.extend_from_slice(&[0, 0, 0, 255]);
        } else {
            let c = palette.colors[usize::from(p)];
            rgba.extend_from_slice(&[c.r, c.g, c.b, 255]);
        }
    }
    rgba
}

/// Render directly into a bounded viewport at a power-of-two zoom. Downscaling samples
/// source pixels without allocating a full-size intermediate image. Zoom zero is exact.
pub fn render_zoomed(layout: &Layout, lib: &TileLibrary, view: View, zoom: i8) -> Vec<u8> {
    if zoom == 0 {
        return render_indexed(layout, lib, view);
    }
    let zoom = zoom.clamp(-10, 2);
    let (mul, div) = if zoom > 0 {
        (1i64 << zoom, 1)
    } else {
        (1, 1i64 << -zoom)
    };
    let mut out = vec![0; view.width as usize * view.height as usize];
    let ceil = |n: i64| n.div_euclid(div) + i64::from(n.rem_euclid(div) != 0);
    for item in &layout.items {
        let img = &lib.images[item.image];
        let ox = i64::from(item.x) - i64::from(view.left);
        let oy = i64::from(item.y) - i64::from(view.top);
        let x0 = ceil(ox * mul).clamp(0, i64::from(view.width));
        let y0 = ceil(oy * mul).clamp(0, i64::from(view.height));
        let x1 = ceil((ox + i64::from(img.width)) * mul).clamp(0, i64::from(view.width));
        let y1 = ceil((oy + i64::from(img.height)) * mul).clamp(0, i64::from(view.height));
        for y in y0..y1 {
            let sy = y * div / mul - oy;
            for x in x0..x1 {
                let sx = x * div / mul - ox;
                let p = img.pixels[sy as usize * img.width as usize + sx as usize];
                if p != 0 {
                    out[y as usize * view.width as usize + x as usize] = p;
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::layout::{DrawItem, Source};
    use crate::map::tiles::{TileImage, TileKey};

    fn item(x: i32, y: i32) -> DrawItem {
        DrawItem {
            image: 0,
            x,
            y,
            key: TileKey {
                orientation: 0,
                main: 0,
                sub: 0,
            },
            cell: (0, 0),
            source: Source::Sprite,
        }
    }

    fn lib() -> TileLibrary {
        let mut lib = TileLibrary::new();
        lib.images.push(TileImage {
            x0: 0,
            y0: 0,
            width: 2,
            height: 2,
            pixels: vec![1, 0, 2, 3],
            roof_height: 0,
        });
        lib
    }

    #[test]
    fn later_items_paint_over_and_zero_is_transparent() {
        let lib = lib();
        let layout = Layout {
            items: vec![item(0, 0), item(1, 0)],
            ..Layout::default()
        };
        let view = View {
            left: 0,
            top: 0,
            width: 3,
            height: 2,
        };
        // Second copy shifted by one: its (0,0)=1 covers first's (1,0)=0;
        // its transparent (1,0) leaves the background at (2,0).
        assert_eq!(render_indexed(&layout, &lib, view), [1, 1, 0, 2, 2, 3]);
    }

    #[test]
    fn clipping_to_view() {
        let lib = lib();
        let layout = Layout {
            items: vec![item(-1, -1)],
            ..Layout::default()
        };
        let view = View {
            left: 0,
            top: 0,
            width: 2,
            height: 2,
        };
        assert_eq!(render_indexed(&layout, &lib, view), [3, 0, 0, 0]);
    }

    #[test]
    fn probe_finds_topmost_opaque_items() {
        let lib = lib();
        let layout = Layout {
            items: vec![item(0, 0), item(1, 0)],
            ..Layout::default()
        };
        // (1,0): second item's pixel (0,0)=1 on top; first item's (1,0) is
        // transparent, so only one hit.
        let hits = layout.probe(&lib, 1, 0);
        assert_eq!(hits.len(), 1);
        assert_eq!((hits[0].0, hits[0].2), (1, 1));
        // (1,1): both opaque, topmost first.
        let hits = layout.probe(&lib, 1, 1);
        assert_eq!(hits.iter().map(|h| h.0).collect::<Vec<_>>(), [1, 0]);
    }

    #[test]
    fn rgba_mapping() {
        let mut palette = Palette {
            colors: [Default::default(); 256],
        };
        palette.colors[2] = d2_formats::palette::Rgb { r: 9, g: 8, b: 7 };
        assert_eq!(to_rgba(&[0, 2], &palette), [0, 0, 0, 255, 9, 8, 7, 255]);
    }
}

#[cfg(test)]
mod zoom_tests {
    use super::*;
    use crate::map::{
        layout::{DrawItem, Source},
        tiles::{TileImage, TileKey},
    };
    #[test]
    fn zoom_matches_reference_and_samples_clipped_items_in_order() {
        let mut lib = TileLibrary::new();
        lib.images.push(TileImage {
            x0: 0,
            y0: 0,
            width: 4,
            height: 2,
            pixels: vec![1, 2, 3, 4, 5, 0, 7, 8],
            roof_height: 0,
        });
        let key = TileKey {
            orientation: 0,
            main: 0,
            sub: 0,
        };
        let item = |x, y| DrawItem {
            image: 0,
            x,
            y,
            key,
            cell: (0, 0),
            source: Source::Sprite,
        };
        let layout = Layout {
            items: vec![item(-1, 0), item(1, 1)],
            ..Default::default()
        };
        let view = View {
            left: -2,
            top: 0,
            width: 4,
            height: 2,
        };
        assert_eq!(
            render_zoomed(&layout, &lib, view, 0),
            render_indexed(&layout, &lib, view)
        );
        assert_eq!(
            render_zoomed(&layout, &lib, view, -1),
            [0, 2, 4, 0, 0, 0, 0, 8]
        );
        let layout = Layout {
            items: vec![item(0, 0)],
            ..Default::default()
        };
        let view = View {
            left: 0,
            top: 0,
            width: 4,
            height: 4,
        };
        assert_eq!(
            render_zoomed(&layout, &lib, view, 1),
            [1, 1, 2, 2, 1, 1, 2, 2, 5, 5, 0, 0, 5, 5, 0, 0]
        );
    }
}
