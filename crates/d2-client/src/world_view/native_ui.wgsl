// The presented picture with the UI drawn at the output resolution
// (`native_ui.rs`). The picture is scaled exactly as `sharp.wgsl` scales
// the frame (the same filter function): the frame image itself (the scene, without its UI), or
// between ticks the smooth camera's world layer (shifted) under its screen
// layer. The native UI pass's two images are then read at this output
// pixel and applied in the palette's own units: `out = picture ·
// (1 − cover) + add`. Where there is no UI both are zero and the picture
// passes through unchanged.

#import bevy_sprite::mesh2d_vertex_output::VertexOutput

// `sharp.wgsl`'s `Present`: scale, picture options and scaling filter.
struct Present {
    scale: vec4<f32>,
    kind: vec4<f32>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var frame: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var frame_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var world: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(3) var screen: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(4) var cover: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(5) var add: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(6) var<uniform> present: Present;
// The world layer's uv origin (xy) and size (zw).
@group(#{MATERIAL_BIND_GROUP}) @binding(7) var<uniform> world_uv: vec4<f32>;
// x: 1 shows the world and screen layers, 0 the frame. y: 1 while the
// post pass runs (`post.rs`): the picture alone, which the pass finishes
// and lays the UI on. z: an exponent on the picture before the UI is laid
// on (the realm lobby's picture, `app::lobby_picture`); 0: none.
@group(#{MATERIAL_BIND_GROUP}) @binding(8) var<uniform> flags: vec4<f32>;
// The tick before's world layer and its uv origin (xy) and size (zw;
// zero: no fill). It fills the strip the shifted world layer uncovers.
@group(#{MATERIAL_BIND_GROUP}) @binding(9) var prev: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(10) var<uniform> prev_uv: vec4<f32>;

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

// Defaults are exactly identity; only the presented colour changes.
fn picture_options(rgb: vec3<f32>) -> vec3<f32> {
    return clamp(pow(max(rgb, vec3<f32>(0.0)), vec3<f32>(present.scale.z + 1.0)) * (present.scale.w + 1.0), vec3<f32>(0.0), vec3<f32>(1.0));
}

@fragment
fn fragment(mesh: VertexOutput) -> @location(0) vec4<f32> {
    var picture: vec3<f32>;
    if flags.x > 0.5 {
        let wuv = world_uv.xy + mesh.uv * world_uv.zw;
        var w: vec4<f32>;
        if prev_uv.z > 0.0 && (any(wuv < vec2<f32>(0.0)) || any(wuv > vec2<f32>(1.0))) {
            w = present_sample(prev, prev_uv.xy + mesh.uv * prev_uv.zw);
        } else {
            w = present_sample(world, wuv);
        }
        let s = present_sample(screen, mesh.uv);
        picture = s.rgb + w.rgb * (1.0 - s.a);
    } else {
        picture = present_sample(frame, mesh.uv).rgb;
    }
    if flags.z > 0.0 {
        picture = pow(max(picture, vec3<f32>(0.0)), vec3<f32>(flags.z));
    }
    if flags.y > 0.5 {
        return vec4<f32>(picture, 1.0);
    }
    let size = vec2<i32>(textureDimensions(cover));
    let at = clamp(vec2<i32>(floor(mesh.uv * vec2<f32>(size))), vec2<i32>(0), size - 1);
    let c = textureLoad(cover, at, 0).rgb;
    let a = textureLoad(add, at, 0).rgb;
    if all(c == vec3<f32>(0.0)) && all(a == vec3<f32>(0.0)) {
        return vec4<f32>(picture_options(picture), 1.0);
    }
    let out = to_gamma(picture) * (1.0 - c) + a;
    return vec4<f32>(picture_options(to_linear(clamp(out, vec3<f32>(0.0), vec3<f32>(1.0)))), 1.0);
}
