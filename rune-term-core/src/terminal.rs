//! Reusable terminal runtime state.

use std::sync::Arc;

use librio::SurfaceId;
use rune::Mods;
use term_grid::GridRenderer;

use crate::hyperlink::HoverLink;
use crate::images::Images;
use crate::ime::Preedit;
use crate::render::{self, GridDrawStamp};
use crate::surfaces::{HostSurface, Surfaces};
use crate::theme::Palette;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RenderReport {
    pub drew: bool,
    pub rearms: usize,
}

#[derive(Default)]
pub struct State {
    surfaces: Surfaces,
    preedit: Option<Preedit>,
    cursor_cell: (u16, u16),
    blink_phase_on: bool,
    held_mods: Mods,
    grid_draw: GridDrawStamp,
}

impl State {
    pub fn new(theme_name: String) -> Self {
        Self {
            surfaces: Surfaces::new(),
            preedit: None,
            cursor_cell: (0, 0),
            blink_phase_on: true,
            held_mods: Mods::default(),
            grid_draw: GridDrawStamp {
                theme: theme_name,
                ..GridDrawStamp::default()
            },
        }
    }

    pub fn primary(&self) -> Option<&HostSurface> {
        self.surfaces.primary()
    }

    pub fn primary_mut(&mut self) -> Option<&mut HostSurface> {
        self.surfaces.primary_mut()
    }

    pub fn by_mark(&self, mark: u32) -> Option<&HostSurface> {
        self.surfaces.by_mark(mark)
    }

    pub fn by_mark_mut(&mut self, mark: u32) -> Option<&mut HostSurface> {
        self.surfaces.by_mark_mut(mark)
    }

    pub fn shown_id(&self) -> Option<SurfaceId> {
        self.surfaces.shown_id()
    }

    pub fn show(&mut self, id: SurfaceId) {
        self.surfaces.show(id);
    }

    pub fn insert(&mut self, host: HostSurface) {
        self.surfaces.insert(host);
    }

    pub fn remove(&mut self, id: SurfaceId) -> Option<HostSurface> {
        self.surfaces.remove(id)
    }

    pub fn get(&self, id: SurfaceId) -> Option<&HostSurface> {
        self.surfaces.get(id)
    }

    pub fn get_mut(&mut self, id: SurfaceId) -> Option<&mut HostSurface> {
        self.surfaces.get_mut(id)
    }

    pub fn len(&self) -> usize {
        self.surfaces.len()
    }

    pub fn is_empty(&self) -> bool {
        self.surfaces.is_empty()
    }

    pub fn focus(&self, mark: u32) -> bool {
        self.surfaces.focus(mark)
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut HostSurface> {
        self.surfaces.iter_mut()
    }

    pub fn invalidate_theme(&mut self) {
        self.grid_draw.invalidate_theme();
    }

    pub fn clear_preedit(&mut self) {
        self.preedit = None;
    }

    pub fn set_preedit(&mut self, text: String, cursor: Option<(usize, usize)>) {
        crate::ime::set_preedit(&mut self.preedit, text, cursor);
    }

    /// Clears preedit and commits text to the primary surface, if any.
    pub fn commit_preedit_text(&mut self, text: &str) -> bool {
        self.preedit = None;
        self.primary()
            .is_some_and(|host| crate::ime::commit_text(&host.surface, text))
    }

    pub fn set_held_mods(&mut self, mods: Mods) -> bool {
        let changed = self.held_mods != mods;
        self.held_mods = mods;
        changed
    }

    pub fn held_mods(&self) -> Mods {
        self.held_mods
    }

    pub fn cursor_cell(&self) -> (u16, u16) {
        self.cursor_cell
    }

    pub fn blink_phase_on(&self) -> bool {
        self.blink_phase_on
    }

    pub fn preedit(&self) -> Option<&Preedit> {
        self.preedit.as_ref()
    }

    pub fn any_hover_link(&mut self) -> bool {
        self.surfaces
            .iter_mut()
            .any(|host| host.hover_link.is_some())
    }

    pub fn hover_for_render(&self) -> Option<HoverLink> {
        self.primary().and_then(|host| host.hover_link.clone())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn sync_render(
        &mut self,
        grid: &mut GridRenderer,
        images: &mut Images,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        palette: Arc<Palette>,
        search_pattern: &str,
        hover_active_mods: bool,
    ) -> RenderReport {
        let active = self.shown_id();
        let hover = self.hover_for_render();
        let hover_active = hover.is_some() && hover_active_mods;
        let preedit = self.preedit.clone();

        let force_upload = search_pattern != self.grid_draw.search
            || hover != self.grid_draw.hover
            || hover_active != self.grid_draw.hover_active
            || preedit != self.grid_draw.preedit
            || palette.name != self.grid_draw.theme
            || self.grid_draw.shown != active;

        let mut report = RenderReport::default();
        let mut cursor_cell = self.cursor_cell;
        for host in self.surfaces.iter_mut() {
            let is_active = Some(host.surface.id()) == active;
            if is_active {
                let result = render::sync_active_host(
                    host,
                    grid,
                    images,
                    device,
                    queue,
                    palette.clone(),
                    search_pattern,
                    hover.as_ref(),
                    preedit.as_ref(),
                    force_upload,
                );
                if result.rearmed {
                    report.rearms += 1;
                }
                if result.drew {
                    cursor_cell = result.cursor_cell;
                    report.drew = true;
                }
            } else if render::rearm_background(host) {
                report.rearms += 1;
            }
        }
        if report.drew {
            self.cursor_cell = cursor_cell;
            self.grid_draw.search = search_pattern.to_owned();
            self.grid_draw.shown = active;
            self.grid_draw.hover = hover;
            self.grid_draw.hover_active = hover_active;
            self.grid_draw.preedit = preedit;
            self.grid_draw.theme = palette.name.clone();
        } else {
            images.clear();
        }
        report
    }

    pub fn set_blink_phase(&mut self, on: bool) {
        self.blink_phase_on = on;
    }

    pub fn host_for_pane(&self, pane: u32, shown_pane: u32) -> Option<&HostSurface> {
        if pane == shown_pane {
            self.primary()
        } else {
            self.by_mark(pane)
        }
    }

    pub fn primary_cell_px(&self) -> (u16, u16) {
        self.primary().map(|h| h.cell_px).unwrap_or((0, 0))
    }

    pub fn primary_mark(&self) -> Option<u32> {
        self.primary().map(|h| h.mark)
    }

    pub fn host_for_pane_mut(&mut self, pane: u32, shown_pane: u32) -> Option<&mut HostSurface> {
        if pane == shown_pane {
            self.primary_mut()
        } else {
            self.by_mark_mut(pane)
        }
    }
}
