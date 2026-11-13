//! Snapshot, compose, and upload the active terminal surface.

use std::sync::Arc;

use rune_term_core::images::Images;
use rune_term_core::render as core_render;
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
    let active = model.surfaces.shown_id();
    let hover = model
        .surfaces
        .primary()
        .and_then(|host| host.hover_link.clone());
    let hover_active = hover.is_some() && crate::hyperlink::open_mods_rune(model.held_mods);
    let preedit = model.preedit.clone();

    let force_upload = shown != model.grid_draw.search
        || hover != model.grid_draw.hover
        || hover_active != model.grid_draw.hover_active
        || preedit != model.grid_draw.preedit
        || model.palette.name != model.grid_draw.theme
        || model.grid_draw.shown != active;

    let mut drew = false;
    let shared = Arc::clone(&model.shared);
    for host in model.surfaces.iter_mut() {
        let is_active = Some(host.surface.id()) == active;
        if is_active {
            let result = core_render::sync_active_host(
                host,
                grid,
                images,
                device,
                queue,
                model.palette.clone(),
                &shown,
                hover.as_ref(),
                preedit.as_ref(),
                force_upload,
            );
            if result.rearmed {
                shared.note_rearm();
            }
            if result.drew {
                model.cursor_cell = result.cursor_cell;
                drew = true;
            }
        } else if core_render::rearm_background(host) {
            shared.note_rearm();
        }
    }
    if !drew {
        images.clear();
    } else {
        model.grid_draw.search = shown;
        model.grid_draw.shown = active;
        model.grid_draw.hover = hover;
        model.grid_draw.hover_active = hover_active;
        model.grid_draw.preedit = preedit;
        model.grid_draw.theme = model.palette.name.clone();
    }
}
