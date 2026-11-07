// Grid shader. Straight sRGB comes from the CPU. FLAG_LINEAR_BLENDING
// (bit 1) writes linear premultiplied color; otherwise the output stays
// sRGB-encoded and premultiplied. Display P3 is not applied.

struct Uniforms {
    projection:       mat4x4<f32>,
    grid_padding:     vec4<f32>,
    cursor_color:     vec4<f32>,
    cursor_bg_color:  vec4<f32>,
    cell_size:        vec2<f32>,
    grid_size:        vec2<u32>,
    cursor_pos:       vec2<u32>,
    _pad_cursor:      vec2<u32>,
    min_contrast:     f32,
    flags:            u32,
    padding_extend:   u32,
    input_colorspace: u32,
};

const FLAG_LINEAR_BLENDING: u32 = 2u;

fn grid_srgb_to_linear(c: vec3<f32>) -> vec3<f32> {
    let lo = c / 12.92;
    let hi = pow((c + vec3<f32>(0.055)) / vec3<f32>(1.055), vec3<f32>(2.4));
    return select(lo, hi, c > vec3<f32>(0.04045));
}

fn grid_prepare_output_rgb(srgb: vec3<f32>, flags: u32) -> vec3<f32> {
    if ((flags & FLAG_LINEAR_BLENDING) != 0u) {
        return grid_srgb_to_linear(srgb);
    }
    return srgb;
}

@group(0) @binding(0) var<uniform> uniforms: Uniforms;
@group(0) @binding(1) var<storage, read> cells: array<u32>;

struct VsOut {
    @builtin(position) position: vec4<f32>,
};

@vertex
fn grid_bg_vertex(@builtin(vertex_index) vid: u32) -> VsOut {
    var x = -1.0;
    var y = 1.0;
    if (vid == 2u) { x = 3.0; }
    if (vid == 0u) { y = -3.0; }
    var out: VsOut;
    out.position = vec4<f32>(x, y, 1.0, 1.0);
    return out;
}

@fragment
fn grid_bg_fragment(in: VsOut) -> @location(0) vec4<f32> {
    let cell_fx = (in.position.xy - vec2<f32>(uniforms.grid_padding.w, uniforms.grid_padding.x))
        / uniforms.cell_size;
    let orig_grid_pos = vec2<i32>(floor(cell_fx));
    var grid_pos = orig_grid_pos;
    let cols = i32(uniforms.grid_size.x);
    let rows = i32(uniforms.grid_size.y);
    if (grid_pos.x < 0 || grid_pos.x > cols - 1 || grid_pos.y < 0 || grid_pos.y > rows - 1) {
        return vec4<f32>(0.0);
    }
    if (uniforms.cursor_bg_color.a > 0.0
        && orig_grid_pos.x == i32(uniforms.cursor_pos.x)
        && orig_grid_pos.y == i32(uniforms.cursor_pos.y)) {
        let rgb = grid_prepare_output_rgb(uniforms.cursor_bg_color.rgb, uniforms.flags);
        let a = uniforms.cursor_bg_color.a;
        return vec4<f32>(rgb * a, a);
    }
    let idx = u32(grid_pos.y) * uniforms.grid_size.x + u32(grid_pos.x);
    let word = cells[idx];
    let r = f32((word >> 0u) & 0xFFu) / 255.0;
    let g = f32((word >> 8u) & 0xFFu) / 255.0;
    let b = f32((word >> 16u) & 0xFFu) / 255.0;
    let a = f32((word >> 24u) & 0xFFu) / 255.0;
    let rgb = grid_prepare_output_rgb(vec3<f32>(r, g, b), uniforms.flags);
    return vec4<f32>(rgb * a, a);
}

struct CellTextVertexIn {
    @location(0) glyph_pos:  vec2<u32>,
    @location(1) glyph_size: vec2<u32>,
    @location(2) bearings:   vec2<i32>,
    @location(3) grid_pos:   vec2<u32>,
    @location(4) color:      vec4<f32>,
    @location(5) atlas:      u32,
    @location(6) bools:      u32,
};

struct TextVsOut {
    @builtin(position) position: vec4<f32>,
    @location(0) @interpolate(flat) atlas: u32,
    @location(1) @interpolate(flat) color: vec4<f32>,
    @location(2) tex_coord: vec2<f32>,
};

@group(1) @binding(0) var atlas_grayscale: texture_2d<f32>;
@group(1) @binding(1) var atlas_color: texture_2d<f32>;

@vertex
fn grid_text_vertex(
    @builtin(vertex_index) vid: u32,
    in: CellTextVertexIn,
) -> TextVsOut {
    let cell_pos = uniforms.cell_size * vec2<f32>(in.grid_pos);
    var corner: vec2<f32>;
    corner.x = select(0.0, 1.0, vid == 1u || vid == 3u);
    corner.y = select(0.0, 1.0, vid == 2u || vid == 3u);
    // The face is rasterized into the cell. Draw that bitmap at its pixel size.
    let origin = cell_pos + vec2<f32>(in.bearings);
    var quad = origin + vec2<f32>(in.glyph_size) * corner;
    quad.x += uniforms.grid_padding.w;
    quad.y += uniforms.grid_padding.x;
    var out: TextVsOut;
    out.position = uniforms.projection * vec4<f32>(quad, 0.0, 1.0);
    out.tex_coord = vec2<f32>(in.glyph_pos) + vec2<f32>(in.glyph_size) * corner;
    out.atlas = in.atlas;
    var color = in.color;
    color = vec4<f32>(
        grid_prepare_output_rgb(color.rgb, uniforms.flags) * color.a,
        color.a,
    );
    let is_cursor_pos = in.grid_pos.x == uniforms.cursor_pos.x
        && in.grid_pos.y == uniforms.cursor_pos.y;
    if ((in.bools & 2u) == 0u && is_cursor_pos && uniforms.cursor_color.a > 0.0) {
        let c = uniforms.cursor_color;
        color = vec4<f32>(
            grid_prepare_output_rgb(c.rgb, uniforms.flags) * c.a,
            c.a,
        );
    }
    out.color = color;
    return out;
}

@fragment
fn grid_text_fragment(in: TextVsOut) -> @location(0) vec4<f32> {
    let ic = vec2<i32>(in.tex_coord);
    if (in.atlas == 0u) {
        let a = textureLoad(atlas_grayscale, ic, 0).r;
        return in.color * a;
    }
    return textureLoad(atlas_color, ic, 0);
}
