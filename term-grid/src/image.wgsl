// Kitty/sixel image quads. Straight sRGB in, premultiplied out like the grid.

struct Uniforms {
    projection: mat4x4<f32>,
    flags: u32,
    _pad: vec3<u32>,
};

const FLAG_LINEAR_BLENDING: u32 = 2u;

fn srgb_to_linear(c: vec3<f32>) -> vec3<f32> {
    let lo = c / 12.92;
    let hi = pow((c + vec3<f32>(0.055)) / vec3<f32>(1.055), vec3<f32>(2.4));
    return select(lo, hi, c > vec3<f32>(0.04045));
}

fn prepare_rgb(srgb: vec3<f32>, flags: u32) -> vec3<f32> {
    if ((flags & FLAG_LINEAR_BLENDING) != 0u) {
        return srgb_to_linear(srgb);
    }
    return srgb;
}

@group(0) @binding(0) var<uniform> uniforms: Uniforms;
@group(1) @binding(0) var image_tex: texture_2d<f32>;
@group(1) @binding(1) var image_sampler: sampler;

struct VertexIn {
    @location(0) quad: vec4<f32>,
    @location(1) uv: vec4<f32>,
};

struct VsOut {
    @builtin(position) position: vec4<f32>,
    @location(0) tex_coord: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vid: u32, in: VertexIn) -> VsOut {
    var corner: vec2<f32>;
    corner.x = select(0.0, 1.0, vid == 1u || vid == 3u);
    corner.y = select(0.0, 1.0, vid == 2u || vid == 3u);
    let pos = in.quad.xy + in.quad.zw * corner;
    var out: VsOut;
    out.position = uniforms.projection * vec4<f32>(pos, 0.0, 1.0);
    out.tex_coord = in.uv.xy + (in.uv.zw - in.uv.xy) * corner;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let sample = textureSample(image_tex, image_sampler, in.tex_coord);
    let rgb = prepare_rgb(sample.rgb, uniforms.flags);
    let a = sample.a;
    return vec4<f32>(rgb * a, a);
}
