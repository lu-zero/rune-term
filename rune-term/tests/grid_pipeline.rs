//! Headless grid pass. Opaque gray stays gray in both blend modes. An
//! alpha-128 cell is premultiplied once: about 64 on an sRGB target, and the
//! linear value re-encoded by an sRGB view in the other mode.

use term_grid::{CellSample, CursorSample, DrawSlot, GridRenderer, GridSource};

#[test]
fn premul_gray_survives_both_blends() {
    let gpu = rune::render::Gpu::headless();
    let srgb = render(&gpu, true);
    let linear = render(&gpu, false);
    assert_near("srgb opaque", srgb.0, [128, 128, 128, 255], 3);
    assert_near("srgb alpha", srgb.1, [64, 64, 64, 128], 3);
    assert_near("linear opaque", linear.0, [128, 128, 128, 255], 3);
    let encoded = linear_alpha_byte();
    assert_near(
        "linear alpha",
        linear.1,
        [encoded, encoded, encoded, 128],
        4,
    );
}

fn render(gpu: &rune::render::Gpu, srgb_blend: bool) -> ([u8; 4], [u8; 4]) {
    let format = if srgb_blend {
        wgpu::TextureFormat::Rgba8Unorm
    } else {
        wgpu::TextureFormat::Rgba8UnormSrgb
    };
    let mut grid = GridRenderer::new(&gpu.device, &gpu.queue, format);
    let mut source = GrayGrid::new();
    grid.upload(&gpu.device, &gpu.queue, &mut source, (16, 16), true);
    grid.publish(
        &gpu.queue,
        DrawSlot {
            origin: [0.0, 0.0],
            size: [32.0, 16.0],
            target: [32, 16],
            pass_origin: [0.0, 0.0],
        },
        srgb_blend,
        true,
    );
    let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("grid-readback"),
        size: wgpu::Extent3d {
            width: 32,
            height: 16,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&Default::default());
    let mut encoder = gpu.device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("grid-readback"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        grid.draw(&mut pass);
    }
    let readback = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("grid-readback"),
        size: 256 * 16,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &readback,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(256),
                rows_per_image: Some(16),
            },
        },
        texture.size(),
    );
    gpu.queue.submit([encoder.finish()]);
    let slice = readback.slice(..);
    slice.map_async(wgpu::MapMode::Read, |result| result.expect("map readback"));
    gpu.device
        .poll(wgpu::PollType::wait_indefinitely())
        .expect("poll readback");
    let bytes = slice.get_mapped_range().expect("mapped range").to_vec();
    readback.unmap();
    (pixel(&bytes, 8, 8), pixel(&bytes, 24, 8))
}

fn pixel(bytes: &[u8], x: u32, y: u32) -> [u8; 4] {
    let i = (y * 256 + x * 4) as usize;
    [bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]]
}

fn assert_near(label: &str, got: [u8; 4], expect: [u8; 4], tol: u8) {
    for (g, e) in got.iter().zip(expect) {
        assert!(g.abs_diff(e) <= tol, "{label} {got:?} expected {expect:?}");
    }
}

/// sRGB 128/255 converted to linear, scaled by alpha 128/255, then encoded.
fn linear_alpha_byte() -> u8 {
    let srgb = 128.0_f32 / 255.0;
    let linear = if srgb <= 0.04045 {
        srgb / 12.92
    } else {
        ((srgb + 0.055) / 1.055).powf(2.4)
    };
    let premul = linear * srgb;
    let encoded = if premul <= 0.0031308 {
        premul * 12.92
    } else {
        1.055 * premul.powf(1.0 / 2.4) - 0.055
    };
    (encoded * 255.0).round() as u8
}

struct GrayGrid {
    cells: [CellSample; 2],
}

impl GrayGrid {
    fn new() -> Self {
        let cell = |alpha| CellSample {
            ch: ' ',
            width: 0,
            fg: [0, 0, 0, 0],
            bg: [128, 128, 128, alpha],
            underline: false,
        };
        Self {
            cells: [cell(255), cell(128)],
        }
    }
}

impl GridSource for GrayGrid {
    fn columns(&self) -> usize {
        2
    }
    fn lines(&self) -> usize {
        1
    }
    fn row_dirty(&self, _: usize) -> bool {
        true
    }
    fn cell(&self, _: usize, col: usize) -> CellSample {
        self.cells[col]
    }
    fn cursor(&self) -> CursorSample {
        CursorSample {
            col: 0,
            row: 0,
            visible: false,
            color: [0.0; 4],
            bg: [0.0; 4],
        }
    }
    fn reset_dirty(&mut self) {}
}
