//! Snapshot, then upload. Dirty bits stay set until the upload reads them.

use std::sync::atomic::Ordering;
use std::sync::Arc;

use term_grid::GridRenderer;

use crate::grid_source::SurfaceGrid;
use crate::images;
use crate::model::Model;
use crate::search;
use crate::selection;

pub fn sync(
    model: &mut Model,
    grid: &mut GridRenderer,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
) {
    let shown = search::shown(model);
    let force_search = shown != model.search_drawn;
    let active = model.surfaces.shown_id();
    let switched = model.grid_shown != active;
    let mut drew = false;
    let shared = Arc::clone(&model.shared);
    for host in model.surfaces.iter_mut() {
        let resized = host.resized.swap(false, Ordering::AcqRel);
        let flagged = host.frame_flag.load(Ordering::Acquire);
        if flagged || resized {
            host.render.update_rearm(&host.frame_flag);
            shared.note_rearm();
        }
        // One grid. The selected tab is what it draws. A hidden shell still rearms.
        let mine = Some(host.surface.id()) == active;
        if !mine || host.cell_px.0 == 0 || host.cell_px.1 == 0 {
            images::draw();
            continue;
        }
        let hits = search::collect(&host.surface, &host.render, &shown);
        let source = SurfaceGrid {
            render: &mut host.render,
        };
        let source = selection::tint(source);
        let mut source = search::tint(source, &hits);
        grid.upload(
            device,
            queue,
            &mut source,
            host.cell_px,
            resized || flagged || force_search || switched,
        );
        drew = true;
        images::draw();
    }
    if drew {
        model.search_drawn = shown;
        model.grid_shown = active;
    }
}
