//! Looks: colour grades of our own for the game world, as 3D colour tables.
//!
//! A look maps every encoded (sRGB) colour to another. Each is defined
//! here as a small function and baked into a [`SIZE`]³ table that the post
//! pass (`super::post`, `post.wgsl`) reads with trilinear filtering, before
//! the brightness, saturation and sharpen sliders. Original is no table at
//! all: the pass doesn't read one, and the picture is the scaled frame.
//! Looks touch the world only; the native UI is laid on afterwards.
//!
//! The grades (all mild, so the art keeps its own palette):
//! - **Warm**: torch-lit, reds and yellows up a little, blues down,
//!   gentle contrast.
//! - **Cool**: moonlit, blues up, reds down, a touch less saturation.
//! - **Vivid**: more saturation and a soft S-curve.
//! - **Muted**: faded film, less saturation, lifted blacks and softened
//!   whites.

use bevy::asset::RenderAssetUsages;
use bevy::image::{Image, ImageSampler};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

pub use crate::app::config::Look;

/// Grid points per axis of a look's table.
pub const SIZE: u32 = 33;

/// Rec. 709 luma of an encoded colour.
fn luma(c: [f32; 3]) -> f32 {
    0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]
}

/// `c` with its chroma scaled by `k` around its luma.
fn saturate(c: [f32; 3], k: f32) -> [f32; 3] {
    let y = luma(c);
    c.map(|v| y + (v - y) * k)
}

/// A soft S-curve of strength `k` (0: none): `c` pulled toward smoothstep.
fn s_curve(v: f32, k: f32) -> f32 {
    let s = v * v * (3.0 - 2.0 * v);
    v + (s - v) * k
}

/// The look's grade of encoded colour `c` (each channel 0..1).
pub fn grade(look: Look, c: [f32; 3]) -> [f32; 3] {
    let out = match look {
        Look::Original => c,
        Look::Warm => {
            let [r, g, b] = c;
            let c = [r * 1.04 + 0.01, g * 1.01, b * 0.90];
            c.map(|v| s_curve(v.clamp(0.0, 1.0), 0.15))
        }
        Look::Cool => {
            let [r, g, b] = c;
            let c = saturate([r * 0.93, g * 0.99, b * 1.06 + 0.01], 0.92);
            c.map(|v| s_curve(v.clamp(0.0, 1.0), 0.10))
        }
        Look::Vivid => saturate(c, 1.3).map(|v| s_curve(v.clamp(0.0, 1.0), 0.25)),
        Look::Muted => saturate(c, 0.7).map(|v| 0.035 + v.clamp(0.0, 1.0) * 0.93),
    };
    out.map(|v| v.clamp(0.0, 1.0))
}

/// The look's table: [`SIZE`]³ RGBA texels, red fastest, then green, then
/// blue (a 3D texture's layout), grid point `i` at encoded value
/// `i / (SIZE − 1)`.
pub fn table(look: Look) -> Vec<u8> {
    let n = SIZE as usize;
    let at = |i: usize| i as f32 / (n - 1) as f32;
    let mut out = Vec::with_capacity(n * n * n * 4);
    for b in 0..n {
        for g in 0..n {
            for r in 0..n {
                let c = grade(look, [at(r), at(g), at(b)]);
                out.extend(c.map(|v| (v * 255.0).round() as u8));
                out.push(255);
            }
        }
    }
    out
}

/// The 3D textures of the looks that have one (not Original), made once.
#[derive(Resource, Clone)]
pub struct LookTables {
    tables: Vec<(Look, Handle<Image>)>,
}

impl LookTables {
    /// The table of `look`; `None` for Original.
    pub fn get(&self, look: Look) -> Option<Handle<Image>> {
        self.tables
            .iter()
            .find(|(l, _)| *l == look)
            .map(|(_, h)| h.clone())
    }
}

/// Bakes every look's table into an image (stored unconverted: the pass
/// reads encoded values and writes encoded values).
pub fn make_tables(images: &mut Assets<Image>) -> LookTables {
    let tables = Look::ALL
        .into_iter()
        .filter(|l| *l != Look::Original)
        .map(|look| {
            let mut image = Image::new(
                Extent3d {
                    width: SIZE,
                    height: SIZE,
                    depth_or_array_layers: SIZE,
                },
                TextureDimension::D3,
                table(look),
                TextureFormat::Rgba8Unorm,
                RenderAssetUsages::RENDER_WORLD,
            );
            image.sampler = ImageSampler::linear();
            (look, images.add(image))
        })
        .collect();
    LookTables { tables }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn original_is_identity_and_every_look_stays_in_range() {
        for i in 0..=20 {
            let v = i as f32 / 20.0;
            for c in [[v, v, v], [v, 1.0 - v, 0.5], [0.2, v, 0.9]] {
                assert_eq!(grade(Look::Original, c), c);
                for look in Look::ALL {
                    assert!(grade(look, c).iter().all(|x| (0.0..=1.0).contains(x)));
                }
            }
        }
    }

    #[test]
    fn every_look_keeps_dark_below_light() {
        // A grey ramp stays a ramp: no look turns a darker grey lighter.
        for look in Look::ALL {
            let mut last = -1.0;
            for i in 0..=64 {
                let v = i as f32 / 64.0;
                let y = luma(grade(look, [v, v, v]));
                assert!(y >= last - 1e-4, "{look:?} at {v}");
                last = y;
            }
        }
    }

    #[test]
    fn the_looks_do_what_their_names_say() {
        let mid = [0.5, 0.45, 0.4];
        let warm = grade(Look::Warm, mid);
        let cool = grade(Look::Cool, mid);
        assert!(
            warm[0] - warm[2] > mid[0] - mid[2],
            "warm: more red than blue"
        );
        assert!(
            cool[2] - cool[0] > mid[2] - mid[0],
            "cool: more blue than red"
        );
        let chroma = |c: [f32; 3]| c[0] - c[2];
        let strong = [0.7, 0.4, 0.2];
        assert!(chroma(grade(Look::Vivid, strong)) > chroma(strong));
        assert!(chroma(grade(Look::Muted, strong)) < chroma(strong));
        // Muted lifts black and softens white.
        assert!(grade(Look::Muted, [0.0; 3])[0] > 0.02);
        assert!(grade(Look::Muted, [1.0; 3])[0] < 0.98);
    }

    #[test]
    fn the_table_is_the_grade_on_its_grid() {
        let t = table(Look::Original);
        let n = SIZE as usize;
        assert_eq!(t.len(), n * n * n * 4);
        // Original's table is its own grid: texel (r, g, b) holds r, g, b.
        for (r, g, b) in [(0, 0, 0), (n - 1, 0, 0), (3, 17, 30), (n - 1, n - 1, n - 1)] {
            let i = ((b * n + g) * n + r) * 4;
            let want = [r, g, b].map(|v| (v as f32 / (n - 1) as f32 * 255.0).round() as u8);
            assert_eq!(&t[i..i + 3], &want);
            assert_eq!(t[i + 3], 255);
        }
        let warm = table(Look::Warm);
        assert_ne!(warm, t);
    }
}
