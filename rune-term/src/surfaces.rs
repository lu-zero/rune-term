//! One `HostSurface`. Later panes are more entries in the same map.

use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use librio::{RenderState, Surface, SurfaceId};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Progress {
    pub state: u8,
    pub value: u8,
}

pub struct HostSurface {
    pub surface: Surface,
    pub render: RenderState,
    pub mark: u32,
    pub title: String,
    pub bell: bool,
    pub progress: Option<Progress>,
    pub blink: bool,
    pub frame_flag: Arc<AtomicBool>,
    pub resized: AtomicBool,
    pub cols: u16,
    pub rows: u16,
    pub cell_px: (u16, u16),
    pub wheel_remainder: f32,
    /// Button held for a drag: 0 left, 1 middle, 2 right. `None` is bare motion.
    pub pointer_button: Option<u8>,
}

pub struct Surfaces {
    inner: HashMap<SurfaceId, HostSurface>,
}

impl Surfaces {
    pub fn new() -> Self {
        Self {
            inner: HashMap::new(),
        }
    }

    pub fn insert(&mut self, host: HostSurface) {
        let id = host.surface.id();
        self.inner.insert(id, host);
    }

    pub fn remove(&mut self, id: SurfaceId) -> Option<HostSurface> {
        self.inner.remove(&id)
    }

    pub fn get_mut(&mut self, id: SurfaceId) -> Option<&mut HostSurface> {
        self.inner.get_mut(&id)
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut HostSurface> {
        self.inner.values_mut()
    }

    pub fn primary(&self) -> Option<&HostSurface> {
        self.inner.values().next()
    }

    pub fn primary_mut(&mut self) -> Option<&mut HostSurface> {
        self.inner.values_mut().next()
    }
}

impl Default for Surfaces {
    fn default() -> Self {
        Self::new()
    }
}
