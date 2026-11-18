//! Snapshot, compose, and upload the panes on screen.

use std::sync::Arc;

use rune_term_core::images::Images;
use rune_term_core::terminal::PaneGrid;

use crate::model::Model;
use crate::search;

pub fn sync(
    model: &mut Model,
    panes: &mut [PaneGrid<'_>],
    images: &mut Images,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
) {
    let shown = search::shown(model);
    let hover_active_mods = crate::hyperlink::open_mods_rune(model.terminal.held_mods());
    let palette = model.palette.clone();
    let report = model.terminal.sync_render(
        panes,
        images,
        device,
        queue,
        palette,
        &model.face,
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
