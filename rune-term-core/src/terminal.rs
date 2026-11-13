//! Reusable terminal runtime state.

use librio::SurfaceId;
use rune::Mods;

use crate::ime::Preedit;
use crate::render::GridDrawStamp;
use crate::surfaces::{HostSurface, Surfaces};

#[derive(Default)]
pub struct State {
    pub surfaces: Surfaces,
    pub preedit: Option<Preedit>,
    pub cursor_cell: (u16, u16),
    pub blink_phase_on: bool,
    pub held_mods: Mods,
    pub grid_draw: GridDrawStamp,
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
}
