//! The presenting shaders' shared filter function on a real adapter
//! (local, needs a GPU): its output against the CPU models in `sharp.rs`,
//! and what each Scaling Filter costs at 2160p.

use super::*;

/// The shared filter block between the bindings it reads and a full-screen
/// triangle; `fs` samples `frame` at each target pixel's centre.
fn shader() -> String {
    let src = include_str!("sharp.wgsl");
    let block = &src[src.find(FILTER_FN_START).expect("start marker")
        ..src.find(FILTER_FN_END).expect("end marker")];
    format!(
        "struct Present {{ scale: vec4<f32>, kind: vec4<f32>, }}
@group(0) @binding(0) var frame: texture_2d<f32>;
@group(0) @binding(1) var frame_sampler: sampler;
@group(0) @binding(2) var<uniform> present: Present;
@group(0) @binding(3) var<uniform> target_size: vec4<f32>;
const SOLID_SLACK: f32 = {SOLID_SLACK:?};
{block}
@vertex
fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {{
    let p = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    return vec4<f32>(p * 2.0 - 1.0, 0.0, 1.0);
}}
@fragment
fn fs(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {{
    return present_sample(frame, pos.xy / target_size.xy);
}}
"
    )
}

/// Test art, premultiplied RGBA8: discs, bars at several angles, a
/// one-pixel diagonal, noise, a ramp and a transparent corner.
fn art(w: u32, h: u32) -> Vec<u8> {
    let mut out = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        for x in 0..w {
            let (fx, fy) = (x as f32, y as f32);
            let noise =
                ((x.wrapping_mul(73_856_093) ^ y.wrapping_mul(19_349_663)) % 97) as f32 / 97.0;
            let mut c = [0.15 + 0.1 * noise, 0.2, 0.1 + 0.3 * fx / w as f32];
            let disc = |cx: f32, cy: f32, r: f32| (fx - cx).powi(2) + (fy - cy).powi(2) < r * r;
            if disc(w as f32 * 0.3, h as f32 * 0.4, h as f32 * 0.25) {
                c = [0.9, 0.7, 0.3];
            }
            if disc(w as f32 * 0.75, h as f32 * 0.6, h as f32 * 0.18) {
                c = [0.2, 0.5, 0.9];
            }
            for (k, slope) in [0.5f32, 1.0, 2.0, -1.0, -0.33].into_iter().enumerate() {
                let d = fy - slope * (fx - w as f32 * (0.1 + 0.18 * k as f32)) - h as f32 * 0.5;
                if d.abs() < 1.2 * (1.0 + slope * slope).sqrt() {
                    c = [0.95, 0.95, 0.9];
                }
            }
            if x == y {
                c = [1.0, 0.2, 0.2];
            }
            let a = if x < w / 8 && y < h / 6 { 0.0 } else { 1.0 };
            out.extend(c.map(|v| (v * a * 255.0).round() as u8));
            out.push((a * 255.0) as u8);
        }
    }
    out
}

struct Bench {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::RenderPipeline,
    info: wgpu::AdapterInfo,
}

/// Every adapter that gives a device (`cpu`: software adapters too).
fn benches(cpu: bool) -> Vec<Bench> {
    let instance =
        wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    let adapters = bevy::tasks::block_on(instance.enumerate_adapters(wgpu::Backends::all()));
    let mut out = Vec::new();
    for adapter in adapters {
        let info = adapter.get_info();
        if !cpu && info.device_type == wgpu::DeviceType::Cpu {
            continue;
        }
        let Ok((device, queue)) =
            bevy::tasks::block_on(adapter.request_device(&Default::default()))
        else {
            continue;
        };
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("present filter"),
            source: wgpu::ShaderSource::Wgsl(shader().into()),
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("present filter"),
            layout: None,
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        out.push(Bench {
            device,
            queue,
            pipeline,
            info,
        });
    }
    out
}

impl Bench {
    fn texture(&self, w: u32, h: u32, usage: wgpu::TextureUsages) -> wgpu::Texture {
        self.device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage,
            view_formats: &[],
        })
    }

    fn uniform(&self, v: [f32; 8]) -> wgpu::Buffer {
        let b = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: 32,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bytes: Vec<u8> = v.iter().flat_map(|f| f.to_le_bytes()).collect();
        self.queue.write_buffer(&b, 0, &bytes);
        b
    }

    /// `pixels` (w × h) shown at `s` times through `filter`: the target
    /// and the bind group that draws it.
    fn setup(
        &self,
        pixels: &[u8],
        (w, h): (u32, u32),
        s: f32,
        filter: ScalingFilter,
    ) -> (wgpu::Texture, wgpu::BindGroup) {
        let frame = self.texture(
            w,
            h,
            wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        );
        self.queue.write_texture(
            frame.as_image_copy(),
            pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(w * 4),
                rows_per_image: None,
            },
            frame.size(),
        );
        let (tw, th) = ((w as f32 * s) as u32, (h as f32 * s) as u32);
        let target = self.texture(
            tw,
            th,
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        );
        let sampler = self.device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let present = self.uniform([s, s, 0.0, 0.0, f32::from(filter.index()), 0.0, 0.0, 0.0]);
        let size = self.uniform([tw as f32, th as f32, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]);
        let view = frame.create_view(&Default::default());
        let entry = |binding, resource| wgpu::BindGroupEntry { binding, resource };
        let group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &self.pipeline.get_bind_group_layout(0),
            entries: &[
                entry(0, wgpu::BindingResource::TextureView(&view)),
                entry(1, wgpu::BindingResource::Sampler(&sampler)),
                entry(2, present.as_entire_binding()),
                entry(3, size.as_entire_binding()),
            ],
        });
        (target, group)
    }

    fn draw(&self, target: &wgpu::Texture, group: &wgpu::BindGroup) -> wgpu::CommandEncoder {
        let view = target.create_view(&Default::default());
        let mut encoder = self.device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, group, &[]);
            pass.draw(0..3, 0..1);
        }
        encoder
    }

    /// The drawn target's RGBA8 bytes (its rows must be a multiple of 256
    /// bytes).
    fn render(&self, pixels: &[u8], size: (u32, u32), s: f32, filter: ScalingFilter) -> Vec<u8> {
        let (target, group) = self.setup(pixels, size, s, filter);
        let (tw, th) = (target.width(), target.height());
        assert_eq!((tw * 4) % 256, 0);
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: u64::from(tw * th * 4),
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let mut encoder = self.draw(&target, &group);
        encoder.copy_texture_to_buffer(
            target.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(tw * 4),
                    rows_per_image: None,
                },
            },
            target.size(),
        );
        self.queue.submit([encoder.finish()]);
        buffer.map_async(wgpu::MapMode::Read, .., |r| r.expect("map"));
        self.device
            .poll(wgpu::PollType::wait_indefinitely())
            .expect("poll");
        let bytes = buffer.get_mapped_range(..).to_vec();
        buffer.unmap();
        bytes
    }
}

/// The CPU model of `filter` at `texel` (uv × size).
fn cpu_sample(t: &impl Texels, texel: Vec2, s: f32, filter: ScalingFilter) -> Vec4 {
    match filter {
        ScalingFilter::SharpBilinear => sharp_sample(t, texel, Vec2::splat(s)),
        ScalingFilter::Nearest => t.texel(texel.x.floor() as i64, texel.y.floor() as i64),
        ScalingFilter::CatmullRom | ScalingFilter::Lanczos => {
            let p = texel - 0.5;
            let base = p.floor();
            let wx = kernel_weights(filter, p.x - base.x).expect("kernel");
            let wy = kernel_weights(filter, p.y - base.y).expect("kernel");
            let mut sum = Vec4::ZERO;
            for (j, wy) in wy.iter().enumerate() {
                for (i, wx) in wx.iter().enumerate() {
                    let at = t.texel(base.x as i64 - 2 + i as i64, base.y as i64 - 2 + j as i64);
                    sum += *wx * *wy * at;
                }
            }
            let a = sum.w.clamp(0.0, 1.0);
            sum.truncate().clamp(Vec3::ZERO, Vec3::splat(a)).extend(a)
        }
        ScalingFilter::SmoothEdges => smooth_edges_sample(t, texel, Vec2::splat(s)).0,
    }
}

/// Local, needs a GPU: every filter on every adapter (software ones too)
/// draws what its CPU model computes, to rounding (at most 2 of 255; 3 on
/// under 0.1% of channels for Smooth Edges),
/// at 3x and at 1.5x.
/// `cargo test -p d2-client --lib world_view::sharp::gpu_tests -- --ignored --nocapture`
#[test]
#[ignore = "needs a GPU"]
fn gpu_present_filters_match_the_cpu_models() {
    let mut ran = 0;
    for bench in benches(true) {
        for ((w, h), s) in [((320u32, 180u32), 3.0f32), ((384, 216), 1.5)] {
            let pixels = art(w, h);
            let t = |x: i64, y: i64| {
                let (x, y) = (
                    x.clamp(0, w as i64 - 1) as u32,
                    y.clamp(0, h as i64 - 1) as u32,
                );
                let i = ((y * w + x) * 4) as usize;
                Vec4::from_array(std::array::from_fn(|k| f32::from(pixels[i + k]) / 255.0))
            };
            for filter in ScalingFilter::ALL {
                let got = bench.render(&pixels, (w, h), s, filter);
                let (tw, th) = ((w as f32 * s) as u32, (h as f32 * s) as u32);
                let (mut worst, mut off) = (0u8, 0usize);
                for y in 0..th {
                    for x in 0..tw {
                        let texel = (Vec2::new(x as f32, y as f32) + 0.5) / s;
                        // Nearest at 1.5x: every other screen pixel's
                        // centre lies exactly on a game pixel edge, where
                        // the GPU's rounding may take either side.
                        let tie = (texel - texel.round()).abs().min_element() < 1e-4;
                        if filter == ScalingFilter::Nearest && tie {
                            continue;
                        }
                        let want = cpu_sample(&t, texel, s, filter).to_array();
                        let i = ((y * tw + x) * 4) as usize;
                        for k in 0..4 {
                            let want = (want[k].clamp(0.0, 1.0) * 255.0).round() as u8;
                            let d = got[i + k].abs_diff(want);
                            worst = worst.max(d);
                            off += usize::from(d > 1);
                        }
                    }
                }
                println!(
                    "{} ({:?}): {filter:?} at {s}x: worst {worst}/255, {off} channels off by more than 1",
                    bench.info.name, bench.info.backend
                );
                // Where Smooth Edges smooths at 3x its across-edge Gaussian
                // is steep, so a GPU's own rounding of the edge direction
                // shows: up to 3 of 255 on a few channels (RX 9070 XT).
                let (most, share) = match filter {
                    ScalingFilter::SmoothEdges => (3, 0.001),
                    _ => (2, 0.0),
                };
                let channels = (tw * th * 4) as f64;
                assert!(
                    worst <= most && off as f64 <= share * channels,
                    "{} {filter:?} at {s}x: worst {worst}, {off} off",
                    bench.info.name
                );
            }
        }
        ran += 1;
    }
    assert!(ran > 0, "no adapter");
}

/// Local, needs a GPU: the time to draw one 2160p frame (1280 × 720 at 3x)
/// through each filter, per hardware adapter, and Smooth Edges' cost over
/// Sharp Bilinear.
/// `cargo test -p d2-client --lib world_view::sharp::gpu_tests::gpu_present_filter_cost -- --ignored --nocapture`
#[test]
#[ignore = "needs a GPU"]
fn gpu_present_filter_cost() {
    let (w, h) = (1280u32, 720u32);
    let pixels = art(w, h);
    for bench in benches(false) {
        let mut sharp_ms = 0.0;
        for filter in ScalingFilter::ALL {
            let (target, group) = bench.setup(&pixels, (w, h), 3.0, filter);
            let runs = 300;
            let mut times = Vec::with_capacity(runs);
            for n in 0..runs + 20 {
                let start = std::time::Instant::now();
                bench.queue.submit([bench.draw(&target, &group).finish()]);
                bench
                    .device
                    .poll(wgpu::PollType::wait_indefinitely())
                    .expect("poll");
                if n >= 20 {
                    times.push(start.elapsed().as_secs_f64() * 1000.0);
                }
            }
            times.sort_by(f64::total_cmp);
            let median = times[runs / 2];
            if filter == ScalingFilter::SharpBilinear {
                sharp_ms = median;
            }
            println!(
                "{} ({:?}, {:?}): {filter:?} 2160p median {median:.3} ms, min {:.3} ms, p95 {:.3} ms, over Sharp Bilinear {:+.3} ms",
                bench.info.name,
                bench.info.backend,
                bench.info.device_type,
                times[0],
                times[runs * 95 / 100],
                median - sharp_ms
            );
        }
    }
}
