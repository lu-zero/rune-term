// One triangle covers clip space. The compositor's scissor is already the
// effect slot, so the triangle fills that rectangle and nothing past it.

@group(0) @binding(0) var<uniform> color: vec4<f32>;

@vertex
fn vs(@builtin(vertex_index) id: u32) -> @builtin(position) vec4<f32> {
    let x = f32((id << 1u) & 2u);
    let y = f32(id & 2u);
    return vec4<f32>(x * 2.0 - 1.0, y * 2.0 - 1.0, 0.0, 1.0);
}

@fragment
fn fs() -> @location(0) vec4<f32> {
    return color;
}
