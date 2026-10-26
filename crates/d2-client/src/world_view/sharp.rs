//! Sharp scaling of the presented game frame (and the front end's
//! picture).
//!
//! The frame is shown through [`SharpMaterial`] on a quad the size of the
//! frame, scaled to the presentation. Its shader keeps every game pixel a
//! solid block and blends only the one screen pixel where two game pixels
//! meet. At a fractional scale (fit: 1.8× at 1080p, 2.4× at 1440p, 3.6× at
//! 2160p for a 600-line frame) every game pixel then covers the same width
//! on screen, with no smear and no shimmer as the picture moves; at a
//! whole-number scale every screen pixel falls inside one game pixel, so
//! the result is exactly nearest sampling.
//!
//! Fit is a nearest integer prescale (rounded up), then linear sampling
//! to the final size. The shader evaluates both steps in one read, avoiding
//! a large intermediate texture on small GPUs.
//!
//! It is one pass and one texture read per screen pixel, with a plain
//! linear sampler, so it runs on every GPU Bevy runs on.
//!
//! [`sharp_offset`] is the shader's per-axis rule in Rust, so the rule is
//! tested here ([`present_row`] models a whole screen row through it).
//!
//! The Scaling Filter setting ([`set_filter`]) picks how the frame is
//! scaled; Sharp Bilinear (above) is the default:
//! - **Nearest**: the game pixel under each screen pixel's centre. At a
//!   whole-number scale it is exactly Sharp Bilinear; at a fractional one
//!   game pixels come out one screen pixel wider or narrower.
//! - **Catmull-Rom**: the cubic spline through the 4 × 4 game pixels around
//!   each sample (Keys' cubic with a = −0.5, the B = 0, C = ½ member of the
//!   Mitchell-Netravali family). Soft, flat areas stay flat, slight
//!   overshoot at edges.
//! - **Lanczos**: the windowed sinc `sinc(x) · sinc(x / 3)` over the 6 × 6
//!   game pixels around each sample (three lobes), weights normalised.
//!   Sharper than Catmull-Rom, with a little ringing at hard edges. (The
//!   two-lobe Lanczos is almost exactly Catmull-Rom, so it isn't offered.)
//!
//! - **Smooth Edges** (optional, never the default): Sharp Bilinear,
//!   except along a clear diagonal edge in the art, which is read along
//!   its own direction so the staircase of game pixels becomes a straight
//!   edge ([`smooth_edges_sample`] has the rule).
//!
//! Catmull-Rom reads 16 texels per sample, Lanczos and Smooth Edges 36,
//! with `textureLoad`. Both
//! presenting shaders carry the same sampling function (between
//! [`FILTER_FN_START`] and [`FILTER_FN_END`]); [`filter_row`] models it on
//! the CPU.

use bevy::asset::embedded_asset;
use bevy::image::ImageSampler;
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, ShaderType};
use bevy::shader::ShaderRef;
use bevy::sprite_render::{AlphaMode2d, Material2d, Material2dPlugin};

/// The frame image, sampled with a linear filter, the scale per axis and
/// the part of the image shown.
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct SharpMaterial {
    #[texture(0)]
    #[sampler(1)]
    pub frame: Handle<Image>,
    /// The presentation scale, picture options and scaling filter
    /// ([`scale`]).
    #[uniform(2)]
    pub scale: PresentScale,
    /// The part of the image the quad shows, in uv units: xy its origin,
    /// zw its size. [`FULL_IMAGE`] shows the whole frame; a layer that
    /// slides by a fraction of a pixel moves the origin instead.
    #[uniform(3)]
    pub uv_rect: Vec4,
    /// The smooth camera's world layer only: the tick before's world
    /// layer, read where the shifted image has no picture (its uv is
    /// outside the image), at [`Self::prev_uv`] (`super::smooth::prev_uv`;
    /// zero-sized: no fill). `None`: the image's clamped edge.
    #[texture(4)]
    pub prev: Option<Handle<Image>>,
    #[uniform(5)]
    pub prev_uv: Vec4,
    /// Opaque for the game frame; [`AlphaMode2d::Blend`] for a layer with
    /// transparent parts drawn over another (the smooth camera's screen
    /// layer, `super::smooth`).
    pub alpha_mode: AlphaMode2d,
}

/// How the presenting shaders scale the frame (WGSL `Present`; `filter`
/// is its `kind`, a reserved word in WGSL).
#[derive(ShaderType, Debug, Clone, Copy, PartialEq)]
pub struct PresentScale {
    /// xy: presentation scale (at least 1); zw: gamma exponent and
    /// contrast minus one (zero: identity).
    pub scale: Vec4,
    /// x: the [`ScalingFilter`] ([`ScalingFilter::index`]).
    pub filter: Vec4,
}

/// The whole image: origin (0, 0), size (1, 1).
pub const FULL_IMAGE: Vec4 = Vec4::new(0.0, 0.0, 1.0, 1.0);

impl SharpMaterial {
    /// The whole of `frame`, at scale 1 until the first placement.
    pub fn new(frame: Handle<Image>) -> Self {
        SharpMaterial {
            frame,
            scale: scale(1.0, 1.0),
            uv_rect: FULL_IMAGE,
            prev: None,
            prev_uv: Vec4::ZERO,
            alpha_mode: AlphaMode2d::Opaque,
        }
    }
}

impl Material2d for SharpMaterial {
    fn fragment_shader() -> ShaderRef {
        "embedded://d2_client/world_view/sharp.wgsl".into()
    }

    fn alpha_mode(&self) -> AlphaMode2d {
        self.alpha_mode
    }
}

/// Registers the material and its shader.
pub struct SharpPlugin;

impl Plugin for SharpPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "sharp.wgsl");
        app.add_plugins(Material2dPlugin::<SharpMaterial>::default());
    }
}

/// Adds [`SharpPlugin`] when the app renders and nothing added it yet (the
/// game's view and the front end both present through it).
pub fn add_sharp(app: &mut App) {
    if app.get_sub_app(bevy::render::RenderApp).is_some() && !app.is_plugin_added::<SharpPlugin>() {
        app.add_plugins(SharpPlugin);
    }
}

/// The shader's scale uniform for a presentation scale `(sx, sy)`, at
/// least 1 (below 1 game pixels are smaller than screen pixels and the
/// blend covers the whole pixel, which is plain linear filtering), with the
/// picture options and the scaling filter in force.
pub fn scale(sx: f32, sy: f32) -> PresentScale {
    let (gamma, contrast) = picture_adjustment();
    PresentScale {
        scale: Vec4::new(sx.max(1.0), sy.max(1.0), gamma - 1.0, contrast - 1.0),
        filter: Vec4::new(f32::from(filter().index()), 0.0, 0.0, 0.0),
    }
}

pub use crate::app::config::ScalingFilter;

/// `[video] scaling_filter`, as [`ScalingFilter::index`].
static FILTER: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);

/// Sets the scaling filter (from the settings; the next presented frame
/// uses it).
pub fn set_filter(f: ScalingFilter) {
    FILTER.store(f.index(), std::sync::atomic::Ordering::Relaxed);
}

/// The scaling filter in force.
pub fn filter() -> ScalingFilter {
    ScalingFilter::from_index(FILTER.load(std::sync::atomic::Ordering::Relaxed))
}

// Packed so a renderer sees Gamma and Contrast from the same change.
static PICTURE: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(155 | (100 << 16));
static BLENDED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

/// Apply picture options without changing the indexed frame or its hashes.
pub fn set_picture_options(s: &crate::app::config::Settings) {
    use std::sync::atomic::Ordering;
    PICTURE.store(
        u32::from(s.gamma) | (u32::from(s.contrast) << 16),
        Ordering::Relaxed,
    );
    BLENDED.store(s.blended_shadows != 0, Ordering::Relaxed);
    set_filter(s.scaling_filter);
    super::post::set_options(s);
}

pub fn blended_shadows() -> bool {
    BLENDED.load(std::sync::atomic::Ordering::Relaxed)
}

pub fn picture_adjustment() -> (f32, f32) {
    let p = PICTURE.load(std::sync::atomic::Ordering::Relaxed);
    picture_adjustment_for((p & 65535) as u16, (p >> 16) as u8)
}

fn picture_adjustment_for(gamma: u16, contrast: u8) -> (f32, f32) {
    (155.0 / gamma.max(1) as f32, f32::from(contrast) / 100.0)
}

/// Where the shader samples within a game pixel, on one axis: `d` is the
/// offset of the screen pixel's centre from the game pixel's left edge
/// (0..1) and `scale` the screen pixels per game pixel. The result is 0.5
/// (the pixel's own centre, a solid colour) except within half a screen
/// pixel of either edge, where it ramps toward the neighbour, reaching the
/// boundary between the two (an even blend) at the edge itself.
///
/// The solid region is widened by [`SOLID_SLACK`] so that at a
/// whole-number scale, where the outermost screen pixel centres sit right
/// on its edge, rounding never lets a sliver of the neighbour in.
pub fn sharp_offset(d: f32, scale: f32) -> f32 {
    let scale = scale.ceil().max(1.0);
    let c = d - 0.5;
    let solid = 0.5 - 0.5 / scale + SOLID_SLACK;
    (c - c.clamp(-solid, solid)) * scale + 0.5
}

/// How far (in game pixels) the solid region reaches past its exact edge;
/// the shader uses the same number.
pub const SOLID_SLACK: f32 = 1.0 / 4096.0;

/// The linear sampler the material reads the frame with: clamped at the
/// edges, so the outermost game pixels stay solid.
pub fn frame_sampler() -> ImageSampler {
    ImageSampler::linear()
}

/// One screen row through the shader, on the CPU: `frame` is a row of
/// game pixel values, `screen_w` the screen pixels it covers. Each screen
/// pixel samples where [`sharp_offset`] says, with the linear filter the
/// GPU applies (clamped at the ends).
pub fn present_row(frame: &[f32], screen_w: u32) -> Vec<f32> {
    let n = frame.len();
    let s = screen_w as f32 / n as f32;
    let at = |i: i64| frame[i.clamp(0, n as i64 - 1) as usize];
    (0..screen_w)
        .map(|x| {
            let texel = (x as f32 + 0.5) / s;
            let base = texel.floor();
            let p = base + sharp_offset(texel - base, s.max(1.0));
            // Linear filter between the texel centres around `p`.
            let left = (p - 0.5).floor();
            let w = p - 0.5 - left;
            at(left as i64) * (1.0 - w) + at(left as i64 + 1) * w
        })
        .collect()
}

/// The first line of the sampling function both presenting shaders carry
/// (`sharp.wgsl`, `native_ui.wgsl`); it runs to [`FILTER_FN_END`].
pub const FILTER_FN_START: &str = "// ---- present filter";
/// The line after the shared sampling function.
pub const FILTER_FN_END: &str = "// ---- end present filter";

/// The six tap weights of a kernel filter at fraction `t` (0..1) past the
/// texel centre before the sample: taps at −2 to 3 from that texel
/// (Catmull-Rom's outer two are 0). Catmull-Rom's sum to 1 by
/// construction; Lanczos's are normalised. `None` for the filters that
/// have no kernel.
pub fn kernel_weights(f: ScalingFilter, t: f32) -> Option<[f32; 6]> {
    let kernel: fn(f32) -> f32 = match f {
        ScalingFilter::CatmullRom => catmull_rom,
        ScalingFilter::Lanczos => lanczos3,
        _ => return None,
    };
    let mut w = [0.0; 6];
    for (i, w) in w.iter_mut().enumerate() {
        *w = kernel(i as f32 - 2.0 - t);
    }
    let sum: f32 = w.iter().sum();
    Some(w.map(|v| v / sum))
}

/// The Catmull-Rom kernel (Keys' cubic, a = −0.5).
pub fn catmull_rom(x: f32) -> f32 {
    let x = x.abs();
    if x < 1.0 {
        (1.5 * x - 2.5) * x * x + 1.0
    } else if x < 2.0 {
        ((-0.5 * x + 2.5) * x - 4.0) * x + 2.0
    } else {
        0.0
    }
}

/// The three-lobe Lanczos kernel, `sinc(x) · sinc(x / 3)` for |x| < 3.
pub fn lanczos3(x: f32) -> f32 {
    let x = x.abs();
    if x < 1e-5 {
        return 1.0;
    }
    if x >= 3.0 {
        return 0.0;
    }
    let px = std::f32::consts::PI * x;
    3.0 * px.sin() * (px / 3.0).sin() / (px * px)
}

/// One screen row through the shader's filter `f`, on the CPU, as
/// [`present_row`] is for Sharp Bilinear.
pub fn filter_row(frame: &[f32], screen_w: u32, f: ScalingFilter) -> Vec<f32> {
    let n = frame.len();
    let s = screen_w as f32 / n as f32;
    let at = |i: i64| frame[i.clamp(0, n as i64 - 1) as usize];
    match f {
        // A single row has no diagonal edges: Smooth Edges is Sharp
        // Bilinear there.
        ScalingFilter::SharpBilinear | ScalingFilter::SmoothEdges => present_row(frame, screen_w),
        ScalingFilter::Nearest => (0..screen_w)
            .map(|x| at(((x as f32 + 0.5) / s).floor() as i64))
            .collect(),
        ScalingFilter::CatmullRom | ScalingFilter::Lanczos => (0..screen_w)
            .map(|x| {
                let texel = (x as f32 + 0.5) / s - 0.5;
                let base = texel.floor();
                let w = kernel_weights(f, texel - base).expect("a kernel filter");
                (0..6)
                    .map(|i| w[i] * at(base as i64 - 2 + i as i64))
                    .sum::<f32>()
            })
            .collect(),
    }
}

/// Smooth Edges' tuning; the shader carries the same numbers as its
/// `SE_*` constants (a test checks).
pub struct SmoothEdges;

impl SmoothEdges {
    /// Coherence (how much one direction dominates the local gradients,
    /// 0..1) below which the filter is Sharp Bilinear, and at which it
    /// takes full effect.
    pub const COHERENCE: (f32, f32) = (0.45, 0.75);
    /// The same for how diagonal the edge runs (|sin 2θ| of its normal:
    /// 0 horizontal or vertical, 1 at 45°).
    pub const DIAGONAL: (f32, f32) = (0.2, 0.7);
    /// The same for the edge's strength (root mean square gradient, in
    /// brightness per game pixel).
    pub const EDGE: (f32, f32) = (0.02, 0.06);
    /// The reading's Gaussian spread across the edge in texels squared at
    /// scale 1 (divided by the scale: about one screen pixel of blend),
    /// and along it.
    pub const ACROSS: f32 = 0.25;
    pub const ALONG: f32 = 1.0;

    /// The shader's constant lines.
    #[cfg(test)]
    fn wgsl() -> String {
        let line = |name: &str, v: f32| format!("const SE_{name}: f32 = {v:?};\n");
        [
            line("COHERENCE_LO", Self::COHERENCE.0),
            line("COHERENCE_HI", Self::COHERENCE.1),
            line("DIAGONAL_LO", Self::DIAGONAL.0),
            line("DIAGONAL_HI", Self::DIAGONAL.1),
            line("EDGE_LO", Self::EDGE.0),
            line("EDGE_HI", Self::EDGE.1),
            line("ACROSS", Self::ACROSS),
            line("ALONG", Self::ALONG),
        ]
        .concat()
    }
}

fn smoothstep(lo: f32, hi: f32, x: f32) -> f32 {
    let t = ((x - lo) / (hi - lo)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// A premultiplied image read by texel, edges clamped by the caller.
pub trait Texels {
    fn texel(&self, x: i64, y: i64) -> Vec4;
}

impl<F: Fn(i64, i64) -> Vec4> Texels for F {
    fn texel(&self, x: i64, y: i64) -> Vec4 {
        self(x, y)
    }
}

/// The Sharp Bilinear sample of an image at `texel` (uv × size) and
/// presentation scale `scale`, on the CPU: [`sharp_offset`] on each axis,
/// then the linear filter.
pub fn sharp_sample(t: &impl Texels, texel: Vec2, scale: Vec2) -> Vec4 {
    let base = texel.floor();
    let f = Vec2::new(
        sharp_offset(texel.x - base.x, scale.x.max(1.0)),
        sharp_offset(texel.y - base.y, scale.y.max(1.0)),
    );
    let s = base + f - 0.5;
    let l = s.floor();
    let w = s - l;
    let (x, y) = (l.x as i64, l.y as i64);
    let top = t.texel(x, y).lerp(t.texel(x + 1, y), w.x);
    let bottom = t.texel(x, y + 1).lerp(t.texel(x + 1, y + 1), w.x);
    top.lerp(bottom, w.y)
}

/// Smooth Edges at `texel` (uv × size), on the CPU, as the shader's
/// `smooth_edges` computes it. Returns the sample and how much of the
/// along-the-edge reading it holds (0: exactly [`sharp_sample`]).
///
/// The local edge comes from the structure tensor of the 5 × 5 two-by-two
/// cells of the 6 × 6 texels around the sample: each cell's gradient
/// `g`, summed as `g gᵀ` with a tent weight (radius 2.5 around the sample,
/// so the result is continuous from one game pixel to the next). Its
/// eigenvalues give the coherence `(λ1 − λ2) / (λ1 + λ2)` and its
/// off-diagonal part how diagonal the dominant direction runs. Where both
/// are high and the edge is strong, the middle 4 × 4 texels are read with a
/// Gaussian that is narrow across the edge and wide along it, so the
/// texels on the edge's own line decide the colour: a 45° staircase reads
/// as a straight edge. Everywhere else the sample is Sharp Bilinear.
pub fn smooth_edges_sample(t: &impl Texels, texel: Vec2, scale: Vec2) -> (Vec4, f32) {
    let sharp = sharp_sample(t, texel, scale);
    let p = texel - 0.5;
    let base = p.floor();
    let f = p - base;
    let (bx, by) = (base.x as i64 - 2, base.y as i64 - 2);
    let luma = Vec4::new(0.299, 0.587, 0.114, 0.5);
    let c: Vec<Vec4> = (0..36).map(|k| t.texel(bx + k % 6, by + k / 6)).collect();
    let v: Vec<f32> = c.iter().map(|c| c.dot(luma)).collect();
    let (mut jxx, mut jxy, mut jyy, mut wsum) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
    for j in 0..5 {
        let wy = 2.5 - (j as f32 - 1.5 - f.y).abs();
        for i in 0..5 {
            let wx = 2.5 - (i as f32 - 1.5 - f.x).abs();
            let k = j * 6 + i;
            let gx = 0.5 * (v[k + 1] - v[k] + v[k + 7] - v[k + 6]);
            let gy = 0.5 * (v[k + 6] - v[k] + v[k + 7] - v[k + 1]);
            let w = wx * wy;
            jxx += w * gx * gx;
            jxy += w * gx * gy;
            jyy += w * gy * gy;
            wsum += w;
        }
    }
    let tr = jxx + jyy;
    let dd = jxx - jyy;
    let delta = (dd * dd + 4.0 * jxy * jxy).sqrt();
    let coherence = delta / tr.max(1e-8);
    let diagonal = (2.0 * jxy).abs() / delta.max(1e-8);
    let edge = (tr / wsum).sqrt();
    let amount = smoothstep(
        SmoothEdges::COHERENCE.0,
        SmoothEdges::COHERENCE.1,
        coherence,
    ) * smoothstep(SmoothEdges::DIAGONAL.0, SmoothEdges::DIAGONAL.1, diagonal)
        * smoothstep(SmoothEdges::EDGE.0, SmoothEdges::EDGE.1, edge);
    if amount <= 0.0 {
        return (sharp, 0.0);
    }
    let cos2 = dd / delta.max(1e-8);
    let n = Vec2::new(
        (0.5 + 0.5 * cos2).max(0.0).sqrt(),
        jxy.signum() * (0.5 - 0.5 * cos2).max(0.0).sqrt(),
    );
    let tangent = Vec2::new(-n.y, n.x);
    let across = scale.x.min(scale.y).max(1.0) / SmoothEdges::ACROSS;
    let along = 1.0 / SmoothEdges::ALONG;
    let (mut sum, mut total) = (Vec4::ZERO, 0.0f32);
    for j in 1..5 {
        for i in 1..5 {
            let q = Vec2::new(i as f32 - 2.0, j as f32 - 2.0) - f;
            let (a, l) = (q.dot(n), q.dot(tangent));
            let w = (-(a * a * across + l * l * along)).exp();
            sum += w * c[j * 6 + i];
            total += w;
        }
    }
    (sharp.lerp(sum / total.max(1e-12), amount), amount)
}

#[cfg(test)]
#[path = "sharp_gpu_tests.rs"]
mod gpu_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_presenting_shaders_carry_the_same_filter_function() {
        let part = |src: &'static str| {
            let a = src.find(FILTER_FN_START).expect("start marker");
            let b = src.find(FILTER_FN_END).expect("end marker");
            &src[a..b]
        };
        assert_eq!(
            part(include_str!("sharp.wgsl")),
            part(include_str!("native_ui.wgsl"))
        );
    }

    #[test]
    fn kernel_weights_interpolate_and_sum_to_one() {
        for f in [ScalingFilter::CatmullRom, ScalingFilter::Lanczos] {
            // On a texel centre: that texel alone.
            let w = kernel_weights(f, 0.0).unwrap();
            for (got, want) in w.iter().zip([0.0, 0.0, 1.0, 0.0, 0.0, 0.0]) {
                assert!((got - want).abs() < 1e-5, "{f:?} {w:?}");
            }
            for k in 0..=16 {
                let t = k as f32 / 16.0;
                let w = kernel_weights(f, t).unwrap();
                assert!((w.iter().sum::<f32>() - 1.0).abs() < 1e-5);
                // Mirror symmetric: t and 1 − t swap the taps.
                let m = kernel_weights(f, 1.0 - t).unwrap();
                for i in 0..6 {
                    assert!((w[i] - m[5 - i]).abs() < 1e-5, "{f:?} t={t}");
                }
            }
            // Halfway: the second taps out are negative lobes.
            let w = kernel_weights(f, 0.5).unwrap();
            assert!(w[1] < 0.0 && w[4] < 0.0 && w[2] > 0.5);
        }
        assert_eq!(kernel_weights(ScalingFilter::Nearest, 0.3), None);
        assert_eq!(kernel_weights(ScalingFilter::SharpBilinear, 0.3), None);
        // Lanczos's third lobe is positive.
        let w = kernel_weights(ScalingFilter::Lanczos, 0.5).unwrap();
        assert!(w[0] > 0.0 && w[5] > 0.0);
        // The published Catmull-Rom weights at ½: −1/16, 9/16, 9/16, −1/16.
        let w = kernel_weights(ScalingFilter::CatmullRom, 0.5).unwrap();
        for (got, want) in w.iter().zip([0.0, -0.0625, 0.5625, 0.5625, -0.0625, 0.0]) {
            assert!((got - want).abs() < 1e-6);
        }
    }

    #[test]
    fn every_filter_keeps_a_flat_picture_flat_and_catmull_rom_keeps_ramps() {
        for f in ScalingFilter::ALL {
            for (frame_w, screen) in [(1280usize, 3840u32), (1280, 1920), (1068, 3840)] {
                for v in filter_row(&vec![0.4f32; frame_w], screen, f) {
                    assert!((v - 0.4).abs() < 1e-5, "{f:?}");
                }
            }
        }
        // A cubic spline reproduces a straight line away from the ends.
        let ramp: Vec<f32> = (0..64).map(|i| i as f32).collect();
        let row = filter_row(&ramp, 64 * 3, ScalingFilter::CatmullRom);
        for (x, v) in row.iter().enumerate().skip(9).take(64 * 3 - 18) {
            let want = (x as f32 + 0.5) / 3.0 - 0.5;
            assert!((v - want).abs() < 1e-3, "x={x}: {v} != {want}");
        }
    }

    #[test]
    fn at_whole_scales_nearest_is_sharp_bilinear_and_the_kernels_soften() {
        let frame: Vec<f32> = (0..1280).map(|i| ((i * 37) % 256) as f32).collect();
        let sharp = filter_row(&frame, 3840, ScalingFilter::SharpBilinear);
        let nearest = filter_row(&frame, 3840, ScalingFilter::Nearest);
        for (a, b) in sharp.iter().zip(&nearest) {
            assert_eq!(a.round(), b.round());
        }
        let off = |f| -> f32 {
            filter_row(&frame, 3840, f)
                .iter()
                .zip(&nearest)
                .map(|(a, b)| (a - b).abs())
                .sum()
        };
        assert!(off(ScalingFilter::CatmullRom) > 1.0);
        assert!(off(ScalingFilter::Lanczos) > 1.0);
    }

    #[test]
    fn lanczos_keeps_more_fine_detail_than_catmull_rom_with_bounded_ringing() {
        // Fine detail: a sine of `f` cycles per game pixel shown at 3x; its
        // amplitude after scaling (RMS × √2, away from the ends).
        let amplitude = |f: f32, filter| {
            let frame: Vec<f32> = (0..512)
                .map(|i| (std::f32::consts::TAU * f * i as f32).sin())
                .collect();
            let row = filter_row(&frame, 512 * 3, filter);
            let mid = &row[150..row.len() - 150];
            (mid.iter().map(|v| v * v).sum::<f32>() / mid.len() as f32).sqrt()
                * std::f32::consts::SQRT_2
        };
        for f in [0.25, 0.35, 0.45] {
            let cr = amplitude(f, ScalingFilter::CatmullRom);
            let l = amplitude(f, ScalingFilter::Lanczos);
            assert!(l > cr, "{f}: {l} vs {cr}");
        }
        // A hard edge, black to white: both overshoot a little, Lanczos
        // more, and neither by much (at 3x, and Fit's 1.5x and 2.4x).
        let edge: Vec<f32> = (0..64).map(|i| if i < 32 { 0.0 } else { 1.0 }).collect();
        for screen in [192u32, 96, 154] {
            let over = |f| {
                filter_row(&edge, screen, f)
                    .iter()
                    .cloned()
                    .fold(0.0f32, f32::max)
                    - 1.0
            };
            let (cr, l) = (
                over(ScalingFilter::CatmullRom),
                over(ScalingFilter::Lanczos),
            );
            assert!(cr > 0.0 && cr < 0.1, "{screen}: {cr}");
            assert!(l > cr && l < 0.15, "{screen}: {l}");
        }
    }

    /// A `w` × `h` grey image (premultiplied, opaque) from `f`, edges
    /// clamped as the shader's reads are.
    fn image(w: i64, h: i64, f: impl Fn(i64, i64) -> f32) -> impl Fn(i64, i64) -> Vec4 {
        move |x, y| {
            let v = f(x.clamp(0, w - 1), y.clamp(0, h - 1));
            Vec4::new(v, v, v, 1.0)
        }
    }

    /// Output pixel `(x, y)` of the image shown at `s` times.
    fn shown(t: &impl Texels, x: u32, y: u32, s: f32) -> (Vec4, Vec4, f32) {
        let texel = (Vec2::new(x as f32, y as f32) + 0.5) / s;
        let (smooth, amount) = smooth_edges_sample(t, texel, Vec2::splat(s));
        (sharp_sample(t, texel, Vec2::splat(s)), smooth, amount)
    }

    #[test]
    fn smooth_edges_tuning_is_the_shaders() {
        let a = include_str!("sharp.wgsl");
        assert!(a.contains(&SmoothEdges::wgsl()), "{}", SmoothEdges::wgsl());
    }

    #[test]
    fn smooth_edges_is_sharp_bilinear_on_flat_areas_straight_edges_corners_and_dither() {
        let n = 32;
        type Art = Box<dyn Fn(i64, i64) -> f32>;
        let cases: [(&str, Art); 6] = [
            ("flat", Box::new(|_, _| 0.4)),
            (
                "vertical edge",
                Box::new(|x, _| if x < 15 { 0.1 } else { 0.9 }),
            ),
            (
                "horizontal edge",
                Box::new(|_, y| if y < 12 { 0.9 } else { 0.1 }),
            ),
            (
                "square",
                Box::new(|x, y| {
                    if (8..24).contains(&x) && (6..20).contains(&y) {
                        1.0
                    } else {
                        0.0
                    }
                }),
            ),
            ("checkerboard", Box::new(|x, y| ((x + y) % 2) as f32)),
            ("stripes", Box::new(|x, _| ((x / 2) % 2) as f32)),
        ];
        for (name, f) in cases {
            let t = image(n, n, f);
            for s in [3.0f32, 1.5, 2.0] {
                let size = (n as f32 * s) as u32;
                for y in 0..size {
                    for x in 0..size {
                        // The same on the 8-bit target.
                        let (sharp, smooth, _) = shown(&t, x, y, s);
                        let level = |v: Vec4| (v * 255.0).round();
                        assert_eq!(level(sharp), level(smooth), "{name} at {s}x: ({x}, {y})");
                    }
                }
            }
        }
    }

    /// Where each output row crosses half brightness on a dark-to-light
    /// edge, in output pixels, less the row's own position: a straight 45°
    /// edge gives the same number on every row.
    fn crossings(t: &impl Texels, s: f32, rows: std::ops::Range<u32>, filter: usize) -> Vec<f32> {
        rows.map(|y| {
            let v: Vec<f32> = (0..(48.0 * s) as u32)
                .map(|x| {
                    let (sharp, smooth, _) = shown(t, x, y, s);
                    [sharp, smooth][filter].x
                })
                .collect();
            let x = (0..v.len() - 1)
                .find(|&x| v[x] < 0.5 && v[x + 1] >= 0.5)
                .expect("the row crosses the edge");
            x as f32 + (0.5 - v[x]) / (v[x + 1] - v[x]) - y as f32
        })
        .collect()
    }

    #[test]
    fn smooth_edges_turns_a_45_degree_staircase_into_a_straight_edge() {
        let t = image(48, 48, |x, y| if x > y { 1.0 } else { 0.0 });
        let spread = |v: Vec<f32>| {
            v.iter().cloned().fold(f32::MIN, f32::max) - v.iter().cloned().fold(f32::MAX, f32::min)
        };
        for (s, sharp_at_least) in [(3.0f32, 1.9), (1.5, 0.45)] {
            let rows = (10.0 * s) as u32..(36.0 * s) as u32;
            // Sharp Bilinear: a staircase, one game pixel per step.
            let sharp = spread(crossings(&t, s, rows.clone(), 0));
            assert!(sharp >= sharp_at_least, "{s}x sharp: {sharp}");
            // Smooth Edges: straight to a hundredth of a screen pixel.
            let smooth = spread(crossings(&t, s, rows, 1));
            assert!(smooth < 0.02, "{s}x smooth: {smooth}");
        }
    }

    #[test]
    fn smooth_edges_keeps_a_thin_diagonal_line_as_heavy_and_as_sharp() {
        // A one-pixel line at 45°: each output row still holds one game
        // pixel's worth of it, and its middle stays at full brightness.
        let t = image(40, 40, |x, y| if x == y { 1.0 } else { 0.0 });
        let s = 3.0;
        for y in 30..90 {
            let row: Vec<f32> = (0..120).map(|x| shown(&t, x, y, s).1.x).collect();
            let mass: f32 = row.iter().sum::<f32>() / s;
            assert!((mass - 1.0).abs() < 0.02, "row {y}: {mass}");
            assert!(row.iter().cloned().fold(0.0, f32::max) > 0.95, "row {y}");
        }
    }

    #[test]
    fn smooth_edges_moves_with_the_picture() {
        // The same art one game pixel further on (the next frame of a
        // camera step) is the same picture `s` screen pixels further on:
        // nothing shimmers as the view moves.
        let art = |x: i64, y: i64| {
            let (dx, dy) = (x as f32 - 20.3, y as f32 - 17.6);
            if dx * dx + dy * dy < 90.0 {
                0.85
            } else {
                0.1 + 0.02 * (x % 3) as f32
            }
        };
        let a = image(96, 96, art);
        let b = image(96, 96, move |x, y| art(x - 1, y - 1));
        for step in [3u32, 2] {
            let s = step as f32;
            for y in 18..150 {
                for x in 18..150 {
                    // The same to float rounding (the sample positions
                    // differ in their last bits), far below one 8-bit level.
                    let (here, there) = (shown(&a, x, y, s).1, shown(&b, x + step, y + step, s).1);
                    assert!(
                        (here - there).abs().max_element() < 1e-5,
                        "({x}, {y}): {here} {there}"
                    );
                }
            }
        }
    }

    #[test]
    fn smooth_edges_has_no_seams_between_game_pixels() {
        // A disc: sampled every 1/64 of a game pixel, no step is bigger
        // than Sharp Bilinear's own blend ramp allows.
        let t = image(40, 40, |x, y| {
            let (dx, dy) = (x as f32 - 20.3, y as f32 - 19.7);
            if dx * dx + dy * dy < 120.0 {
                0.9
            } else {
                0.1
            }
        });
        let scale = Vec2::splat(3.0);
        let mut worst = 0.0f32;
        for row in 0..80 {
            let y = 5.0 + row as f32 * 0.37;
            let mut prev: Option<Vec4> = None;
            for k in 0..(30 * 64) {
                let p = Vec2::new(5.0 + k as f32 / 64.0, y);
                let (v, _) = smooth_edges_sample(&t, p, scale);
                if let Some(q) = prev {
                    worst = worst.max((v - q).abs().max_element());
                }
                prev = Some(v);
            }
        }
        assert!(worst < 0.05, "{worst}");
    }

    #[test]
    fn the_filter_reaches_the_scale_uniform() {
        for f in ScalingFilter::ALL {
            set_filter(f);
            assert_eq!(filter(), f);
            assert_eq!(scale(3.0, 3.0).filter.x, f32::from(f.index()));
        }
        set_filter(ScalingFilter::SharpBilinear);
        assert_eq!(scale(3.0, 3.0).filter, Vec4::ZERO);
    }

    #[test]
    fn presentation_shaders_validate_after_material_preprocessing() {
        for shader in [include_str!("sharp.wgsl"), include_str!("native_ui.wgsl")] {
            let shader = shader
                .replace("#import bevy_sprite::mesh2d_vertex_output::VertexOutput", "struct VertexOutput { @builtin(position) position: vec4<f32>, @location(0) uv: vec2<f32>, }")
                .replace("#{MATERIAL_BIND_GROUP}", "0");
            let module =
                wgpu::naga::front::wgsl::parse_str(&shader).expect("presentation shader parses");
            wgpu::naga::valid::Validator::new(
                wgpu::naga::valid::ValidationFlags::all(),
                wgpu::naga::valid::Capabilities::default(),
            )
            .validate(&module)
            .expect("presentation shader validates");
        }
    }

    #[test]
    fn picture_options_have_identity_defaults_and_distinct_slider_values() {
        assert_eq!(picture_adjustment_for(155, 100), (1.0, 1.0));
        for gamma in (55..255).step_by(10) {
            assert!(
                picture_adjustment_for(gamma, 100).0 > picture_adjustment_for(gamma + 10, 100).0
            );
        }
        for contrast in 0..100 {
            assert!(
                picture_adjustment_for(155, contrast).1
                    < picture_adjustment_for(155, contrast + 1).1
            );
        }
    }

    /// How much of each game pixel shows on screen: each screen pixel's
    /// colour split between the game pixels it blends, summed per game
    /// pixel. A row of distinct powers keeps the shares separable.
    fn widths(frame_w: usize, screen_w: u32) -> Vec<f32> {
        let mut out = vec![0.0; frame_w];
        for i in 0..frame_w {
            let mut row = vec![0.0; frame_w];
            row[i] = 1.0;
            out[i] = present_row(&row, screen_w).iter().sum();
        }
        out
    }

    #[test]
    fn fractional_scales_give_every_game_pixel_an_even_width() {
        // 600 lines at 1080, 1440 and 2160 (1.8, 2.4, 3.6), and the
        // 1068-wide 16:9 frame across the same screens.
        for (frame, screen) in [
            (600usize, 1080u32),
            (600, 1440),
            (600, 2160),
            (1068, 1920),
            (1068, 2560),
            (1068, 3840),
        ] {
            let s = screen as f32 / frame as f32;
            for (i, w) in widths(frame, screen).into_iter().enumerate() {
                assert!(
                    (w - s).abs() <= 0.25,
                    "{frame} -> {screen}: pixel {i} is {w} wide, scale {s}"
                );
            }
        }
    }

    #[test]
    fn nearest_at_a_fractional_scale_is_uneven_but_sharp_is_not() {
        // The reason for the sharp scaler: nearest at 1.8x draws game
        // pixels 1 or 2 screen pixels wide.
        let (frame, screen) = (600usize, 1080u32);
        let s = screen as f32 / frame as f32;
        let mut nearest = vec![0u32; frame];
        for x in 0..screen {
            nearest[((x as f32 + 0.5) / s) as usize] += 1;
        }
        assert!(nearest.contains(&1) && nearest.contains(&2));
        let sharp = widths(frame, screen);
        let spread = sharp.iter().cloned().fold(0.0f32, f32::max)
            - sharp.iter().cloned().fold(f32::MAX, f32::min);
        assert!(spread < 0.5, "{spread}");
    }

    /// Picture check against a separately implemented two-stage scaler:
    /// nearest integer enlargement, followed by linear sampling.
    #[test]
    fn fit_matches_integer_prescale_then_linear_at_every_target_size() {
        for (fw, fh) in [(800u32, 600u32), (1068, 600), (640, 480)] {
            let frame: Vec<f32> = (0..fw).map(|x| ((x * 37) % 256) as f32 / 255.0).collect();
            for (w, h) in [
                (1280, 720),
                (1920, 1080),
                (2560, 1440),
                (3840, 2160),
                (1365, 767),
                (1919, 1079),
            ] {
                let p = crate::ui::Presentation::with_mode(w, h, fw, fh, crate::ui::ScaleMode::Fit)
                    .unwrap();
                let k = p.sx.ceil().max(1.0) as usize;
                let prescaled: Vec<f32> = frame
                    .iter()
                    .flat_map(|v| std::iter::repeat_n(*v, k))
                    .collect();
                let at = |x: i64| prescaled[x.clamp(0, prescaled.len() as i64 - 1) as usize];
                for (x, actual) in present_row(&frame, p.image_w).iter().enumerate() {
                    let u = (x as f32 + 0.5) * prescaled.len() as f32 / p.image_w as f32 - 0.5;
                    let l = u.floor();
                    let expected = at(l as i64) * (1.0 - (u - l)) + at(l as i64 + 1) * (u - l);
                    assert!(
                        (actual - expected).abs() < 0.002,
                        "{fw}x{fh} -> {w}x{h}, x={x}: {actual} != {expected}"
                    );
                }
            }
        }
    }

    #[test]
    fn whole_scales_are_exactly_nearest() {
        let frame: Vec<f32> = (0..800).map(|i| (i * 37 % 256) as f32).collect();
        for k in [1u32, 2, 3] {
            let row = present_row(&frame, 800 * k);
            for (x, v) in row.iter().enumerate() {
                // Exact once written to an 8-bit target.
                assert_eq!(v.round(), frame[x / k as usize], "scale {k} pixel {x}");
            }
        }
    }

    #[test]
    fn a_whole_number_scale_samples_every_pixel_at_its_centre() {
        for s in 1..=5 {
            for k in 0..s {
                let d = (k as f32 + 0.5) / s as f32;
                assert_eq!(sharp_offset(d, s as f32), 0.5, "scale {s} pixel {k}");
            }
        }
    }

    #[test]
    fn the_edge_band_reaches_the_boundary() {
        assert!(sharp_offset(0.0, 2.4).abs() < 1e-3);
        assert!((sharp_offset(1.0, 2.4) - 1.0).abs() < 1e-3);
        assert_eq!(sharp_offset(0.5, 2.4), 0.5);
    }

    #[test]
    fn the_scale_uniform_is_at_least_one() {
        assert_eq!(scale(0.8, 0.8).scale.xy(), Vec2::new(1.0, 1.0));
        assert_eq!(scale(2.4, 2.4).scale.xy(), Vec2::new(2.4, 2.4));
    }
}
