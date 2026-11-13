//! Snapshot, compose, and upload the active terminal surface.

use std::sync::Arc;

use rune_term_core::images::Images;
use term_grid::GridRenderer;

use crate::model::Model;
use crate::search;

pub fn sync(
    model: &mut Model,
    grid: &mut GridRenderer,
    images: &mut Images,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
) {
    let shown = search::shown(model);
    let hover_active_mods = crate::hyperlink::open_mods_rune(model.terminal.held_mods());
    let palette = model.palette.clone();
    let report = model.terminal.sync_render(
        grid,
        images,
        device,
        queue,
        palette,
        &shown,
        hover_active_mods,
    );
    if report.rearms > 0 {
        let shared = Arc::clone(&model.shared);
        for _ in 0..report.rearms {
            shared.note_rearm();
        }
    }
}
