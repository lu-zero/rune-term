//! Kitty image quads. No librio dependency.

use std::collections::HashMap;

use bytemuck::{Pod, Zeroable};

use crate::sixel;

const IMAGE_SHADER: &str = include_str!("image.wgsl");

#[derive(Clone, Copy, Debug)]
pub struct ImagePlacement {
    pub image_id: u32,
    pub z_index: i32,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    /// Normalized [u0, v0, u1, v1].
    pub source_rect: [f32; 4],
}

#[derive(Clone, Copy, Debug)]
pub struct ImageInfo {
    pub width: usize,
    pub height: usize,
    pub stamp: u64,
}

pub trait ImageProvider {
    fn image_info(&self, image_id: u32) -> Option<ImageInfo>;
    fn image_rgba(&self, image_id: u32, buf: &mut [u8]) -> usize;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageBucket {
    UnderText,
    OverText,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct ImageUniforms {
    projection: [f32; 16],
    flags: u32,
    _pad: [u32; 3],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct Quad {
    rect: [f32; 4],
    uv: [f32; 4],
}

struct CachedImage {
    stamp: u64,
    _texture: wgpu::Texture,
    bind: wgpu::BindGroup,
}

pub struct ImagePass {
    pipeline: wgpu::RenderPipeline,
    _uniform_layout: wgpu::BindGroupLayout,
    image_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    uniform: wgpu::Buffer,
    uniform_bind: wgpu::BindGroup,
    vertex: wgpu::Buffer,
    vertex_cap: u64,
    images: HashMap<u32, CachedImage>,
    quads: Vec<Quad>,
}

impl ImagePass {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("images"),
            source: wgpu::ShaderSource::Wgsl(IMAGE_SHADER.into()),
        });
        let uniform_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("images.uniform"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let image_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("images.tex"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("images"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("images"),
            bind_group_layouts: &[Some(&uniform_layout), Some(&image_layout)],
            immediate_size: 0,
        });
        let attrs = [
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: 0,
                shader_location: 0,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: 16,
                shader_location: 1,
            },
        ];
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("images"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Quad>() as u64,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &attrs,
                })],
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleStrip,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(premul_blend()),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("images.uniform"),
            size: std::mem::size_of::<ImageUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let uniform_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("images.uniform"),
            layout: &uniform_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform.as_entire_binding(),
            }],
        });
        let vertex_cap = (std::mem::size_of::<Quad>() * 8) as u64;
        let vertex = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("images.quads"),
            size: vertex_cap,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self {
            pipeline,
            _uniform_layout: uniform_layout,
            image_layout,
            sampler,
            uniform,
            uniform_bind,
            vertex,
            vertex_cap,
            images: HashMap::new(),
            quads: Vec::new(),
        }
    }

    /// Ensure textures exist for placements and upload image data when stamps change.
    pub fn sync(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        placements: &[ImagePlacement],
        provider: &impl ImageProvider,
    ) {
        for placement in placements {
            let Some(info) = provider.image_info(placement.image_id) else {
                continue;
            };
            let fresh = self
                .images
                .get(&placement.image_id)
                .is_none_or(|cached| cached.stamp != info.stamp);
            if !fresh {
                continue;
            }
            let expected = info.width.saturating_mul(info.height).saturating_mul(4);
            let mut rgba = vec![0u8; expected];
            let written = provider.image_rgba(placement.image_id, &mut rgba);
            if written < expected || info.width == 0 || info.height == 0 {
                continue;
            }
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("images.tex"),
                size: wgpu::Extent3d {
                    width: info.width as u32,
                    height: info.height as u32,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            queue.write_texture(
                texture.as_image_copy(),
                &rgba,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some((info.width * 4) as u32),
                    rows_per_image: Some(info.height as u32),
                },
                texture.size(),
            );
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("images.tex"),
                layout: &self.image_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                ],
            });
            self.images.insert(
                placement.image_id,
                CachedImage {
                    stamp: info.stamp,
                    _texture: texture,
                    bind,
                },
            );
        }
        let _ = sixel::draw;
    }

    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        pass: &mut wgpu::RenderPass<'_>,
        placements: &[ImagePlacement],
        bucket: ImageBucket,
        origin: [f32; 2],
        pass_origin: [f32; 2],
        target: [u32; 2],
        srgb_blend: bool,
    ) {
        if target[0] == 0 || target[1] == 0 {
            return;
        }
        self.quads.clear();
        let mut draw_ids: Vec<u32> = Vec::new();
        for placement in placements {
            let is_under = placement.z_index < 0;
            match bucket {
                ImageBucket::UnderText if !is_under => continue,
                ImageBucket::OverText if is_under => continue,
                _ => {}
            }
            if self.images.contains_key(&placement.image_id) {
                let x = placement.x + origin[0] - pass_origin[0];
                let y = placement.y + origin[1] - pass_origin[1];
                self.quads.push(Quad {
                    rect: [x, y, placement.width, placement.height],
                    uv: placement.source_rect,
                });
                draw_ids.push(placement.image_id);
            }
        }
        if self.quads.is_empty() {
            return;
        }
        let bytes = bytemuck::cast_slice(&self.quads);
        let needed = bytes.len() as u64;
        if needed > self.vertex_cap {
            self.vertex_cap = needed.next_power_of_two();
            self.vertex = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("images.quads"),
                size: self.vertex_cap,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }
        queue.write_buffer(&self.vertex, 0, bytes);
        let uniforms = ImageUniforms {
            projection: orthographic(target[0] as f32, target[1] as f32),
            flags: if srgb_blend { 0 } else { 2 },
            _pad: [0; 3],
        };
        queue.write_buffer(&self.uniform, 0, bytemuck::bytes_of(&uniforms));

        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.uniform_bind, &[]);
        pass.set_vertex_buffer(0, self.vertex.slice(..));
        for (qi, image_id) in draw_ids.iter().enumerate() {
            if let Some(cached) = self.images.get(image_id) {
                pass.set_bind_group(1, &cached.bind, &[]);
                pass.draw(0..4, qi as u32..qi as u32 + 1);
            }
        }
        let _ = sixel::draw;
    }
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
