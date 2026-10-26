// The presented game frame, scaled with the Scaling Filter in force
// (`sharp.rs`). The default, Sharp Bilinear, keeps every game pixel a
// solid block and blends only the screen pixel where two game pixels
// meet: even pixel widths at a fractional scale without the blur of plain
// linear filtering, and exactly the nearest game pixel at a whole-number
// scale (one texture read with a linear sampler). Nearest reads the game
// pixel under the screen pixel; Catmull-Rom and Lanczos weigh the 4 x 4
// and 6 x 6 game pixels around the sample; Smooth Edges reads diagonal
// edges along their direction and is Sharp Bilinear everywhere else.
// `sharp.rs` holds the same rules in Rust (`sharp_offset`, `filter_row`,
// `smooth_edges_sample`) with their tests.

#import bevy_sprite::mesh2d_vertex_output::VertexOutput

// xy: screen pixels per game pixel on each axis (at least 1); zw: gamma
// exponent and contrast minus one. kind.x: the scaling filter
// (0 Sharp Bilinear, 1 Nearest, 2 Catmull-Rom, 3 Lanczos, 4 Smooth Edges).
struct Present {
    scale: vec4<f32>,
    kind: vec4<f32>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var frame: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var frame_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var<uniform> present: Present;
// The part of the image shown: xy its uv origin, zw its uv size.
@group(#{MATERIAL_BIND_GROUP}) @binding(3) var<uniform> uv_rect: vec4<f32>;
// The tick before's image and its uv origin (xy) and size (zw; zero: no
// fill): read where `uv` is outside the image (the smooth camera's world
// layer, `smooth.rs`).
@group(#{MATERIAL_BIND_GROUP}) @binding(4) var prev: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(5) var<uniform> prev_uv: vec4<f32>;

// `SOLID_SLACK` in sharp.rs.
const SOLID_SLACK: f32 = 0.000244140625;

// ---- present filter: the same text in sharp.wgsl and native_ui.wgsl (a sharp.rs test checks).

// The Catmull-Rom kernel (Keys' cubic, a = -0.5).
fn catmull_rom(v: f32) -> f32 {
    let x = abs(v);
    if x < 1.0 {
        return (1.5 * x - 2.5) * x * x + 1.0;
    }
    if x < 2.0 {
        return ((-0.5 * x + 2.5) * x - 4.0) * x + 2.0;
    }
    return 0.0;
}

// The three-lobe Lanczos kernel, sinc(x) sinc(x / 3) for |x| < 3.
fn lanczos3(v: f32) -> f32 {
    let x = abs(v);
    if x < 0.00001 {
        return 1.0;
    }
    if x >= 3.0 {
        return 0.0;
    }
    let px = 3.14159265358979 * x;
    return 3.0 * sin(px) * sin(px / 3.0) / (px * px);
}

// The weights of the taps at -2 to 3 from the texel before the sample, `t`
// past its centre, normalised (Catmull-Rom's outer two are 0).
fn kernel_weights(t: f32, mode: u32) -> array<f32, 6> {
    var w: array<f32, 6>;
    var sum = 0.0;
    for (var i = 0; i < 6; i++) {
        let x = f32(i) - 2.0 - t;
        if mode == 2u {
            w[i] = catmull_rom(x);
        } else {
            w[i] = lanczos3(x);
        }
        sum += w[i];
    }
    for (var i = 0; i < 6; i++) {
        w[i] = w[i] / sum;
    }
    return w;
}

// The kernel sample at texel coordinates `p` (texel centres at whole
// numbers), edges clamped, kept a valid premultiplied colour. Catmull-Rom
// reads the middle 4 x 4 texels, Lanczos all 6 x 6.
fn kernel_sample(t: texture_2d<f32>, p: vec2<f32>, mode: u32) -> vec4<f32> {
    let base = floor(p);
    var wx = kernel_weights(p.x - base.x, mode);
    var wy = kernel_weights(p.y - base.y, mode);
    let last = vec2<i32>(textureDimensions(t)) - 1;
    let b = vec2<i32>(base) - 2;
    let lo = select(0, 1, mode == 2u);
    let hi = 6 - lo;
    var sum = vec4<f32>(0.0);
    for (var j = lo; j < hi; j++) {
        var row = vec4<f32>(0.0);
        for (var i = lo; i < hi; i++) {
            let at = clamp(b + vec2<i32>(i, j), vec2<i32>(0), last);
            row += wx[i] * textureLoad(t, at, 0);
        }
        sum += wy[j] * row;
    }
    let a = clamp(sum.a, 0.0, 1.0);
    return vec4<f32>(clamp(sum.rgb, vec3<f32>(0.0), vec3<f32>(a)), a);
}

// `t` at `uv` through the scaling filter, premultiplied (a transparent
// texel is all zero).
fn present_sample(t: texture_2d<f32>, uv: vec2<f32>) -> vec4<f32> {
    let size = vec2<f32>(textureDimensions(t));
    let texel = uv * size;
    let mode = u32(present.kind.x + 0.5);
    if mode == 1u {
        let last = vec2<i32>(size) - 1;
        return textureLoad(t, clamp(vec2<i32>(floor(texel)), vec2<i32>(0), last), 0);
    }
    if mode == 2u || mode == 3u {
        return kernel_sample(t, texel - 0.5, mode);
    }
    // Sharp Bilinear: distance from the game pixel's centre; inside the
    // solid region the sample snaps to the centre, within half a screen
    // pixel of an edge it ramps to the boundary with the neighbour.
    let base = floor(texel);
    let d = fract(texel) - 0.5;
    let prescale = ceil(present.scale.xy);
    let solid = 0.5 - 0.5 / prescale + SOLID_SLACK;
    let f = (d - clamp(d, -solid, solid)) * prescale + 0.5;
    let sharp = textureSampleLevel(t, frame_sampler, (base + f) / size, 0.0);
    if mode == 4u {
        return smooth_edges(t, texel - 0.5, sharp);
    }
    return sharp;
}

// Smooth Edges' tuning (`SmoothEdges` in sharp.rs): the coherence,
// diagonal and edge strength ramps, and the kernel's spread across the
// edge (texels squared at scale 1, shrinking with the scale) and along it
// (texels squared).
const SE_COHERENCE_LO: f32 = 0.45;
const SE_COHERENCE_HI: f32 = 0.75;
const SE_DIAGONAL_LO: f32 = 0.2;
const SE_DIAGONAL_HI: f32 = 0.7;
const SE_EDGE_LO: f32 = 0.02;
const SE_EDGE_HI: f32 = 0.06;
const SE_ACROSS: f32 = 0.25;
const SE_ALONG: f32 = 1.0;

// What the edge finder sees of a premultiplied texel: its brightness,
// plus its coverage so an outline against a transparent texel counts.
fn edge_value(c: vec4<f32>) -> f32 {
    return dot(c, vec4<f32>(0.299, 0.587, 0.114, 0.5));
}

// Smooth Edges at texel coordinates `p` (texel centres at whole numbers):
// `sharp` (the Sharp Bilinear sample) where the art has no clear diagonal
// edge, and a reading along the edge where it has one. The local edge is
// the structure tensor of the 5 x 5 two-by-two cells of the 6 x 6 texels
// around the sample, each cell's gradient weighted by a tent of radius 2.5
// so the result is continuous from one game pixel to the next. A clear
// edge (one dominant direction: coherence) that runs diagonally (the
// tensor's off-diagonal part) is read with a Gaussian stretched along it
// and narrow across it, over the middle 4 x 4 texels; horizontal and
// vertical edges, corners, flat areas and noise stay Sharp Bilinear.
fn smooth_edges(t: texture_2d<f32>, p: vec2<f32>, sharp: vec4<f32>) -> vec4<f32> {
    let base = floor(p);
    let f = p - base;
    let last = vec2<i32>(textureDimensions(t)) - 1;
    let b = vec2<i32>(base) - 2;
    var c: array<vec4<f32>, 36>;
    var v: array<f32, 36>;
    for (var j = 0; j < 6; j++) {
        for (var i = 0; i < 6; i++) {
            let k = j * 6 + i;
            c[k] = textureLoad(t, clamp(b + vec2<i32>(i, j), vec2<i32>(0), last), 0);
            v[k] = edge_value(c[k]);
        }
    }
    var jxx = 0.0;
    var jxy = 0.0;
    var jyy = 0.0;
    var wsum = 0.0;
    for (var j = 0; j < 5; j++) {
        let wy = 2.5 - abs(f32(j) - 1.5 - f.y);
        for (var i = 0; i < 5; i++) {
            let wx = 2.5 - abs(f32(i) - 1.5 - f.x);
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
    let delta = sqrt(dd * dd + 4.0 * jxy * jxy);
    let coherence = delta / max(tr, 1e-8);
    let diagonal = abs(2.0 * jxy) / max(delta, 1e-8);
    let edge = sqrt(tr / wsum);
    let amount = smoothstep(SE_COHERENCE_LO, SE_COHERENCE_HI, coherence)
        * smoothstep(SE_DIAGONAL_LO, SE_DIAGONAL_HI, diagonal)
        * smoothstep(SE_EDGE_LO, SE_EDGE_HI, edge);
    if amount <= 0.0 {
        return sharp;
    }
    // The edge normal from the tensor's double angle; the tangent runs
    // along the edge.
    let cos2 = dd / max(delta, 1e-8);
    let n = vec2<f32>(sqrt(max(0.5 + 0.5 * cos2, 0.0)), sign(jxy) * sqrt(max(0.5 - 0.5 * cos2, 0.0)));
    let tangent = vec2<f32>(-n.y, n.x);
    let across = max(min(present.scale.x, present.scale.y), 1.0) / SE_ACROSS;
    let along = 1.0 / SE_ALONG;
    var sum = vec4<f32>(0.0);
    var total = 0.0;
    for (var j = 1; j < 5; j++) {
        for (var i = 1; i < 5; i++) {
            let q = vec2<f32>(f32(i) - 2.0, f32(j) - 2.0) - f;
            let a = dot(q, n);
            let l = dot(q, tangent);
            let w = exp(-(a * a * across + l * l * along));
            sum += w * c[j * 6 + i];
            total += w;
        }
    }
    return mix(sharp, sum / max(total, 1e-12), amount);
}

// ---- end present filter

// Defaults are exactly identity; only the presented colour changes.
fn picture_options(rgb: vec3<f32>) -> vec3<f32> {
    return clamp(pow(max(rgb, vec3<f32>(0.0)), vec3<f32>(present.scale.z + 1.0)) * (present.scale.w + 1.0), vec3<f32>(0.0), vec3<f32>(1.0));
}

@fragment
fn fragment(mesh: VertexOutput) -> @location(0) vec4<f32> {
    let uv = uv_rect.xy + mesh.uv * uv_rect.zw;
    let c_here = present_sample(frame, uv);
    let c_prev = present_sample(prev, prev_uv.xy + mesh.uv * prev_uv.zw);
    let outside = any(uv < vec2<f32>(0.0)) || any(uv > vec2<f32>(1.0));
    let c = select(c_here, c_prev, prev_uv.z > 0.0 && outside);
    // A transparent texel is all zero, so where an opaque pixel meets a
    // transparent one the filter gives a premultiplied colour; divide it
    // back for straight-alpha blending (a no-op where alpha is 1).
    return vec4<f32>(picture_options(c.rgb / max(c.a, 1e-6)), c.a);
}
