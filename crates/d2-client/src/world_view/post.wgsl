// The game view's post-processing pass (`post.rs`), on the scaled picture
// at the output resolution. Bloom first takes the bright parts of the
// picture at an eighth of its size and blurs them (`bloom_down`,
// `bloom_blur_h`, `bloom_blur_v`); pass A adds that glow, grades the
// picture with the look's 3D colour table, changes the colour and
// sharpens (as a compute pass in tiles, `colour_sharpen_tiles`, where the
// sharpen's reach is whole pixels); pass B smooths edges (FXAA) and then
// lays the native UI on top, so text and panels are never touched by an
// effect. Colour work
// happens on sRGB encoded values (as the eye sees them); light (bloom) is
// added in linear values, which the textures read and write.

#import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput

struct Post {
    // x: brightness offset (-0.2..0.2), y: saturation (0..2),
    // z: vibrance (0..1), w: sharpen strength (0..2).
    colour: vec4<f32>,
    // x: FXAA search steps (0: off), y: edge threshold (share of the
    // local brightest), z: darkest edge it still smooths, w: sub-pixel
    // smoothing amount.
    fxaa: vec4<f32>,
    // The native UI images' place in the output: x left, y top, z width,
    // w height, in pixels; z = 0: no UI to lay on.
    rect: vec4<f32>,
    // x: gamma exponent minus one, y: contrast minus one (zero: identity),
    // applied after the UI as the presenting shaders do.
    picture: vec4<f32>,
    // The scaled picture's own pixels in the output: x left, y top,
    // z width, w height; z = 0: the whole output. Pixels outside (the
    // black bars) pass through unchanged, and reads stay inside.
    image: vec4<f32>,
    // x: bloom strength (0: no bloom), y: the brightness bloom starts at,
    // z: 1 to grade with the look's table, w: unused.
    grade: vec4<f32>,
    // x, y: one view pixel's size in output pixels (3 at 2160p, 1.5 at
    // 1080p), the sharpen's reach; z, w: unused.
    view: vec4<f32>,
}

@group(0) @binding(0) var source: texture_2d<f32>;
@group(0) @binding(1) var source_sampler: sampler;
@group(0) @binding(2) var<uniform> post: Post;
@group(0) @binding(3) var cover: texture_2d<f32>;
@group(0) @binding(4) var add: texture_2d<f32>;
// Pass A: the blurred bright parts (eighth size) and the look's table.
@group(0) @binding(5) var bloom: texture_2d<f32>;
@group(0) @binding(6) var look: texture_3d<f32>;
// Pass A in tiles (`colour_sharpen_tiles`): its picture, encoded sRGB
// bytes (pass B reads them through an sRGB view).
@group(0) @binding(7) var tiles_out: texture_storage_2d<rgba8unorm, write>;
// The render target's own sRGB encoding, measured once (`srgb_probe`,
// `srgb_edges`): 0..255, the linear value the measurement for byte edge k
// is centred on (the exact edge, from `post.rs`); 256..511, the least
// linear value the target stores as byte k + 1.
@group(0) @binding(8) var<storage, read> srgb: array<f32, 512>;
// The measurement: the probe's bytes as written, and the table to fill.
@group(0) @binding(9) var probe: texture_2d<f32>;
@group(0) @binding(10) var<storage, read_write> srgb_out: array<f32, 512>;

fn to_gamma(c: vec3<f32>) -> vec3<f32> {
    let low = c * 12.92;
    let high = 1.055 * pow(max(c, vec3<f32>(0.0)), vec3<f32>(1.0 / 2.4)) - 0.055;
    return select(high, low, c <= vec3<f32>(0.0031308));
}

fn to_linear(c: vec3<f32>) -> vec3<f32> {
    let low = c / 12.92;
    let high = pow((max(c, vec3<f32>(0.0)) + 0.055) / 1.055, vec3<f32>(2.4));
    return select(high, low, c <= vec3<f32>(0.04045));
}

// Rec. 709 luma of an encoded colour.
fn luma(g: vec3<f32>) -> f32 {
    return dot(g, vec3<f32>(0.2126, 0.7152, 0.0722));
}

// Brightness, saturation and vibrance of an encoded colour (`post.rs`
// `colour` is the same maths). Vibrance raises the saturation of dull
// colours more than of colours that are already strong.
fn colour(g: vec3<f32>) -> vec3<f32> {
    let b = g + vec3<f32>(post.colour.x);
    let y = luma(b);
    let sat = max(b.r, max(b.g, b.b)) - min(b.r, min(b.g, b.b));
    let k = post.colour.y * (1.0 + post.colour.z * (1.0 - clamp(sat, 0.0, 1.0)));
    return clamp(vec3<f32>(y) + (b - vec3<f32>(y)) * k, vec3<f32>(0.0), vec3<f32>(1.0));
}

// The picture's first and last pixel (`Post.image`).
fn image_lo() -> vec2<i32> {
    return select(vec2<i32>(0), vec2<i32>(post.image.xy), post.image.z > 0.0);
}

fn image_hi() -> vec2<i32> {
    let last = vec2<i32>(textureDimensions(source)) - 1;
    return select(last, vec2<i32>(post.image.xy + post.image.zw) - 1, post.image.z > 0.0);
}

// Whether output pixel `p` is on the picture (not a bar).
fn on_image(p: vec2<i32>) -> bool {
    return all(p >= image_lo()) && all(p <= image_hi());
}

// The picture pixel at `p`, the nearest picture pixel for one off it.
fn texel(p: vec2<i32>) -> vec3<f32> {
    return textureLoad(source, clamp(p, image_lo(), image_hi()), 0).rgb;
}

// `uv` kept on the picture's pixel centres, so filtered reads never take
// in a bar.
fn on_picture(uv: vec2<f32>) -> vec2<f32> {
    let size = vec2<f32>(textureDimensions(source));
    let lo = (vec2<f32>(image_lo()) + 0.5) / size;
    let hi = (vec2<f32>(image_hi()) + 0.5) / size;
    return clamp(uv, lo, hi);
}

// Bloom, step 1: the bright parts of the picture at an eighth of its size.
// Each output texel averages its 8 x 8 picture pixels (sixteen bilinear
// reads, each the mean of a 2 x 2) and keeps only the light above the threshold, with a soft knee
// so the glow fades in.
@fragment
fn bloom_down(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let px = 1.0 / vec2<f32>(textureDimensions(source));
    var c = vec3<f32>(0.0);
    for (var j = 0; j < 4; j++) {
        for (var i = 0; i < 4; i++) {
            let o = (vec2<f32>(f32(i), f32(j)) * 2.0 - 3.0) * px;
            c += textureSampleLevel(source, source_sampler, in.uv + o, 0.0).rgb;
        }
    }
    c *= 0.0625;
    let l = max(c.r, max(c.g, c.b));
    let t = post.grade.y;
    let knee = clamp((l - t) / max(1.0 - t, 1e-4), 0.0, 1.0);
    return vec4<f32>(c * knee * knee, 1.0);
}

// Bloom, steps 2 and 3: a 9-tap Gaussian (sigma 2 texels) across, then
// down, at the eighth size.
fn gauss(uv: vec2<f32>, dir: vec2<f32>) -> vec4<f32> {
    let px = dir / vec2<f32>(textureDimensions(source));
    var w = array<f32, 5>(0.2042, 0.1802, 0.1238, 0.0663, 0.0276);
    var c = textureSampleLevel(source, source_sampler, uv, 0.0).rgb * w[0];
    for (var i = 1; i < 5; i++) {
        let o = px * f32(i);
        c += (textureSampleLevel(source, source_sampler, uv + o, 0.0).rgb
            + textureSampleLevel(source, source_sampler, uv - o, 0.0).rgb) * w[i];
    }
    return vec4<f32>(c, 1.0);
}

@fragment
fn bloom_blur_h(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    return gauss(in.uv, vec2<f32>(1.0, 0.0));
}

@fragment
fn bloom_blur_v(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    return gauss(in.uv, vec2<f32>(0.0, 1.0));
}

// One picture pixel through pass A's colour steps: the glow added (linear
// light), the look's grade, then brightness, saturation and vibrance;
// encoded.
fn graded(p: vec2<i32>) -> vec3<f32> {
    let uv = (vec2<f32>(p) + 0.5) / vec2<f32>(textureDimensions(source));
    return grade(texel(p), uv);
}

// The picture at output position `at` (pixel units; a pixel's centre is
// at its index + 0.5), read filtered and kept on the picture, through the
// colour steps.
fn graded_at(at: vec2<f32>) -> vec3<f32> {
    let uv = on_picture(at / vec2<f32>(textureDimensions(source)));
    return grade(textureSampleLevel(source, source_sampler, uv, 0.0).rgb, uv);
}

// Pass A's colour steps on linear colour `linear` read at `uv`.
fn grade(linear: vec3<f32>, uv: vec2<f32>) -> vec3<f32> {
    var c = linear;
    if post.grade.x > 0.0 {
        c += textureSampleLevel(bloom, source_sampler, uv, 0.0).rgb * post.grade.x;
    }
    // Light past white (from the glow) reads the table's edge (clamped
    // sampler); colour() clamps the result.
    var g = to_gamma(c);
    if post.grade.z > 0.0 {
        let n = f32(textureDimensions(look).x);
        g = textureSampleLevel(look, source_sampler, g * ((n - 1.0) / n) + 0.5 / n, 0.0).rgb;
    }
    return colour(g);
}

// Pass A: the colour steps ([`graded`]), then a sharpen that adds back the
// difference from the four neighbours' average and is limited to the
// range of the 3 x 3 around it, so it can't draw halos past an edge. The
// neighbours are one view pixel away (`Post.view`), not one output pixel:
// the enlarged picture is flat across each view pixel, so output
// neighbours would hold the same colour and the sharpen would do nothing.
@fragment
fn colour_sharpen(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let p = vec2<i32>(floor(in.position.xy));
    if !on_image(p) {
        return textureLoad(source, p, 0);
    }
    let m = graded(p);
    if post.colour.w <= 0.0 {
        return vec4<f32>(to_linear(m), 1.0);
    }
    let at = vec2<f32>(p) + 0.5;
    let d = max(post.view.xy, vec2<f32>(1.0));
    let n = graded_at(at + vec2<f32>(0.0, -d.y));
    let s = graded_at(at + vec2<f32>(0.0, d.y));
    let w = graded_at(at + vec2<f32>(-d.x, 0.0));
    let e = graded_at(at + vec2<f32>(d.x, 0.0));
    let nw = graded_at(at - d);
    let se = graded_at(at + d);
    let ne = graded_at(at + vec2<f32>(d.x, -d.y));
    let sw = graded_at(at + vec2<f32>(-d.x, d.y));
    let blur = (n + s + w + e) * 0.25;
    let lo = min(min(m, min(min(n, s), min(w, e))), min(min(nw, se), min(ne, sw)));
    let hi = max(max(m, max(max(n, s), max(w, e))), max(max(nw, se), max(ne, sw)));
    let sharp = clamp(m + (m - blur) * post.colour.w, lo, hi);
    return vec4<f32>(to_linear(sharp), 1.0);
}

// Pass A in 16 x 16 tiles, where the sharpen's reach is whole output
// pixels (`Post.view` 1 to TILE_REACH; `post.rs` decides): the eight neighbours
// then sit on other pixels' centres, so each pixel of the tile and its
// reach around is graded once, into workgroup memory, instead of nine
// times. The maths is `colour_sharpen`'s, step for step: the same picture
// to the bit (a filtered read at a pixel's centre is that pixel, and the
// result is encoded to sRGB bytes as the render target does, measured).
const TILE: i32 = 16;
// The longest reach in tiles; `post.rs` `TILE_REACH`.
const TILE_REACH: i32 = 4;
const TILE_SPAN: i32 = 24;
var<workgroup> tile: array<vec3<f32>, 576>;

var<workgroup> edges: array<f32, 255>;

// The byte the render target stores linear value `v` as, from the
// measured edges (the count of edges at or below `v`), as 0..1. `g` is
// `v` encoded (near enough): its byte is the first guess, at most a step
// or two off, and the edges either side settle it.
fn srgb_byte(v: f32, g: f32) -> f32 {
    var b = i32(clamp(round(g * 255.0), 0.0, 255.0));
    while b > 0 && edges[b - 1] > v {
        b -= 1;
    }
    while b < 255 && edges[b] <= v {
        b += 1;
    }
    return f32(b) / 255.0;
}

// Linear colour `c` (encoded: `g`) as the render target stores it: its
// sRGB bytes, in the storage texture's unorm units.
fn srgb_bytes(c: vec3<f32>, g: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(srgb_byte(c.r, g.r), srgb_byte(c.g, g.g), srgb_byte(c.b, g.b));
}

// Probe value `i` (0..PROBE_WIDTH) around a byte edge `centre`: `step`
// float steps apart, centred on it. The measurement runs twice: coarse,
// COARSE_STEP apart around the exact edge (the target's encoding can sit
// a few per cent off it in the darks, hundreds of thousands of steps),
// then one step apart around the coarse result.
const PROBE_WIDTH: i32 = 4096;
const COARSE_STEP: i32 = 2048;

fn probe_value(centre: f32, i: i32, step: i32) -> f32 {
    return bitcast<f32>(u32(i32(bitcast<u32>(centre)) + (i - PROBE_WIDTH / 2) * step));
}

// Measuring the encoding, step 1: drawn into an sRGB target of
// PROBE_WIDTH x 255, row k holds the probe values around edge k.
@fragment
fn srgb_probe_coarse(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let p = vec2<i32>(floor(in.position.xy));
    return vec4<f32>(vec3<f32>(probe_value(srgb[p.y], p.x, COARSE_STEP)), 1.0);
}

@fragment
fn srgb_probe_fine(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let p = vec2<i32>(floor(in.position.xy));
    return vec4<f32>(vec3<f32>(probe_value(srgb[256 + p.y], p.x, 1)), 1.0);
}

// Step 2: per edge, the first probe value stored as byte k + 1 (the
// stored bytes rise with the value), into `srgb_out[256 + k]`.
@compute @workgroup_size(64, 1, 1)
fn srgb_edges_coarse(@builtin(global_invocation_id) id: vec3<u32>) {
    let k = i32(id.x);
    if k < 255 {
        srgb_out[256 + k] = probe_edge(k, srgb_out[k], COARSE_STEP);
    }
}

@compute @workgroup_size(64, 1, 1)
fn srgb_edges_fine(@builtin(global_invocation_id) id: vec3<u32>) {
    let k = i32(id.x);
    if k < 255 {
        srgb_out[256 + k] = probe_edge(k, srgb_out[256 + k], 1);
    }
}

// The first probe value around `centre` stored as byte k + 1; `centre`
// itself if the window missed the edge.
fn probe_edge(k: i32, centre: f32, step: i32) -> f32 {
    var lo = 0;
    var hi = PROBE_WIDTH;
    while lo < hi {
        let mid = (lo + hi) / 2;
        if probe_byte(mid, k) >= u32(k + 1) {
            hi = mid;
        } else {
            lo = mid + 1;
        }
    }
    if lo > 0 && lo < PROBE_WIDTH {
        return probe_value(centre, lo, step);
    }
    return centre;
}

fn probe_byte(i: i32, k: i32) -> u32 {
    return u32(round(textureLoad(probe, vec2<i32>(i, k), 0).r * 255.0));
}

@compute @workgroup_size(16, 16, 1)
fn colour_sharpen_tiles(
    @builtin(workgroup_id) group: vec3<u32>,
    @builtin(local_invocation_id) local: vec3<u32>,
    @builtin(local_invocation_index) index: u32,
) {
    let d = vec2<i32>(max(post.view.xy, vec2<f32>(1.0)));
    let corner = vec2<i32>(group.xy) * TILE;
    // The tile and its reach, from `corner - d`, row stride TILE_SPAN.
    let span = vec2<i32>(TILE) + 2 * d;
    if index < 255u {
        edges[index] = srgb[256u + index];
    }
    for (var i = i32(index); i < span.x * span.y; i += TILE * TILE) {
        let at = vec2<i32>(i % span.x, i / span.x);
        let q = clamp(corner - d + at, image_lo(), image_hi());
        tile[at.y * TILE_SPAN + at.x] = graded(q);
    }
    workgroupBarrier();
    let p = corner + vec2<i32>(local.xy);
    if any(p >= vec2<i32>(textureDimensions(source))) {
        return;
    }
    if !on_image(p) {
        let c = textureLoad(source, p, 0).rgb;
        textureStore(tiles_out, p, vec4<f32>(srgb_bytes(c, to_gamma(c)), 1.0));
        return;
    }
    let c = vec2<i32>(local.xy) + d;
    let m = tiled(c);
    if post.colour.w <= 0.0 {
        textureStore(tiles_out, p, vec4<f32>(srgb_bytes(to_linear(m), m), 1.0));
        return;
    }
    let n = tiled(c + vec2<i32>(0, -d.y));
    let s = tiled(c + vec2<i32>(0, d.y));
    let w = tiled(c + vec2<i32>(-d.x, 0));
    let e = tiled(c + vec2<i32>(d.x, 0));
    let nw = tiled(c - d);
    let se = tiled(c + d);
    let ne = tiled(c + vec2<i32>(d.x, -d.y));
    let sw = tiled(c + vec2<i32>(-d.x, d.y));
    let blur = (n + s + w + e) * 0.25;
    let lo = min(min(m, min(min(n, s), min(w, e))), min(min(nw, se), min(ne, sw)));
    let hi = max(max(m, max(max(n, s), max(w, e))), max(max(nw, se), max(ne, sw)));
    let sharp = clamp(m + (m - blur) * post.colour.w, lo, hi);
    textureStore(tiles_out, p, vec4<f32>(srgb_bytes(to_linear(sharp), sharp), 1.0));
}

// The graded pixel at `at` in the workgroup's tile.
fn tiled(at: vec2<i32>) -> vec3<f32> {
    return tile[at.y * TILE_SPAN + at.x];
}

fn luma_at(uv: vec2<f32>) -> f32 {
    return luma(to_gamma(textureSampleLevel(source, source_sampler, on_picture(uv), 0.0).rgb));
}

// The distance (in pixels) the edge search moves at step `i`: one pixel at
// first, then further as the search goes on.
fn step_size(i: i32) -> f32 {
    if i < 5 {
        return 1.0;
    }
    if i == 5 {
        return 1.5;
    }
    if i < 10 {
        return 2.0;
    }
    if i == 10 {
        return 4.0;
    }
    return 8.0;
}

// FXAA: find a pixel on a contrast edge, tell whether the edge runs
// across or down, follow it both ways to its ends, and move the sample
// toward the neighbour across the edge by how close the pixel is to the
// nearer end (long, shallow steps blend most). Faint local contrast adds
// a little sub-pixel blending.
fn fxaa(uv: vec2<f32>) -> vec3<f32> {
    let px = 1.0 / vec2<f32>(textureDimensions(source));
    let centre = textureSampleLevel(source, source_sampler, uv, 0.0).rgb;
    let m = luma(to_gamma(centre));
    let n = luma_at(uv + vec2<f32>(0.0, -px.y));
    let s = luma_at(uv + vec2<f32>(0.0, px.y));
    let w = luma_at(uv + vec2<f32>(-px.x, 0.0));
    let e = luma_at(uv + vec2<f32>(px.x, 0.0));
    let lo = min(m, min(min(n, s), min(w, e)));
    let hi = max(m, max(max(n, s), max(w, e)));
    let range = hi - lo;
    if range < max(post.fxaa.z, hi * post.fxaa.y) {
        return centre;
    }
    let nw = luma_at(uv + vec2<f32>(-px.x, -px.y));
    let ne = luma_at(uv + vec2<f32>(px.x, -px.y));
    let sw = luma_at(uv + vec2<f32>(-px.x, px.y));
    let se = luma_at(uv + vec2<f32>(px.x, px.y));
    // Sub-pixel: how far the pixel is from its neighbourhood's average.
    let avg = (2.0 * (n + s + w + e) + nw + ne + sw + se) / 12.0;
    let sub_raw = clamp(abs(avg - m) / range, 0.0, 1.0);
    let sub_s = smoothstep(0.0, 1.0, sub_raw);
    let sub = sub_s * sub_s * post.fxaa.w;
    // Second differences across each direction: the larger one says the
    // edge runs along the other.
    let across_rows = abs(nw + sw - 2.0 * w) + 2.0 * abs(n + s - 2.0 * m) + abs(ne + se - 2.0 * e);
    let across_cols = abs(nw + ne - 2.0 * n) + 2.0 * abs(w + e - 2.0 * m) + abs(sw + se - 2.0 * s);
    let horizontal = across_rows >= across_cols;
    // The neighbour across the edge with the stronger change.
    let l1 = select(w, n, horizontal);
    let l2 = select(e, s, horizontal);
    let g1 = abs(l1 - m);
    let g2 = abs(l2 - m);
    let first = g1 >= g2;
    let grad = max(g1, g2) * 0.25;
    let edge_luma = select(0.5 * (l2 + m), 0.5 * (l1 + m), first);
    let normal = select(vec2<f32>(px.x, 0.0), vec2<f32>(0.0, px.y), horizontal)
        * select(1.0, -1.0, first);
    let along = select(vec2<f32>(0.0, px.y), vec2<f32>(px.x, 0.0), horizontal);
    // Walk along the edge, half a pixel toward the chosen neighbour.
    let start = uv + normal * 0.5;
    var a = start - along;
    var b = start + along;
    var ea = luma_at(a) - edge_luma;
    var eb = luma_at(b) - edge_luma;
    var done_a = abs(ea) >= grad;
    var done_b = abs(eb) >= grad;
    let steps = i32(post.fxaa.x);
    for (var i = 1; i < steps; i++) {
        if done_a && done_b {
            break;
        }
        if !done_a {
            a -= along * step_size(i);
            ea = luma_at(a) - edge_luma;
            done_a = abs(ea) >= grad;
        }
        if !done_b {
            b += along * step_size(i);
            eb = luma_at(b) - edge_luma;
            done_b = abs(eb) >= grad;
        }
    }
    let da = select(uv.y - a.y, uv.x - a.x, horizontal);
    let db = select(b.y - uv.y, b.x - uv.x, horizontal);
    let near_a = da < db;
    let nearest = min(da, db);
    let span = da + db;
    // Blend only when the nearer end bends the way the centre does.
    let centre_darker = m < edge_luma;
    let end = select(eb, ea, near_a);
    let bends = (end < 0.0) != centre_darker;
    let edge_offset = select(0.0, 0.5 - nearest / max(span, 1e-6), bends);
    let offset = max(edge_offset, sub);
    let at = uv + normal * offset;
    return textureSampleLevel(source, source_sampler, on_picture(at), 0.0).rgb;
}

// Defaults are exactly identity: the presenting shaders' picture options.
fn picture_options(rgb: vec3<f32>) -> vec3<f32> {
    return clamp(pow(max(rgb, vec3<f32>(0.0)), vec3<f32>(post.picture.x + 1.0)) * (post.picture.y + 1.0), vec3<f32>(0.0), vec3<f32>(1.0));
}

// Pass B: FXAA (or the picture as it is), then the native UI on top in the
// palette's own units, `out = picture * (1 - cover) + add`, and the
// picture options, exactly as `native_ui.wgsl` lays them when there is no
// post pass.
@fragment
fn fxaa_ui(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let p = vec2<i32>(floor(in.position.xy));
    if !on_image(p) {
        return textureLoad(source, p, 0);
    }
    var c: vec3<f32>;
    if post.fxaa.x > 0.0 {
        c = fxaa(in.uv);
    } else {
        c = texel(p);
    }
    if post.rect.z <= 0.0 {
        return vec4<f32>(c, 1.0);
    }
    let at = p - vec2<i32>(post.rect.xy);
    let size = vec2<i32>(post.rect.zw);
    if all(at >= vec2<i32>(0)) && all(at < size) {
        let k = textureLoad(cover, at, 0).rgb;
        let a = textureLoad(add, at, 0).rgb;
        if any(k != vec3<f32>(0.0)) || any(a != vec3<f32>(0.0)) {
            let out = to_gamma(c) * (1.0 - k) + a;
            c = to_linear(clamp(out, vec3<f32>(0.0), vec3<f32>(1.0)));
        }
    }
    return vec4<f32>(picture_options(c), 1.0);
}
