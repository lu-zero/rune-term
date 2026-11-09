//! Snapshot, then upload. Dirty bits stay set until the upload reads them.

use std::sync::atomic::Ordering;
use std::sync::Arc;

use term_grid::GridRenderer;

use crate::grid_source::SurfaceGrid;
use crate::hyperlink;
use crate::images::Images;
use crate::ime;
use crate::model::Model;
use crate::search;
use crate::selection;

pub fn sync(
    model: &mut Model,
    grid: &mut GridRenderer,
    images: &mut Images,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
) {
    let shown = search::shown(model);
    let force_search = shown != model.search_drawn;
    let active = model.surfaces.shown_id();
    let hover = model
        .surfaces
        .primary()
        .and_then(|host| host.hover_link.clone());
    let force_hover = hover != model.hover_drawn;
    let preedit = model.preedit.clone();
    let force_preedit = preedit != model.preedit_drawn;
    let force_theme = model.palette.name != model.theme_drawn;
    let switched = model.grid_shown != active;
    let mut drew = false;
    let shared = Arc::clone(&model.shared);
    for host in model.surfaces.iter_mut() {
        let resized = host.resized.swap(false, Ordering::AcqRel);
        let flagged = host.frame_flag.load(Ordering::Acquire);
        if flagged || resized {
            host.render.update_rearm(&host.frame_flag);
            shared.note_rearm();
            host.hover_probe = None;
        }
        // One grid. The selected tab is what it draws. A hidden shell still rearms.
        let mine = Some(host.surface.id()) == active;
        if !mine || host.cell_px.0 == 0 || host.cell_px.1 == 0 {
            continue;
        }
        let hits = search::collect(&host.surface, &host.render, &shown);
        let (cursor_row, cursor_col) = host.render.cursor();
        let palette = model.palette.clone();
        let source = SurfaceGrid {
            render: &mut host.render,
            palette: palette.clone(),
        };
        let source = selection::tint(source);
        let source = search::tint(source, &hits, palette);
        let source = hyperlink::tint(source, host.hover_link.as_ref());
        let mut source = ime::overlay(
            source,
            preedit.as_ref(),
            (
                u16::try_from(cursor_col).unwrap_or(0),
                u16::try_from(cursor_row).unwrap_or(0),
            ),
        );
        grid.upload(
            device,
            queue,
            &mut source,
            host.cell_px,
            resized
                || flagged
                || force_search
                || force_hover
                || force_preedit
                || force_theme
                || switched,
        );
        images.sync(device, queue, &host.render, host.cell_px);
        model.cursor_cell = (
            u16::try_from(cursor_col).unwrap_or(0),
            u16::try_from(cursor_row).unwrap_or(0),
        );
        drew = true;
    }
    if !drew {
        images.clear();
    } else {
        model.search_drawn = shown;
        model.grid_shown = active;
        model.hover_drawn = hover;
        model.preedit_drawn = preedit;
        model.theme_drawn = model.palette.name.clone();
    }
}
