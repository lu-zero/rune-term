//! Kitty image rendering.

use librio::RenderState;
use term_grid::images::{ImageBucket, ImageInfo, ImagePass, ImagePlacement, ImageProvider};
use wgpu::{Device, Queue, RenderPass, TextureFormat};

pub struct Images {
    pass: ImagePass,
    placements: Vec<ImagePlacement>,
}

impl Images {
    pub fn new(device: &Device, format: TextureFormat) -> Self {
        Self {
            pass: ImagePass::new(device, format),
            placements: Vec::new(),
        }
    }

    pub fn sync(
        &mut self,
        device: &Device,
        queue: &Queue,
        render: &RenderState,
        cell_px: (u16, u16),
    ) {
        self.placements.clear();
        let (cw, ch) = (f32::from(cell_px.0), f32::from(cell_px.1));
        if cw <= 0.0 || ch <= 0.0 {
            return;
        }
        for index in 0..render.kitty_count() {
            let Some((image_id, z_index, geometry)) = render.kitty_geometry(index, cw, ch) else {
                continue;
            };
            self.placements.push(ImagePlacement {
                image_id,
                z_index,
                x: geometry.x,
                y: geometry.y,
                width: geometry.width,
                height: geometry.height,
                source_rect: geometry.source_rect,
            });
        }
        let provider = RenderProvider { render };
        self.pass.sync(device, queue, &self.placements, &provider);
    }

    pub fn clear(&mut self) {
        self.placements.clear();
    }

    pub fn draw_bucket(
        &mut self,
        device: &Device,
        queue: &Queue,
        pass: &mut RenderPass<'_>,
        bucket: ImageBucket,
        origin: [f32; 2],
        pass_origin: [f32; 2],
        target: [u32; 2],
        srgb_blend: bool,
    ) {
        self.pass.draw(
            device,
            queue,
            pass,
            &self.placements,
            bucket,
            origin,
            pass_origin,
            target,
            srgb_blend,
        );
    }
}

struct RenderProvider<'a> {
    render: &'a RenderState,
}

impl ImageProvider for RenderProvider<'_> {
    fn image_info(&self, image_id: u32) -> Option<ImageInfo> {
        let (width, height, stamp) = self.render.kitty_image_info(image_id)?;
        Some(ImageInfo {
            width,
            height,
            stamp,
        })
    }

    fn image_rgba(&self, image_id: u32, buf: &mut [u8]) -> usize {
        self.render.kitty_image_rgba(image_id, buf)
    }
}
