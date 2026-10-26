//! The game view's post-processing pass: bloom, colour looks, colour
//! (brightness, saturation, vibrance), sharpen and FXAA on the scaled
//! picture at the output resolution, with the native UI laid on top
//! afterwards.
//!
//! Order: scale (the presenting shaders, `super::sharp`) → bloom → look →
//! colour → sharpen → FXAA → UI. The pass runs in Bevy's 2D
//! post-processing stage on the world view's own camera
//! ([`WorldViewCamera`]), as full-screen passes over the camera's picture
//! (`post.wgsl`): with Bloom on, the bright parts are taken at an eighth
//! of the size and blurred into two textures of the view's own
//! ([`BloomTextures`]); pass A (glow, look, colour and sharpen) runs when
//! any of them is set, pass B (FXAA, then the UI) whenever any effect is
//! on. Where the sharpen's reach is whole output pixels (1x to 4x),
//! pass A runs as a compute pass in tiles that grades each pixel once
//! for all its neighbours ([`tiles_fit`]), with the same picture to the
//! bit. Looks are 3D colour tables of our own (`super::looks`). While it runs, the native UI
//! material draws the picture without its UI and without the picture
//! options (`NativeUiMaterial` `flags.y`), and pass B lays the UI and
//! applies Gamma and Contrast exactly as that material would. The menus
//! and the front end have no post pass.
//!
//! With every effect off nothing is added: the camera has no
//! [`PostFx`], the presenting shaders draw as before, and the picture is
//! the same to the bit. With an effect on, the game view uses the native
//! UI pass at 1× too (`super::native_ui::view_placement`), so the UI stays
//! untouched at every size the frame is shown at or above. Only with
//! Sharp Interface off (`[video] native_ui`), or a window smaller than the
//! frame, is the UI part of the picture, and the effects touch it too.
//!
//! The effects are written from their published descriptions: luma is
//! Rec. 709; the sharpen is an unsharp mask over the four neighbours one
//! view pixel away (not one output pixel: the enlarged picture is flat
//! across each view pixel), limited to the 3 × 3's range; FXAA follows the published outline of the
//! algorithm (edge test on local contrast, edge direction from second
//! differences, an end search along the edge, a blend toward the far side
//! by the distance to the nearer end, plus sub-pixel blending).
//! [`colour`] and [`sharpen`] model the shader on the CPU for the tests.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::RwLock;

use bevy::asset::{embedded_asset, load_embedded_asset};
use bevy::core_pipeline::schedule::{Core2d, Core2dSystems};
use bevy::core_pipeline::tonemapping::tonemapping;
use bevy::core_pipeline::FullscreenShader;
use bevy::prelude::*;
use bevy::render::camera::ExtractedCamera;
use bevy::render::diagnostic::RecordDiagnostics;
use bevy::render::extract_component::{ExtractComponent, ExtractComponentPlugin};
use bevy::render::render_asset::RenderAssets;
use bevy::render::render_resource::binding_types::{
    sampler, storage_buffer_read_only_sized, storage_buffer_sized, texture_2d, texture_3d,
    texture_storage_2d, uniform_buffer_sized,
};
use bevy::render::render_resource::{
    BindGroupEntries, BindGroupLayoutDescriptor, BindGroupLayoutEntries, Buffer,
    BufferInitDescriptor, BufferUsages, CachedComputePipelineId, CachedRenderPipelineId,
    ColorTargetState, ColorWrites, ComputePassDescriptor, ComputePipelineDescriptor, FilterMode,
    FragmentState, Operations, PipelineCache, RenderPassColorAttachment, RenderPassDescriptor,
    RenderPipelineDescriptor, Sampler, SamplerBindingType, SamplerDescriptor, ShaderStages,
    SpecializedRenderPipeline, SpecializedRenderPipelines, StorageTextureAccess, Texture,
    TextureDescriptor, TextureDimension, TextureFormat, TextureSampleType, TextureUsages,
    TextureView, TextureViewDescriptor,
};
use bevy::render::renderer::{RenderContext, RenderDevice, ViewQuery};
use bevy::render::texture::{CachedTexture, FallbackImage, GpuImage, TextureCache};
use bevy::render::view::{ExtractedView, ViewTarget};
use bevy::render::{GpuResourceAppExt, Render, RenderApp, RenderStartup, RenderSystems};
use bevy::shader::Shader;
use bevy::window::PrimaryWindow;

use super::looks::{Look, LookTables};
use crate::app::config::{FxaaLevel, Settings};

/// The post-processing choices in force, in the shader's units.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PostOptions {
    /// Added to each encoded channel: −0.2 to 0.2 (Brightness 0 to 100).
    pub brightness: f32,
    /// The chroma scale: 0 (grey) to 2 (Saturation 0 to 200).
    pub saturation: f32,
    /// Extra saturation for dull colours: 0 to 1.
    pub vibrance: f32,
    /// Unsharp-mask strength: 0 to 2 (Sharpen 0 to 100; 50 is 1).
    pub sharpen: f32,
    pub fxaa: FxaaLevel,
    /// Glow strength: 0 (Bloom off) to 2 (Bloom Strength 100; 50 is 1).
    pub bloom: f32,
    pub look: Look,
}

impl PostOptions {
    /// Every effect off.
    pub const NEUTRAL: PostOptions = PostOptions {
        brightness: 0.0,
        saturation: 1.0,
        vibrance: 0.0,
        sharpen: 0.0,
        fxaa: FxaaLevel::Off,
        bloom: 0.0,
        look: Look::Original,
    };

    /// The options of `s`'s `[video]` keys.
    pub fn of(s: &Settings) -> Self {
        PostOptions {
            brightness: (f32::from(s.brightness) - 50.0) / 250.0,
            saturation: f32::from(s.saturation) / 100.0,
            vibrance: f32::from(s.vibrance) / 100.0,
            sharpen: f32::from(s.sharpen) / 50.0,
            fxaa: s.fxaa,
            bloom: if s.bloom {
                f32::from(s.bloom_strength) / 50.0
            } else {
                0.0
            },
            look: s.look,
        }
    }

    /// The colour or the sharpen pass has something to do.
    pub fn colour_or_sharpen(&self) -> bool {
        self.brightness != 0.0
            || self.saturation != 1.0
            || self.vibrance != 0.0
            || self.sharpen != 0.0
    }

    /// Pass A has something to do: glow, a look, colour or sharpen.
    pub fn pass_a(&self) -> bool {
        self.colour_or_sharpen() || self.bloom > 0.0 || self.look != Look::Original
    }

    /// Any effect is on: the pass runs.
    pub fn on(&self) -> bool {
        self.pass_a() || self.fxaa != FxaaLevel::Off
    }
}

static OPTIONS: RwLock<PostOptions> = RwLock::new(PostOptions::NEUTRAL);

/// Takes the post-processing choices of `s` (at start and on every
/// settings change; the next drawn frame shows them).
pub fn set_options(s: &Settings) {
    *OPTIONS.write().unwrap_or_else(|e| e.into_inner()) = PostOptions::of(s);
}

/// The post-processing choices in force.
pub fn options() -> PostOptions {
    *OPTIONS.read().unwrap_or_else(|e| e.into_inner())
}

/// The brightness (largest channel, linear light) the glow starts from;
/// a soft knee fades it in above.
pub const BLOOM_THRESHOLD: f32 = 0.45;

/// FXAA's shader parameters for `level`: search steps (0: off), edge
/// threshold as a share of the local brightest, the darkest contrast it
/// still smooths, and the sub-pixel amount. Higher levels follow edges
/// further and smooth fainter ones.
pub fn fxaa_params(level: FxaaLevel) -> [f32; 4] {
    match level {
        FxaaLevel::Off => [0.0, 1.0, 1.0, 0.0],
        FxaaLevel::Low => [4.0, 0.25, 0.0833, 0.5],
        FxaaLevel::Medium => [8.0, 0.166, 0.0625, 0.75],
        FxaaLevel::High => [12.0, 0.125, 0.0312, 0.75],
    }
}

/// The Rec. 709 luma of an encoded colour.
pub fn luma(g: [f32; 3]) -> f32 {
    0.2126 * g[0] + 0.7152 * g[1] + 0.0722 * g[2]
}

/// The shader's `colour`: brightness, then saturation scaled up for dull
/// colours by the vibrance, on an encoded colour.
pub fn colour(o: &PostOptions, g: [f32; 3]) -> [f32; 3] {
    let b = g.map(|c| c + o.brightness);
    let y = luma(b);
    let sat =
        b.iter().cloned().fold(f32::MIN, f32::max) - b.iter().cloned().fold(f32::MAX, f32::min);
    let k = o.saturation * (1.0 + o.vibrance * (1.0 - sat.clamp(0.0, 1.0)));
    b.map(|c| (y + (c - y) * k).clamp(0.0, 1.0))
}

/// The shader's sharpen of one channel: `m` plus `strength` times its
/// difference from the four neighbours' average (`cross`: up, down, left,
/// right), kept inside the range of the 3 × 3 (`cross` and the four
/// `corners`).
pub fn sharpen(m: f32, cross: [f32; 4], corners: [f32; 4], strength: f32) -> f32 {
    let blur = cross.iter().sum::<f32>() / 4.0;
    let all = cross.iter().chain(&corners).cloned();
    let lo = all.clone().fold(m, f32::min);
    let hi = all.fold(m, f32::max);
    (m + (m - blur) * strength).clamp(lo, hi)
}

/// The shader's sharpen of the pixel at (`x`, `y`) of an output picture
/// `at` (one channel, kept to the picture's edge by `at`), the neighbours
/// `step` output pixels away: one view pixel (`PostFx::new`'s `view`).
pub fn sharpen_at(at: &dyn Fn(i32, i32) -> f32, x: i32, y: i32, step: i32, strength: f32) -> f32 {
    let d = step.max(1);
    sharpen(
        at(x, y),
        [at(x, y - d), at(x, y + d), at(x - d, y), at(x + d, y)],
        [
            at(x - d, y - d),
            at(x + d, y + d),
            at(x + d, y - d),
            at(x - d, y + d),
        ],
        strength,
    )
}

/// Marks the world view's presenting camera, the one the pass runs on.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct WorldViewCamera;

/// The native UI's cover and add images (`super::native_ui::NativeUiTargets`).
pub type UiImages = (Handle<Image>, Handle<Image>);

/// The longest sharpen reach (output pixels per view pixel) pass A runs
/// in tiles for (`post.wgsl` `TILE_REACH`).
pub const TILE_REACH: f32 = 4.0;

/// Pass A runs in tiles (`colour_sharpen_tiles`) for options `o` and one
/// view pixel's size `view` in output pixels: when it sharpens and the
/// reach is whole output pixels on both axes (1x to 4x), so the
/// neighbours are other pixels' centres and each is graded once.
pub fn tiles_fit(o: &PostOptions, view: [f32; 2]) -> bool {
    o.sharpen > 0.0
        && view.iter().all(|&v| {
            let d = v.max(1.0);
            d.fract() == 0.0 && d <= TILE_REACH
        })
}

/// The pass for one camera, extracted to the render world: the shader's
/// uniform (`Post` in `post.wgsl`), whether pass A runs (and in tiles,
/// [`tiles_fit`]), whether the glow is made, the look's table, and the
/// native UI's two images when pass B lays them.
#[derive(Component, Clone, Debug, PartialEq, ExtractComponent)]
pub struct PostFx {
    pub uniform: [f32; 28],
    pub colour_pass: bool,
    pub tiles: bool,
    pub bloom: bool,
    pub look: Option<Handle<Image>>,
    pub ui: Option<UiImages>,
}

impl PostFx {
    /// The pass for options `o`; `ui`: the native UI's cover and add
    /// images and their place in the output (left, top, width, height),
    /// laid with the picture options `picture` (gamma exponent and
    /// contrast minus one); `image`: the scaled picture's place in the
    /// output (left, top, width, height), the part the effects touch;
    /// `view`: one view pixel's size in output pixels on each axis (the
    /// sharpen's reach); `look`: the look's table (none: Original).
    pub fn new(
        o: &PostOptions,
        ui: Option<(UiImages, [u32; 4])>,
        picture: (f32, f32),
        image: Option<[u32; 4]>,
        view: [f32; 2],
        look: Option<Handle<Image>>,
    ) -> Self {
        let image = image.map_or([0.0; 4], |r| r.map(|v| v as f32));
        let f = fxaa_params(o.fxaa);
        let (images, rect) = match ui {
            Some((images, rect)) => (Some(images), rect.map(|v| v as f32)),
            None => (None, [0.0; 4]),
        };
        let picture = if images.is_some() {
            [picture.0, picture.1, 0.0, 0.0]
        } else {
            [0.0; 4]
        };
        PostFx {
            uniform: [
                o.brightness,
                o.saturation,
                o.vibrance,
                o.sharpen,
                f[0],
                f[1],
                f[2],
                f[3],
                rect[0],
                rect[1],
                rect[2],
                rect[3],
                picture[0],
                picture[1],
                picture[2],
                picture[3],
                image[0],
                image[1],
                image[2],
                image[3],
                o.bloom,
                BLOOM_THRESHOLD,
                if look.is_some() { 1.0 } else { 0.0 },
                0.0,
                view[0],
                view[1],
                0.0,
                0.0,
            ],
            colour_pass: o.pass_a(),
            tiles: o.pass_a() && tiles_fit(o, view),
            bloom: o.bloom > 0.0,
            look,
            ui: images,
        }
    }
}

/// Puts the pass on the world view's camera when an effect is on, with
/// the native UI the frame's job lays (`super::node::ComposeJob::ui`), and
/// takes it off when every effect is off.
pub fn update_camera(
    mut commands: Commands,
    cameras: Query<(Entity, Option<&PostFx>), With<WorldViewCamera>>,
    job: Option<Res<super::node::ComposeJob>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    looks: Option<Res<LookTables>>,
) {
    let o = options();
    let p = windows
        .single()
        .ok()
        .and_then(|w| crate::ui::edge::presentation(w).ok());
    for (entity, fx) in &cameras {
        if !o.on() {
            if fx.is_some() {
                commands.entity(entity).remove::<PostFx>();
            }
            continue;
        }
        let ui = job
            .as_ref()
            .and_then(|j| j.ui.as_ref())
            .zip(p.as_ref())
            .map(|(u, p)| {
                (
                    (u.cover.clone(), u.add.clone()),
                    [p.left, p.top, p.image_w, p.image_h],
                )
            });
        let (gamma, contrast) = super::sharp::picture_adjustment();
        let image = p.as_ref().map(|p| [p.left, p.top, p.image_w, p.image_h]);
        let view = p.as_ref().map_or([1.0; 2], |p| [p.sx, p.sy]);
        let look = looks.as_ref().and_then(|t| t.get(o.look));
        let want = PostFx::new(&o, ui, (gamma - 1.0, contrast - 1.0), image, view, look);
        if fx != Some(&want) {
            commands.entity(entity).insert(want);
        }
    }
}

/// The passes' pipelines' shared parts.
#[derive(Resource)]
pub struct PostPipeline {
    bloom_layout: BindGroupLayoutDescriptor,
    colour_layout: BindGroupLayoutDescriptor,
    tiles_layout: BindGroupLayoutDescriptor,
    tiles: CachedComputePipelineId,
    probe_layout: BindGroupLayoutDescriptor,
    /// The coarse and the fine measurement ([`SrgbEdges`]).
    probe: [CachedRenderPipelineId; 2],
    edges_layout: BindGroupLayoutDescriptor,
    edges: [CachedComputePipelineId; 2],
    ui_layout: BindGroupLayoutDescriptor,
    sampler: Sampler,
    fullscreen: FullscreenShader,
    shader: Handle<Shader>,
}

/// The passes, in the order they run.
#[derive(PartialEq, Eq, Hash, Clone, Copy, Debug)]
pub enum PassKind {
    BloomDown,
    BloomBlurH,
    BloomBlurV,
    Colour,
    Ui,
}

impl PassKind {
    /// The pass's label (and its GPU time's diagnostic name) and entry point.
    fn names(self) -> (&'static str, &'static str) {
        match self {
            PassKind::BloomDown => ("d2_post_bloom_down", "bloom_down"),
            PassKind::BloomBlurH => ("d2_post_bloom_blur_h", "bloom_blur_h"),
            PassKind::BloomBlurV => ("d2_post_bloom_blur_v", "bloom_blur_v"),
            PassKind::Colour => ("d2_post_colour", "colour_sharpen"),
            PassKind::Ui => ("d2_post_fxaa_ui", "fxaa_ui"),
        }
    }
}

/// The glow's texture format: light past white survives.
pub const BLOOM_FORMAT: TextureFormat = TextureFormat::Rgba16Float;

/// Which pass, for which output format.
#[derive(PartialEq, Eq, Hash, Clone, Copy)]
pub struct PostKey {
    kind: PassKind,
    format: TextureFormat,
}

impl SpecializedRenderPipeline for PostPipeline {
    type Key = PostKey;

    fn specialize(&self, key: Self::Key) -> RenderPipelineDescriptor {
        let (label, entry) = key.kind.names();
        let layout = match key.kind {
            PassKind::BloomDown | PassKind::BloomBlurH | PassKind::BloomBlurV => {
                self.bloom_layout.clone()
            }
            PassKind::Colour => self.colour_layout.clone(),
            PassKind::Ui => self.ui_layout.clone(),
        };
        RenderPipelineDescriptor {
            label: Some(label.into()),
            layout: vec![layout],
            vertex: self.fullscreen.to_vertex_state(),
            fragment: Some(FragmentState {
                shader: self.shader.clone(),
                shader_defs: vec![],
                entry_point: Some(entry.into()),
                targets: vec![Some(ColorTargetState {
                    format: key.format,
                    blend: None,
                    write_mask: ColorWrites::ALL,
                })],
            }),
            ..default()
        }
    }
}

fn init_pipeline(
    mut commands: Commands,
    device: Res<RenderDevice>,
    fullscreen: Res<FullscreenShader>,
    assets: Res<AssetServer>,
    cache: Res<PipelineCache>,
) {
    let bloom_layout = BindGroupLayoutDescriptor::new(
        "d2_post_bloom_layout",
        &BindGroupLayoutEntries::sequential(
            ShaderStages::FRAGMENT,
            (
                texture_2d(TextureSampleType::Float { filterable: true }),
                sampler(SamplerBindingType::Filtering),
                uniform_buffer_sized(false, None),
            ),
        ),
    );
    let colour_layout = BindGroupLayoutDescriptor::new(
        "d2_post_colour_layout",
        &BindGroupLayoutEntries::with_indices(
            ShaderStages::FRAGMENT,
            (
                (0, texture_2d(TextureSampleType::Float { filterable: true })),
                (1, sampler(SamplerBindingType::Filtering)),
                (2, uniform_buffer_sized(false, None)),
                (5, texture_2d(TextureSampleType::Float { filterable: true })),
                (6, texture_3d(TextureSampleType::Float { filterable: true })),
            ),
        ),
    );
    let tiles_layout = BindGroupLayoutDescriptor::new(
        "d2_post_tiles_layout",
        &BindGroupLayoutEntries::with_indices(
            ShaderStages::COMPUTE,
            (
                (0, texture_2d(TextureSampleType::Float { filterable: true })),
                (1, sampler(SamplerBindingType::Filtering)),
                (2, uniform_buffer_sized(false, None)),
                (5, texture_2d(TextureSampleType::Float { filterable: true })),
                (6, texture_3d(TextureSampleType::Float { filterable: true })),
                (
                    7,
                    texture_storage_2d(TILES_FORMAT, StorageTextureAccess::WriteOnly),
                ),
                (8, storage_buffer_read_only_sized(false, None)),
            ),
        ),
    );
    let probe_layout = BindGroupLayoutDescriptor::new(
        "d2_post_srgb_probe_layout",
        &BindGroupLayoutEntries::with_indices(
            ShaderStages::FRAGMENT,
            ((8, storage_buffer_read_only_sized(false, None)),),
        ),
    );
    let edges_layout = BindGroupLayoutDescriptor::new(
        "d2_post_srgb_edges_layout",
        &BindGroupLayoutEntries::with_indices(
            ShaderStages::COMPUTE,
            (
                (9, texture_2d(TextureSampleType::Float { filterable: true })),
                (10, storage_buffer_sized(false, None)),
            ),
        ),
    );
    let shader = load_embedded_asset!(assets.as_ref(), "post.wgsl");
    let tiles = cache.queue_compute_pipeline(ComputePipelineDescriptor {
        label: Some("d2_post_colour_tiles".into()),
        layout: vec![tiles_layout.clone()],
        immediate_size: 0,
        shader: shader.clone(),
        shader_defs: vec![],
        entry_point: Some("colour_sharpen_tiles".into()),
        zero_initialize_workgroup_memory: false,
    });
    let probe = ["srgb_probe_coarse", "srgb_probe_fine"].map(|entry| {
        cache.queue_render_pipeline(RenderPipelineDescriptor {
            label: Some("d2_post_srgb_probe".into()),
            layout: vec![probe_layout.clone()],
            vertex: fullscreen.to_vertex_state(),
            fragment: Some(FragmentState {
                shader: shader.clone(),
                shader_defs: vec![],
                entry_point: Some(entry.into()),
                targets: vec![Some(ColorTargetState {
                    format: TextureFormat::Rgba8UnormSrgb,
                    blend: None,
                    write_mask: ColorWrites::ALL,
                })],
            }),
            ..default()
        })
    });
    let edges = ["srgb_edges_coarse", "srgb_edges_fine"].map(|entry| {
        cache.queue_compute_pipeline(ComputePipelineDescriptor {
            label: Some("d2_post_srgb_edges".into()),
            layout: vec![edges_layout.clone()],
            immediate_size: 0,
            shader: shader.clone(),
            shader_defs: vec![],
            entry_point: Some(entry.into()),
            zero_initialize_workgroup_memory: false,
        })
    });
    commands.insert_resource(SrgbEdges::new(&device));
    let ui_layout = BindGroupLayoutDescriptor::new(
        "d2_post_ui_layout",
        &BindGroupLayoutEntries::sequential(
            ShaderStages::FRAGMENT,
            (
                texture_2d(TextureSampleType::Float { filterable: true }),
                sampler(SamplerBindingType::Filtering),
                uniform_buffer_sized(false, None),
                texture_2d(TextureSampleType::Float { filterable: true }),
                texture_2d(TextureSampleType::Float { filterable: true }),
            ),
        ),
    );
    let sampler = device.create_sampler(&SamplerDescriptor {
        mag_filter: FilterMode::Linear,
        min_filter: FilterMode::Linear,
        ..default()
    });
    commands.insert_resource(PostPipeline {
        bloom_layout,
        colour_layout,
        tiles_layout,
        tiles,
        probe_layout,
        probe,
        edges_layout,
        edges,
        ui_layout,
        sampler,
        fullscreen: fullscreen.clone(),
        shader,
    });
}

/// The pipelines a camera's pass uses this frame, by [`PassKind`].
#[derive(Component)]
pub struct PostPipelines([CachedRenderPipelineId; 5]);

/// The view's two eighth-size glow textures: the bright parts go into
/// the first, the blur across into the second and back down into the
/// first, which pass A reads.
#[derive(Component)]
pub struct BloomTextures([CachedTexture; 2]);

/// Pass A's picture when it runs in tiles: the sRGB bytes the compute
/// pass stores ([`TILES_FORMAT`]), and the sRGB view pass B reads them
/// through, so pass B reads what it reads from the render target
/// otherwise.
#[derive(Component)]
pub struct TileTexture {
    texture: CachedTexture,
    srgb: TextureView,
}

/// The probe's size: [`PROBE_WIDTH`] values around each of the 255 byte
/// edges (`post.wgsl` `PROBE_WIDTH`).
pub const PROBE_WIDTH: u32 = 4096;

/// The exact sRGB encoding's byte edges, the centres of the measurement
/// (and the table until it is measured): edge `k` is the linear value
/// whose encoding is byte `k + 0.5`. Entries 0..255 and again 256..511
/// (`post.wgsl` `srgb`).
pub fn srgb_edge_centres() -> [f32; 512] {
    let mut table = [0.0; 512];
    for k in 0..255 {
        let s = (k as f64 + 0.5) / 255.0;
        let linear = if s <= 0.04045 {
            s / 12.92
        } else {
            ((s + 0.055) / 1.055).powf(2.4)
        };
        table[k] = linear as f32;
        table[256 + k] = linear as f32;
    }
    table
}

/// The render target's own sRGB encoding, measured once on the device
/// before pass A first runs in tiles: the compute pass can't write an
/// sRGB texture, so it encodes itself, and it has to round exactly as the
/// target does (the hardware's encoding is not the exact formula, and a
/// byte off at an edge is a different picture). An sRGB probe texture of
/// [`PROBE_WIDTH`] x 255 holds values around every edge, `srgb_edges`
/// finds where each byte starts: first coarsely, then one float step
/// apart (`post.wgsl`).
#[derive(Resource)]
pub struct SrgbEdges {
    buffer: Buffer,
    target: TextureView,
    bytes: TextureView,
    measured: AtomicBool,
    _probe: Texture,
}

impl SrgbEdges {
    fn new(device: &RenderDevice) -> Self {
        let table = srgb_edge_centres();
        let bytes: Vec<u8> = table.iter().flat_map(|v| v.to_le_bytes()).collect();
        let buffer = device.create_buffer_with_data(&BufferInitDescriptor {
            label: Some("d2_post_srgb_edges"),
            contents: &bytes,
            usage: BufferUsages::STORAGE,
        });
        let probe = device.create_texture(&TextureDescriptor {
            label: Some("d2_post_srgb_probe"),
            size: bevy::render::render_resource::Extent3d {
                width: PROBE_WIDTH,
                height: 255,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: TextureFormat::Rgba8UnormSrgb,
            usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::TEXTURE_BINDING,
            view_formats: &[TILES_FORMAT],
        });
        let target = probe.create_view(&TextureViewDescriptor::default());
        let bytes = probe.create_view(&TextureViewDescriptor {
            label: Some("d2_post_srgb_probe_bytes"),
            format: Some(TILES_FORMAT),
            usage: Some(TextureUsages::TEXTURE_BINDING),
            ..default()
        });
        SrgbEdges {
            buffer,
            target,
            bytes,
            measured: AtomicBool::new(false),
            _probe: probe,
        }
    }
}

/// The tiles' storage format: the render target's bytes without the sRGB
/// encoding (storage textures can't be sRGB; `post.wgsl` encodes).
pub const TILES_FORMAT: TextureFormat = TextureFormat::Rgba8Unorm;

/// The tiles' texture for cameras whose pass A runs in tiles
/// ([`PostFx::tiles`]) onto an sRGB byte target, the encoding the shader
/// writes; any other target keeps the full-screen pass A.
fn prepare_tile_textures(
    mut commands: Commands,
    mut cache: ResMut<TextureCache>,
    device: Res<RenderDevice>,
    cameras: Query<(Entity, &ExtractedCamera, &ExtractedView, &PostFx)>,
) {
    for (entity, camera, view, fx) in &cameras {
        let Some(size) = camera
            .physical_target_size
            .filter(|_| fx.tiles && view.target_format == TextureFormat::Rgba8UnormSrgb)
        else {
            commands.entity(entity).remove::<TileTexture>();
            continue;
        };
        let texture = cache.get(
            &device,
            TextureDescriptor {
                label: Some("d2_post_tiles"),
                size: bevy::render::render_resource::Extent3d {
                    width: size.x,
                    height: size.y,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: TextureDimension::D2,
                format: TILES_FORMAT,
                usage: TextureUsages::STORAGE_BINDING | TextureUsages::TEXTURE_BINDING,
                view_formats: &[TextureFormat::Rgba8UnormSrgb],
            },
        );
        let srgb = texture.texture.create_view(&TextureViewDescriptor {
            label: Some("d2_post_tiles_srgb"),
            format: Some(TextureFormat::Rgba8UnormSrgb),
            usage: Some(TextureUsages::TEXTURE_BINDING),
            ..default()
        });
        commands
            .entity(entity)
            .insert(TileTexture { texture, srgb });
    }
}

/// An eighth of the view's size (at least one texel) for the glow: a
/// wide halo for few texels.
pub fn bloom_size(view: UVec2) -> UVec2 {
    (view / 8).max(UVec2::ONE)
}

fn prepare_bloom_textures(
    mut commands: Commands,
    mut cache: ResMut<TextureCache>,
    device: Res<RenderDevice>,
    cameras: Query<(Entity, &ExtractedCamera, &PostFx)>,
) {
    for (entity, camera, fx) in &cameras {
        let Some(size) = camera.physical_viewport_size.filter(|_| fx.bloom) else {
            commands.entity(entity).remove::<BloomTextures>();
            continue;
        };
        let size = bloom_size(size);
        let texture = |cache: &mut TextureCache| {
            cache.get(
                &device,
                TextureDescriptor {
                    label: Some("d2_post_bloom"),
                    size: bevy::render::render_resource::Extent3d {
                        width: size.x,
                        height: size.y,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: TextureDimension::D2,
                    format: BLOOM_FORMAT,
                    usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                },
            )
        };
        let textures = [texture(&mut cache), texture(&mut cache)];
        commands.entity(entity).insert(BloomTextures(textures));
    }
}

fn prepare_pipelines(
    mut commands: Commands,
    cache: Res<PipelineCache>,
    mut pipelines: ResMut<SpecializedRenderPipelines<PostPipeline>>,
    post: Res<PostPipeline>,
    cameras: Query<(Entity, &ExtractedView), With<PostFx>>,
) {
    for (entity, view) in &cameras {
        let ids = [
            (PassKind::BloomDown, BLOOM_FORMAT),
            (PassKind::BloomBlurH, BLOOM_FORMAT),
            (PassKind::BloomBlurV, BLOOM_FORMAT),
            (PassKind::Colour, view.target_format),
            (PassKind::Ui, view.target_format),
        ]
        .map(|(kind, format)| pipelines.specialize(&cache, &post, PostKey { kind, format }));
        commands.entity(entity).insert(PostPipelines(ids));
    }
}

/// What the passes read of their camera.
type PostView = (
    &'static ViewTarget,
    &'static PostFx,
    &'static PostPipelines,
    Option<&'static BloomTextures>,
    Option<&'static TileTexture>,
);

/// Records the camera's passes: the glow (when on), then A (when set) and
/// B, A and B each from the camera's picture into its other main texture;
/// A in tiles ([`PostFx::tiles`]) goes into the tiles' texture instead,
/// and B reads it from there.
#[allow(clippy::too_many_arguments)]
fn post_pass(
    view: ViewQuery<PostView>,
    post: Res<PostPipeline>,
    cache: Res<PipelineCache>,
    images: Res<RenderAssets<GpuImage>>,
    fallback: Res<FallbackImage>,
    srgb: Res<SrgbEdges>,
    mut ctx: RenderContext,
) {
    let (target, fx, ids, bloom, tiles) = view.into_inner();
    let mut pipelines = Vec::with_capacity(5);
    for id in ids.0 {
        let Some(p) = cache.get_render_pipeline(id) else {
            return;
        };
        pipelines.push(p);
    }
    let pipeline = |kind: PassKind| pipelines[kind as usize];
    let bytes: Vec<u8> = fx.uniform.iter().flat_map(|v| v.to_le_bytes()).collect();
    let uniform = ctx
        .render_device()
        .create_buffer_with_data(&BufferInitDescriptor {
            label: Some("d2_post_uniform"),
            contents: &bytes,
            usage: BufferUsages::UNIFORM,
        });
    let (cover, add) = match &fx.ui {
        Some((c, a)) => match (images.get(c), images.get(a)) {
            (Some(c), Some(a)) => (&c.texture_view, &a.texture_view),
            _ => (&fallback.d2.texture_view, &fallback.d2.texture_view),
        },
        None => (&fallback.d2.texture_view, &fallback.d2.texture_view),
    };
    let look = fx
        .look
        .as_ref()
        .and_then(|h| images.get(h))
        .map_or(&fallback.d3.texture_view, |g| &g.texture_view);
    let glow = bloom.filter(|_| fx.bloom);
    let glow_view = glow.map_or(&fallback.d2.texture_view, |t| &t.0[0].default_view);
    // Records one full-screen pass of `kind` into `dest`.
    let run = |ctx: &mut RenderContext,
               kind: PassKind,
               dest: &bevy::render::render_resource::TextureView,
               bind_group: &bevy::render::render_resource::BindGroup| {
        let (label, _) = kind.names();
        // GPU time per pass, recorded only with the render diagnostics
        // (`D2_POST_TIMING`, [`add_timing`]).
        let diagnostics = ctx.diagnostic_recorder();
        let diagnostics = diagnostics.as_deref();
        let mut pass = ctx
            .command_encoder()
            .begin_render_pass(&RenderPassDescriptor {
                label: Some(label),
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: dest,
                    depth_slice: None,
                    resolve_target: None,
                    ops: Operations::default(),
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
        let span = diagnostics.pass_span(&mut pass, label);
        pass.set_pipeline(pipeline(kind));
        pass.set_bind_group(0, bind_group, &[]);
        pass.draw(0..3, 0..1);
        span.end(&mut pass);
    };
    if let Some(t) = glow {
        let bloom_layout = cache.get_bind_group_layout(&post.bloom_layout);
        let steps = [
            (PassKind::BloomDown, target.main_texture_view(), &t.0[0]),
            (PassKind::BloomBlurH, &t.0[0].default_view, &t.0[1]),
            (PassKind::BloomBlurV, &t.0[1].default_view, &t.0[0]),
        ];
        for (kind, source, dest) in steps {
            let bind_group = ctx.render_device().create_bind_group(
                "d2_post_bloom",
                &bloom_layout,
                &BindGroupEntries::sequential((source, &post.sampler, uniform.as_entire_binding())),
            );
            run(&mut ctx, kind, &dest.default_view, &bind_group);
        }
    }
    let tiled = tiles
        .filter(|_| fx.colour_pass && fx.tiles)
        .and_then(|t| cache.get_compute_pipeline(post.tiles).map(|p| (t, p)))
        .filter(|_| measure_srgb(&mut ctx, &post, &cache, &srgb));
    if let Some((t, tiles_pipeline)) = tiled {
        // Pass A in tiles, from the camera's picture into the tiles'
        // texture, which pass B reads.
        let layout = cache.get_bind_group_layout(&post.tiles_layout);
        let bind_group = ctx.render_device().create_bind_group(
            "d2_post_tiles",
            &layout,
            &BindGroupEntries::with_indices((
                (0, target.main_texture_view()),
                (1, &post.sampler),
                (2, uniform.as_entire_binding()),
                (5, glow_view),
                (6, look),
                (7, &t.texture.default_view),
                (8, srgb.buffer.as_entire_binding()),
            )),
        );
        let size = t.texture.texture.size();
        let (label, _) = PassKind::Colour.names();
        let diagnostics = ctx.diagnostic_recorder();
        let diagnostics = diagnostics.as_deref();
        let mut pass = ctx
            .command_encoder()
            .begin_compute_pass(&ComputePassDescriptor {
                label: Some(label),
                timestamp_writes: None,
            });
        let span = diagnostics.pass_span(&mut pass, label);
        pass.set_pipeline(tiles_pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.dispatch_workgroups(size.width.div_ceil(16), size.height.div_ceil(16), 1);
        span.end(&mut pass);
    } else if fx.colour_pass {
        let write = target.post_process_write();
        let layout = cache.get_bind_group_layout(&post.colour_layout);
        let bind_group = ctx.render_device().create_bind_group(
            "d2_post_colour",
            &layout,
            &BindGroupEntries::with_indices((
                (0, write.source),
                (1, &post.sampler),
                (2, uniform.as_entire_binding()),
                (5, glow_view),
                (6, look),
            )),
        );
        run(&mut ctx, PassKind::Colour, write.destination, &bind_group);
    }
    let write = target.post_process_write();
    let source = tiled.map_or(write.source, |(t, _)| &t.srgb);
    let layout = cache.get_bind_group_layout(&post.ui_layout);
    let bind_group = ctx.render_device().create_bind_group(
        "d2_post_ui",
        &layout,
        &BindGroupEntries::sequential((
            source,
            &post.sampler,
            uniform.as_entire_binding(),
            cover,
            add,
        )),
    );
    run(&mut ctx, PassKind::Ui, write.destination, &bind_group);
}

/// Measures the target's sRGB encoding ([`SrgbEdges`]) once, into this
/// frame's commands ahead of pass A; false while its pipelines are not
/// ready (pass A then runs full screen).
fn measure_srgb(
    ctx: &mut RenderContext,
    post: &PostPipeline,
    cache: &PipelineCache,
    srgb: &SrgbEdges,
) -> bool {
    if srgb.measured.load(Ordering::Relaxed) {
        return true;
    }
    let (Some(probe), Some(edges)) = (
        post.probe
            .iter()
            .map(|&id| cache.get_render_pipeline(id))
            .collect::<Option<Vec<_>>>(),
        post.edges
            .iter()
            .map(|&id| cache.get_compute_pipeline(id))
            .collect::<Option<Vec<_>>>(),
    ) else {
        return false;
    };
    let device = ctx.render_device().clone();
    let probe_group = device.create_bind_group(
        "d2_post_srgb_probe",
        &cache.get_bind_group_layout(&post.probe_layout),
        &BindGroupEntries::with_indices(((8, srgb.buffer.as_entire_binding()),)),
    );
    let edges_group = device.create_bind_group(
        "d2_post_srgb_edges",
        &cache.get_bind_group_layout(&post.edges_layout),
        &BindGroupEntries::with_indices(((9, &srgb.bytes), (10, srgb.buffer.as_entire_binding()))),
    );
    let encoder = ctx.command_encoder();
    for (probe, edges) in probe.into_iter().zip(edges) {
        let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("d2_post_srgb_probe"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view: &srgb.target,
                depth_slice: None,
                resolve_target: None,
                ops: Operations::default(),
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(probe);
        pass.set_bind_group(0, &probe_group, &[]);
        pass.draw(0..3, 0..1);
        drop(pass);
        let mut pass = encoder.begin_compute_pass(&ComputePassDescriptor {
            label: Some("d2_post_srgb_edges"),
            timestamp_writes: None,
        });
        pass.set_pipeline(edges);
        pass.set_bind_group(0, &edges_group, &[]);
        pass.dispatch_workgroups(255u32.div_ceil(64), 1, 1);
    }
    srgb.measured.store(true, Ordering::Relaxed);
    true
}

/// `D2_POST_TIMING=1`: records the GPU time of the post passes (Bevy's
/// render diagnostics, timestamp queries on Vulkan and DX12) and logs
/// their running means every 300 frames, for the frame-time check.
pub fn add_timing(app: &mut App) {
    if std::env::var_os("D2_POST_TIMING").is_none() || app.get_sub_app(RenderApp).is_none() {
        return;
    }
    app.add_plugins(bevy::render::diagnostic::RenderDiagnosticsPlugin)
        .add_systems(Last, log_timing);
}

fn log_timing(store: Res<bevy::diagnostic::DiagnosticsStore>, mut frames: Local<u32>) {
    *frames += 1;
    if !(*frames).is_multiple_of(300) {
        return;
    }
    let mut line = format!("post timing: frame {}", *frames);
    for kind in [
        PassKind::BloomDown,
        PassKind::BloomBlurH,
        PassKind::BloomBlurV,
        PassKind::Colour,
        PassKind::Ui,
    ] {
        let pass = kind.names().0;
        for field in ["elapsed_gpu", "elapsed_cpu"] {
            let path = bevy::diagnostic::DiagnosticPath::from_components(["render", pass, field]);
            if let Some(ms) = store.get(&path).and_then(|d| d.average()) {
                line.push_str(&format!(" {pass}/{field}={ms:.4}ms"));
            }
        }
    }
    info!("{line}");
}

/// Bakes the looks' tables once (`super::looks`).
fn init_looks(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    commands.insert_resource(super::looks::make_tables(&mut images));
}

/// Adds the pass when the app renders.
pub fn add_post(app: &mut App) {
    if app.get_sub_app(RenderApp).is_none() {
        return;
    }
    embedded_asset!(app, "post.wgsl");
    app.add_plugins(ExtractComponentPlugin::<PostFx>::default())
        .add_systems(Startup, init_looks)
        .add_systems(Update, update_camera);
    add_timing(app);
    let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
        return;
    };
    render_app
        .init_gpu_resource::<SpecializedRenderPipelines<PostPipeline>>()
        .add_systems(RenderStartup, init_pipeline)
        .add_systems(Render, prepare_pipelines.in_set(RenderSystems::Prepare))
        .add_systems(
            Render,
            (prepare_bloom_textures, prepare_tile_textures).in_set(RenderSystems::PrepareResources),
        )
        .add_systems(
            Core2d,
            post_pass
                .after(tonemapping)
                .in_set(Core2dSystems::PostProcess),
        );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_post_shader_validates() {
        let shader = include_str!("post.wgsl").replace(
            "#import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput",
            "struct FullscreenVertexOutput { @builtin(position) position: vec4<f32>, @location(0) uv: vec2<f32>, }",
        );
        let module = wgpu::naga::front::wgsl::parse_str(&shader).expect("post shader parses");
        wgpu::naga::valid::Validator::new(
            wgpu::naga::valid::ValidationFlags::all(),
            wgpu::naga::valid::Capabilities::default(),
        )
        .validate(&module)
        .expect("post shader validates");
    }

    /// The default look F (Vivid, Sharpen 25) runs pass A; Look Original
    /// and Sharpen 0 with every other default turn every effect off.
    #[test]
    fn default_settings_run_the_default_look_and_original_turns_every_effect_off() {
        let d = PostOptions::of(&Settings::default());
        assert_eq!(
            d,
            PostOptions {
                sharpen: 0.5,
                look: Look::Vivid,
                ..PostOptions::NEUTRAL
            }
        );
        assert!(d.on() && d.colour_or_sharpen() && d.pass_a());
        let neutral = Settings {
            look: Look::Original,
            sharpen: 0,
            ..Settings::default()
        };
        let o = PostOptions::of(&neutral);
        assert_eq!(o, PostOptions::NEUTRAL);
        assert!(!o.on() && !o.colour_or_sharpen());
        for (key, ..) in crate::app::config::POST_KEYS {
            if key == "bloom_strength" {
                continue;
            }
            let mut s = neutral;
            let v = if key == "brightness" {
                51
            } else {
                s.post_value(key) + 1
            };
            s.set_post_value(key, v);
            let o = PostOptions::of(&s);
            assert!(o.on() && o.colour_or_sharpen(), "{key}");
        }
        for level in [FxaaLevel::Low, FxaaLevel::Medium, FxaaLevel::High] {
            let o = PostOptions::of(&Settings {
                fxaa: level,
                ..neutral
            });
            assert!(o.on() && !o.colour_or_sharpen() && !o.pass_a());
        }
        // Bloom strength alone is nothing until Bloom is on; Bloom on is
        // pass A, and so is any look but Original.
        let strength = Settings {
            bloom_strength: 80,
            ..neutral
        };
        assert!(!PostOptions::of(&strength).on());
        let bloom = PostOptions::of(&Settings {
            bloom: true,
            ..strength
        });
        assert!(bloom.pass_a() && (bloom.bloom - 1.6).abs() < 1e-6);
        for look in Look::ALL {
            let o = PostOptions::of(&Settings { look, ..neutral });
            assert_eq!(o.pass_a(), look != Look::Original, "{look:?}");
        }
    }

    #[test]
    fn neutral_colour_is_identity_and_the_sliders_do_what_they_say() {
        let n = PostOptions::NEUTRAL;
        for g in [
            [0.0, 0.0, 0.0],
            [1.0, 1.0, 1.0],
            [0.2, 0.5, 0.9],
            [0.7, 0.1, 0.3],
        ] {
            // Within rounding (pass A doesn't run at all when neutral).
            for (a, b) in colour(&n, g).iter().zip(g) {
                assert!((a - b).abs() < 1e-6, "{g:?}");
            }
        }
        let with = |f: fn(&mut PostOptions)| {
            let mut o = PostOptions::NEUTRAL;
            f(&mut o);
            o
        };
        // Saturation 0 is grey at the colour's own luma.
        let grey = colour(&with(|o| o.saturation = 0.0), [0.2, 0.5, 0.9]);
        assert!((grey[0] - grey[1]).abs() < 1e-6 && (grey[1] - grey[2]).abs() < 1e-6);
        assert!((grey[0] - luma([0.2, 0.5, 0.9])).abs() < 1e-6);
        // Brightness lifts every channel.
        let up = colour(&with(|o| o.brightness = 0.1), [0.2, 0.5, 0.6]);
        assert!(up.iter().zip([0.2, 0.5, 0.6]).all(|(a, b)| a > &b));
        // Vibrance boosts a dull colour's chroma more than a strong one's.
        let v = with(|o| o.vibrance = 1.0);
        let chroma = |g: [f32; 3]| g[0] - g[2];
        let dull = [0.5, 0.45, 0.4];
        let strong = [0.9, 0.5, 0.1];
        let gain = |g: [f32; 3]| chroma(colour(&v, g)) / chroma(g);
        assert!(gain(dull) > gain(strong) && gain(strong) >= 1.0);
    }

    #[test]
    fn the_sharpen_leaves_flat_areas_and_never_overshoots_its_neighbours() {
        assert_eq!(sharpen(0.4, [0.4; 4], [0.4; 4], 2.0), 0.4);
        // A bright pixel among darker ones gets brighter, but no brighter
        // than the brightest of the 3 x 3.
        let s = sharpen(0.6, [0.4, 0.6, 0.6, 0.6], [0.6; 4], 2.0);
        assert!((0.6..=0.6 + 1e-6).contains(&s));
        let s = sharpen(0.6, [0.4, 0.6, 0.6, 0.6], [0.6, 0.7, 0.6, 0.6], 2.0);
        assert!(s > 0.6 && s <= 0.7);
        let s = sharpen(0.5, [0.4, 0.6, 0.6, 0.6], [0.5; 4], 2.0);
        assert!((0.4..=0.6).contains(&s));
        let mid = sharpen(0.5, [0.3, 0.7, 0.45, 0.45], [0.5; 4], 1.0);
        assert!((0.3..=0.7).contains(&mid));
        // Strength 0: unchanged.
        assert_eq!(sharpen(0.55, [0.1, 0.9, 0.2, 0.3], [0.0; 4], 0.0), 0.55);
        // Sharpen 50 is strength 1, Sharpen 100 strength 2.
        let at = |v| {
            PostOptions::of(&Settings {
                sharpen: v,
                ..Settings::default()
            })
            .sharpen
        };
        assert_eq!((at(0), at(25), at(50), at(100)), (0.0, 0.5, 1.0, 2.0));
    }

    /// QA-5: at an exact whole scale the enlarged picture is flat across
    /// each view pixel, so a sharpen that looks one output pixel away
    /// finds the same colour and leaves the picture as it was. Looking one
    /// view pixel away sharpens it, and keeps each view pixel's block whole.
    #[test]
    fn the_sharpen_works_at_exact_whole_scales() {
        const N: i32 = 16;
        let mut seed = 7u32;
        let view: Vec<f32> = (0..N * N)
            .map(|_| {
                seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                0.2 + 0.6 * (seed >> 8) as f32 / (1u32 << 24) as f32
            })
            .collect();
        for scale in [2, 3] {
            let size = N * scale;
            let out = |x: i32, y: i32| {
                let (x, y) = (x.clamp(0, size - 1) / scale, y.clamp(0, size - 1) / scale);
                view[(y * N + x) as usize]
            };
            let centres = (1..N - 1).flat_map(|vy| (1..N - 1).map(move |vx| (vx, vy)));
            let changed = |step: i32| {
                centres
                    .clone()
                    .filter(|&(vx, vy)| {
                        let (x, y) = (vx * scale + scale / 2, vy * scale + scale / 2);
                        (sharpen_at(&out, x, y, step, 1.0) - out(x, y)).abs() > 1.0 / 255.0
                    })
                    .count()
            };
            let all = ((N - 2) * (N - 2)) as usize;
            // One output pixel away (the bug): nothing at a block's centre.
            if scale == 3 {
                assert_eq!(changed(1), 0, "{scale}x");
            }
            // One view pixel away: most of the picture.
            assert!(
                changed(scale) * 2 > all,
                "{scale}x: {} of {all}",
                changed(scale)
            );
            // Every pixel of a block gets its centre's value: no new edges
            // inside a view pixel.
            for (vx, vy) in centres.clone() {
                let c = sharpen_at(&out, vx * scale, vy * scale, scale, 1.0);
                for (dx, dy) in (0..scale).flat_map(|dy| (0..scale).map(move |dx| (dx, dy))) {
                    let v = sharpen_at(&out, vx * scale + dx, vy * scale + dy, scale, 1.0);
                    assert_eq!(v, c, "{scale}x block ({vx}, {vy})");
                }
            }
        }
    }

    #[test]
    fn fxaa_levels_search_further_and_smooth_fainter_edges() {
        assert_eq!(fxaa_params(FxaaLevel::Off)[0], 0.0);
        let levels = [FxaaLevel::Low, FxaaLevel::Medium, FxaaLevel::High];
        for w in levels.windows(2) {
            let (a, b) = (fxaa_params(w[0]), fxaa_params(w[1]));
            assert!(b[0] > a[0] && b[1] < a[1] && b[2] < a[2] && b[3] >= a[3]);
        }
    }

    #[test]
    fn the_uniform_carries_the_ui_rect_and_picture_options_only_with_the_ui() {
        let o = PostOptions {
            sharpen: 0.5,
            ..PostOptions::NEUTRAL
        };
        let none = PostFx::new(&o, None, (0.3, -0.2), None, [1.0; 2], None);
        assert_eq!(&none.uniform[8..20], &[0.0; 12]);
        assert!(none.colour_pass && none.ui.is_none() && !none.bloom);
        assert_eq!(&none.uniform[20..24], &[0.0, BLOOM_THRESHOLD, 0.0, 0.0]);
        assert_eq!(&none.uniform[24..], &[1.0, 1.0, 0.0, 0.0]);
        let h = (Handle::<Image>::default(), Handle::<Image>::default());
        let with = PostFx::new(
            &o,
            Some((h, [318, 180, 3204, 1800])),
            (0.3, -0.2),
            Some([318, 180, 3204, 1800]),
            [3.0, 3.0],
            None,
        );
        assert_eq!(&with.uniform[16..20], &[318.0, 180.0, 3204.0, 1800.0]);
        assert_eq!(&with.uniform[24..26], &[3.0, 3.0]);
        assert_eq!(
            &with.uniform[8..14],
            &[318.0, 180.0, 3204.0, 1800.0, 0.3, -0.2]
        );
        assert_eq!(with.uniform[3], 0.5);
        let glow = PostOptions {
            bloom: 0.5,
            ..PostOptions::NEUTRAL
        };
        let fx = PostFx::new(
            &glow,
            None,
            (0.0, 0.0),
            None,
            [1.5, 1.5],
            Some(Handle::default()),
        );
        assert!(fx.bloom && fx.colour_pass && fx.look.is_some());
        assert_eq!(&fx.uniform[20..24], &[0.5, BLOOM_THRESHOLD, 1.0, 0.0]);
    }

    #[test]
    fn pass_a_runs_in_tiles_where_the_sharpen_reaches_whole_pixels() {
        let sharpen = PostOptions {
            sharpen: 0.8,
            ..PostOptions::NEUTRAL
        };
        for view in [[1.0, 1.0], [0.5, 0.5], [2.0, 2.0], [3.0, 3.0], [4.0, 4.0]] {
            assert!(tiles_fit(&sharpen, view), "{view:?}");
        }
        for view in [[1.5, 1.5], [3.0, 2.5], [1.0667, 1.0667], [5.0, 5.0]] {
            assert!(!tiles_fit(&sharpen, view), "{view:?}");
        }
        let colour = PostOptions {
            brightness: 0.1,
            ..PostOptions::NEUTRAL
        };
        assert!(
            !tiles_fit(&colour, [3.0, 3.0]),
            "no sharpen: one read a pixel"
        );
        let fx = PostFx::new(&sharpen, None, (0.0, 0.0), None, [3.0, 3.0], None);
        assert!(fx.colour_pass && fx.tiles);
        let fx = PostFx::new(&sharpen, None, (0.0, 0.0), None, [1.5, 1.5], None);
        assert!(fx.colour_pass && !fx.tiles);
    }

    #[test]
    fn the_srgb_edges_start_from_where_each_byte_begins() {
        let encode = |v: f32| {
            let v = v as f64;
            let s = if v <= 0.0031308 {
                v * 12.92
            } else {
                1.055 * v.powf(1.0 / 2.4) - 0.055
            };
            (s * 255.0).round() as u32
        };
        let table = srgb_edge_centres();
        assert_eq!(table[..255], table[256..511]);
        for (k, &edge) in table[..255].iter().enumerate() {
            let below = f32::from_bits(edge.to_bits() - 1);
            let above = f32::from_bits(edge.to_bits() + 1);
            assert_eq!(encode(below), k as u32, "edge {k}");
            assert_eq!(encode(above), k as u32 + 1, "edge {k}");
        }
    }

    #[test]
    fn the_glow_is_an_eighth_of_the_view() {
        assert_eq!(bloom_size(UVec2::new(3840, 2160)), UVec2::new(480, 270));
        assert_eq!(bloom_size(UVec2::new(1366, 768)), UVec2::new(170, 96));
        assert_eq!(bloom_size(UVec2::new(2, 3)), UVec2::ONE);
    }
}
