//! Two cell-buffer copies. The CPU pack is skipped when the snapshot is clean.
//! Each draw writes the other copy, so a frame still on the GPU is left alone.

use crate::atlas::Atlas;
use crate::cell::{CellBg, CellSample, CellText, CursorSample, GridSource, GridUniforms};
use crate::sprite;

const SHADER: &str = include_str!("grid.wgsl");
const FRAMES_IN_FLIGHT: usize = 2;

/// Where the grid sits in the pass, in device pixels.
#[derive(Clone, Copy, Debug)]
pub struct DrawSlot {
    /// Pane top-left in window device pixels.
    pub origin: [f32; 2],
    /// Pane size in device pixels.
    pub size: [f32; 2],
    /// Pass target size. The projection covers this.
    pub target: [u32; 2],
    /// Pass origin in window device pixels. Zero for the window target.
    pub pass_origin: [f32; 2],
}

pub struct GridRenderer {
    bg_pipeline: wgpu::RenderPipeline,
    text_pipeline: wgpu::RenderPipeline,
    bg_layout: wgpu::BindGroupLayout,
    text_layout: wgpu::BindGroupLayout,
    atlas_bind: wgpu::BindGroup,
    gray: wgpu::Texture,
    color: wgpu::Texture,
    slots: [Slot; FRAMES_IN_FLIGHT],
    frame: usize,
    atlas: Atlas,
    bg: Vec<CellBg>,
    text: Vec<CellText>,
    cols: u32,
    rows: u32,
    cell_px: (u16, u16),
    cursor: CursorSample,
    text_count: u32,
    ready: bool,
}

struct Slot {
    uniform: wgpu::Buffer,
    bg: wgpu::Buffer,
    text: wgpu::Buffer,
    bg_bind: wgpu::BindGroup,
    text_bind: wgpu::BindGroup,
}

impl GridRenderer {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("grid"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let bg_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("grid.bg"),
            entries: &[uniform_entry(0), storage_entry(1)],
        });
        let text_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("grid.text"),
            entries: &[uniform_entry(0)],
        });
        let atlas_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("grid.atlas"),
            entries: &[texture_entry(0), texture_entry(1)],
        });
        let atlas = Atlas::new();
        let gray = gray_texture(device);
        upload_gray(queue, &gray, &atlas);
        let gray_view = gray.create_view(&wgpu::TextureViewDescriptor::default());
        let color = color_texture(device);
        clear_color(queue, &color);
        let color_view = color.create_view(&wgpu::TextureViewDescriptor::default());
        let atlas_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("grid.atlas"),
            layout: &atlas_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&gray_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&color_view),
                },
            ],
        });
        let bg_pipeline = bg_pipeline(device, format, &bg_layout, &shader);
        let text_pipeline = text_pipeline(device, format, &[&text_layout, &atlas_layout], &shader);
        let slots = [0, 1].map(|_| Slot::new(device, &bg_layout, &text_layout, 16, 32));
        Self {
            bg_pipeline,
            text_pipeline,
            bg_layout,
            text_layout,
            atlas_bind,
            gray,
            color,
            slots,
            frame: 0,
            atlas,
            bg: Vec::new(),
            text: Vec::new(),
            cols: 0,
            rows: 0,
            cell_px: (0, 0),
            cursor: CursorSample {
                col: 0,
                row: 0,
                visible: false,
                color: [0.0; 4],
                bg: [0.0; 4],
            },
            text_count: 0,
            ready: false,
        }
    }

    /// Pack dirty rows into the CPU cache. `force` rebuilds after a resize.
    pub fn upload(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        source: &mut impl GridSource,
        cell_px: (u16, u16),
        force: bool,
    ) {
        let Ok(cols) = u32::try_from(source.columns()) else {
            return;
        };
        let Ok(rows) = u32::try_from(source.lines()) else {
            return;
        };
        self.cursor = source.cursor();
        if cols == 0 || rows == 0 || cell_px.0 == 0 || cell_px.1 == 0 {
            source.reset_dirty();
            return;
        }
        let Some(n) = cols.checked_mul(rows) else {
            source.reset_dirty();
            return;
        };
        let size_changed =
            !self.ready || self.cols != cols || self.rows != rows || self.cell_px != cell_px;
        let dirty = (0..rows as usize).any(|line| source.row_dirty(line));
        if !force && !size_changed && !dirty {
            source.reset_dirty();
            return;
        }
        let cell = (u32::from(cell_px.0), u32::from(cell_px.1));
        if self.atlas.cell() != cell {
            self.atlas = Atlas::with_cell(cell.0, cell.1);
            upload_gray(queue, &self.gray, &self.atlas);
            clear_color(queue, &self.color);
        }
        self.cols = cols;
        self.rows = rows;
        self.cell_px = cell_px;
        self.bg.clear();
        self.text.clear();
        self.bg.resize(n as usize, CellBg::TRANSPARENT);
        for line in 0..rows as usize {
            for col in 0..cols as usize {
                let sample = source.cell(line, col);
                self.bg[line * cols as usize + col] = CellBg { rgba: sample.bg };
                push_glyph(&mut self.text, &mut self.atlas, &sample, col, line);
            }
        }
        if self.atlas.take_dirty() {
            upload_gray(queue, &self.gray, &self.atlas);
        }
        if self.atlas.take_color_dirty() {
            upload_color(queue, &self.color, &self.atlas);
        }
        self.text_count = u32::try_from(self.text.len()).unwrap_or(u32::MAX);
        let bg_bytes = (self.bg.len() * std::mem::size_of::<CellBg>()).max(16) as u64;
        let text_bytes = (self.text.len().max(1) * std::mem::size_of::<CellText>()).max(32) as u64;
        for slot in &mut self.slots {
            if slot.bg.size() < bg_bytes || slot.text.size() < text_bytes {
                let bg_size = slot.bg.size().max(bg_bytes);
                let text_size = slot.text.size().max(text_bytes);
                *slot = Slot::new(
                    device,
                    &self.bg_layout,
                    &self.text_layout,
                    bg_size,
                    text_size,
                );
            }
        }
        self.ready = true;
        source.reset_dirty();
    }

    /// Write this frame's copy. Call before the render pass that [`draw`]s it.
    pub fn publish(
        &mut self,
        queue: &wgpu::Queue,
        slot: DrawSlot,
        srgb_blend: bool,
        cursor_on: bool,
    ) {
        if !self.ready || self.bg.is_empty() || slot.target[0] == 0 || slot.target[1] == 0 {
            return;
        }
        let uniforms = self.uniforms(slot, srgb_blend, cursor_on);
        self.frame = (self.frame + 1) % FRAMES_IN_FLIGHT;
        let gpu = &self.slots[self.frame];
        queue.write_buffer(&gpu.bg, 0, bytemuck::cast_slice(&self.bg));
        if !self.text.is_empty() {
            queue.write_buffer(&gpu.text, 0, bytemuck::cast_slice(&self.text));
        }
        queue.write_buffer(&gpu.uniform, 0, bytemuck::bytes_of(&uniforms));
    }

    pub fn draw(&self, pass: &mut wgpu::RenderPass<'_>) {
        if !self.ready || self.bg.is_empty() {
            return;
        }
        let gpu = &self.slots[self.frame];
        pass.set_pipeline(&self.bg_pipeline);
        pass.set_bind_group(0, &gpu.bg_bind, &[]);
        pass.draw(0..3, 0..1);
        if self.text_count == 0 {
            return;
        }
        pass.set_pipeline(&self.text_pipeline);
        pass.set_bind_group(0, &gpu.text_bind, &[]);
        pass.set_bind_group(1, &self.atlas_bind, &[]);
        pass.set_vertex_buffer(0, gpu.text.slice(..));
        pass.draw(0..4, 0..self.text_count);
    }

    fn uniforms(&self, slot: DrawSlot, srgb_blend: bool, cursor_on: bool) -> GridUniforms {
        let width = slot.target[0].max(1) as f32;
        let height = slot.target[1].max(1) as f32;
        let left = slot.origin[0] - slot.pass_origin[0];
        let top = slot.origin[1] - slot.pass_origin[1];
        let grid_w = self.cols as f32 * f32::from(self.cell_px.0);
        let grid_h = self.rows as f32 * f32::from(self.cell_px.1);
        let right = (slot.size[0] - grid_w).max(0.0);
        let bottom = (slot.size[1] - grid_h).max(0.0);
        let show = cursor_on && self.cursor.visible;
        let mut cursor_fg = self.cursor.color;
        let mut cursor_bg = self.cursor.bg;
        if !show {
            cursor_fg[3] = 0.0;
            cursor_bg[3] = 0.0;
        }
        GridUniforms {
            projection: orthographic(width, height),
            grid_padding: [top, right, bottom, left],
            cursor_color: cursor_fg,
            cursor_bg_color: cursor_bg,
            cell_size: [f32::from(self.cell_px.0), f32::from(self.cell_px.1)],
            grid_size: [self.cols, self.rows],
            cursor_pos: if show {
                [self.cursor.col, self.cursor.row]
            } else {
                [u32::MAX, u32::MAX]
            },
            _pad_cursor: [0, 0],
            min_contrast: 0.0,
            flags: if srgb_blend {
                0
            } else {
                GridUniforms::FLAG_LINEAR_BLENDING
            },
            padding_extend: 0,
            input_colorspace: 0,
        }
    }
}

impl Slot {
    fn new(
        device: &wgpu::Device,
        bg_layout: &wgpu::BindGroupLayout,
        text_layout: &wgpu::BindGroupLayout,
        bg_size: u64,
        text_size: u64,
    ) -> Self {
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("grid.uniform"),
            size: std::mem::size_of::<GridUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bg = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("grid.bg"),
            size: bg_size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let text = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("grid.text"),
            size: text_size,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bg_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("grid.bg"),
            layout: bg_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: bg.as_entire_binding(),
                },
            ],
        });
        let text_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("grid.text"),
            layout: text_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform.as_entire_binding(),
            }],
        });
        Self {
            uniform,
            bg,
            text,
            bg_bind,
            text_bind,
        }
    }
}

fn push_glyph(
    text: &mut Vec<CellText>,
    atlas: &mut Atlas,
    sample: &CellSample,
    col: usize,
    line: usize,
) {
    if sample.width == 0 {
        return;
    }
    // A wide glyph's bitmap already fills both cells. The spacer cell has width 0.
    let cells = if sample.width >= 2 { 2 } else { 1 };
    let Some(rect) = sprite::sprite(sample.ch).or_else(|| atlas.ensure(sample.ch, cells)) else {
        return;
    };
    let Ok(x) = u32::try_from(col) else {
        return;
    };
    let Ok(y) = u32::try_from(line) else {
        return;
    };
    text.push(CellText {
        glyph_pos: [rect.x, rect.y],
        glyph_size: [rect.w, rect.h],
        bearings: [0, 0],
        grid_pos: [x, y],
        color: sample.fg,
        atlas: rect.atlas,
        bools: 0,
        page: 0,
    });
}

fn orthographic(width: f32, height: f32) -> [f32; 16] {
    [
        2.0 / width,
        0.0,
        0.0,
        0.0,
        0.0,
        -2.0 / height,
        0.0,
        0.0,
        0.0,
        0.0,
        1.0,
        0.0,
        -1.0,
        1.0,
        0.0,
        1.0,
    ]
}

fn uniform_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

fn storage_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only: true },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

fn texture_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: false },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    }
}

fn gray_texture(device: &wgpu::Device) -> wgpu::Texture {
    let size = crate::atlas::ATLAS_SIZE;
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("grid.atlas.gray"),
        size: wgpu::Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::R8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    })
}

fn upload_gray(queue: &wgpu::Queue, texture: &wgpu::Texture, atlas: &Atlas) {
    let size = crate::atlas::ATLAS_SIZE;
    queue.write_texture(
        texture.as_image_copy(),
        atlas.pixels(),
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(size),
            rows_per_image: Some(size),
        },
        texture.size(),
    );
}

fn color_texture(device: &wgpu::Device) -> wgpu::Texture {
    let size = crate::atlas::ATLAS_SIZE;
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("grid.atlas.color"),
        size: wgpu::Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    })
}

fn clear_color(queue: &wgpu::Queue, texture: &wgpu::Texture) {
    let size = crate::atlas::ATLAS_SIZE;
    let bytes = vec![0u8; (size as usize) * (size as usize) * 4];
    queue.write_texture(
        texture.as_image_copy(),
        &bytes,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(size * 4),
            rows_per_image: Some(size),
        },
        texture.size(),
    );
}

fn upload_color(queue: &wgpu::Queue, texture: &wgpu::Texture, atlas: &Atlas) {
    let Some(pixels) = atlas.color_pixels() else {
        return;
    };
    let size = crate::atlas::ATLAS_SIZE;
    queue.write_texture(
        texture.as_image_copy(),
        pixels,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(size * 4),
            rows_per_image: Some(size),
        },
        texture.size(),
    );
}

fn premul_blend() -> wgpu::BlendState {
    wgpu::BlendState {
        color: wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::One,
            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
            operation: wgpu::BlendOperation::Add,
        },
        alpha: wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::One,
            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
            operation: wgpu::BlendOperation::Add,
        },
    }
}

fn bg_pipeline(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    layout: &wgpu::BindGroupLayout,
    shader: &wgpu::ShaderModule,
) -> wgpu::RenderPipeline {
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("grid.bg"),
        bind_group_layouts: &[Some(layout)],
        immediate_size: 0,
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("grid.bg"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("grid_bg_vertex"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            ..Default::default()
        },
        depth_stencil: None,
        multisample: Default::default(),
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some("grid_bg_fragment"),
            compilation_options: Default::default(),
            targets: &[Some(color_target(format))],
        }),
        multiview_mask: None,
        cache: None,
    })
}

fn text_pipeline(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    layouts: &[&wgpu::BindGroupLayout],
    shader: &wgpu::ShaderModule,
) -> wgpu::RenderPipeline {
    let layouts: Vec<Option<&wgpu::BindGroupLayout>> = layouts.iter().copied().map(Some).collect();
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("grid.text"),
        bind_group_layouts: &layouts,
        immediate_size: 0,
    });
    let attrs = [
        wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Uint32x2,
            offset: 0,
            shader_location: 0,
        },
        wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Uint32x2,
            offset: 8,
            shader_location: 1,
        },
        wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Sint32x2,
            offset: 16,
            shader_location: 2,
        },
        wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Uint32x2,
            offset: 24,
            shader_location: 3,
        },
        wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Unorm8x4,
            offset: 32,
            shader_location: 4,
        },
        wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Uint32,
            offset: 36,
            shader_location: 5,
        },
        wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Uint32,
            offset: 40,
            shader_location: 6,
        },
    ];
    let buffer = wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<CellText>() as u64,
        step_mode: wgpu::VertexStepMode::Instance,
        attributes: &attrs,
    };
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("grid.text"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("grid_text_vertex"),
            compilation_options: Default::default(),
            buffers: &[Some(buffer)],
        },
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleStrip,
            ..Default::default()
        },
        depth_stencil: None,
        multisample: Default::default(),
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some("grid_text_fragment"),
            compilation_options: Default::default(),
            targets: &[Some(color_target(format))],
        }),
        multiview_mask: None,
        cache: None,
    })
}

fn color_target(format: wgpu::TextureFormat) -> wgpu::ColorTargetState {
    wgpu::ColorTargetState {
        format,
        blend: Some(premul_blend()),
        write_mask: wgpu::ColorWrites::ALL,
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_grid_shader_premultiplies_from_straight_color() {
        let src = include_str!("grid.wgsl");
        assert!(src.contains("FLAG_LINEAR_BLENDING"));
        assert!(src.contains("grid_bg_vertex"));
        assert!(src.contains("rgb * a"));
        assert!(!src.contains("display_p3"));
    }
}
