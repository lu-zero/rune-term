//! Terminal grid/image rendering composition.

use std::sync::atomic::Ordering;
use std::sync::Arc;

use librio::SurfaceId;
use term_grid::GridRenderer;

use crate::grid_source::SurfaceGrid;
use crate::hyperlink::{self, HoverLink};
use crate::images::Images;
use crate::ime::{self, Preedit};
use crate::search;
use crate::selection;
use crate::surfaces::HostSurface;
use crate::theme::Palette;

#[derive(Clone, Debug, Default)]
pub struct SyncResult {
    pub drew: bool,
    pub rearmed: bool,
    pub cursor_cell: (u16, u16),
}

#[allow(clippy::too_many_arguments)]
pub fn sync_active_host(
    host: &mut HostSurface,
    grid: &mut GridRenderer,
    images: &mut Images,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    palette: Arc<Palette>,
    search_pattern: &str,
    hover: Option<&HoverLink>,
    preedit: Option<&Preedit>,
    force_upload: bool,
) -> SyncResult {
    let resized = host.resized.swap(false, Ordering::AcqRel);
    let flagged = host.frame_flag.load(Ordering::Acquire);
    let rearmed = flagged || resized;
    if rearmed {
        host.render.update_rearm(&host.frame_flag);
        host.hover_probe = None;
    }
    if host.cell_px.0 == 0 || host.cell_px.1 == 0 {
        return SyncResult {
            drew: false,
            rearmed,
            cursor_cell: (0, 0),
        };
    }
    let hits = search::collect(&host.surface, &host.render, search_pattern);
    let (cursor_row, cursor_col) = host.render.cursor();
    let source = SurfaceGrid {
        render: &mut host.render,
        palette: palette.clone(),
    };
    let source = selection::tint(source);
    let source = search::tint(source, &hits, palette);
    let source = hyperlink::tint(source, hover);
    let mut source = ime::overlay(
        source,
        preedit,
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
        resized || flagged || force_upload,
    );
    images.sync(device, queue, &host.render, host.cell_px);
    SyncResult {
        drew: true,
        rearmed,
        cursor_cell: (
            u16::try_from(cursor_col).unwrap_or(0),
            u16::try_from(cursor_row).unwrap_or(0),
        ),
    }
}

/// Rearm non-active surfaces so damage flags are consumed.
pub fn rearm_background(host: &mut HostSurface) -> bool {
    let resized = host.resized.swap(false, Ordering::AcqRel);
    let flagged = host.frame_flag.load(Ordering::Acquire);
    if flagged || resized {
        host.render.update_rearm(&host.frame_flag);
        host.hover_probe = None;
        true
    } else {
        false
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GridDrawStamp {
    pub shown: Option<SurfaceId>,
    pub search: String,
    pub hover: Option<HoverLink>,
    pub hover_active: bool,
    pub preedit: Option<Preedit>,
    pub theme: String,
}

impl GridDrawStamp {
    pub fn invalidate_theme(&mut self) {
        self.theme.clear();
    }
}
