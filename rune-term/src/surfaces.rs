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
    /// A local selection is tracking the pointer. The program did not take the press.
    pub selecting: bool,
    pub scrollbar_hover: bool,
    pub scrollbar_active_until: f64,
    pub scrollbar_opacity: f32,
    pub scrollbar_fade_start: f64,
    pub scrollbar_fade_from: f32,
    pub hover_link: Option<crate::hyperlink::HoverLink>,
    pub(crate) hover_probe: Option<(usize, usize, Option<crate::hyperlink::HoverLink>)>,
}

pub struct Surfaces {
    inner: HashMap<SurfaceId, HostSurface>,
    /// The shell the pane follows. A new tab replaces it.
    shown: Option<SurfaceId>,
}

impl Surfaces {
    pub fn new() -> Self {
        Self {
            inner: HashMap::new(),
            shown: None,
        }
    }

    pub fn insert(&mut self, host: HostSurface) {
        let id = host.surface.id();
        self.inner.insert(id, host);
        if self.shown.is_none() {
            self.shown = Some(id);
        }
    }

    /// The pane, the keys, and the grid follow this shell.
    pub fn show(&mut self, id: SurfaceId) {
        if self.inner.contains_key(&id) {
            self.shown = Some(id);
        }
    }

    pub fn shown_id(&self) -> Option<SurfaceId> {
        self.shown.filter(|id| self.inner.contains_key(id))
    }

    pub fn remove(&mut self, id: SurfaceId) -> Option<HostSurface> {
        let host = self.inner.remove(&id)?;
        if self.shown == Some(id) {
            self.shown = None;
        }
        Some(host)
    }

    pub fn get(&self, id: SurfaceId) -> Option<&HostSurface> {
        self.inner.get(&id)
    }

    pub fn get_mut(&mut self, id: SurfaceId) -> Option<&mut HostSurface> {
        self.inner.get_mut(&id)
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut HostSurface> {
        self.inner.values_mut()
    }

    /// The selected tab's shell. The first pane, when nothing has selected one.
    pub fn primary(&self) -> Option<&HostSurface> {
        self.resolve().and_then(|id| self.inner.get(&id))
    }

    pub fn primary_mut(&mut self) -> Option<&mut HostSurface> {
        let id = self.resolve()?;
        self.inner.get_mut(&id)
    }

    fn resolve(&self) -> Option<SurfaceId> {
        self.shown_id()
            .or_else(|| {
                self.inner
                    .iter()
                    .find(|(_, host)| host.mark == crate::chrome::PANE_MARK)
                    .map(|(id, _)| *id)
            })
            .or_else(|| self.inner.keys().next().copied())
    }

    pub(crate) fn by_mark(&self, mark: u32) -> Option<&HostSurface> {
        self.inner.values().find(|host| host.mark == mark)
    }

    pub(crate) fn by_mark_mut(&mut self, mark: u32) -> Option<&mut HostSurface> {
        let id = self
            .inner
            .iter()
            .find(|(_, host)| host.mark == mark)
            .map(|(id, _)| *id)?;
        self.inner.get_mut(&id)
    }
}

impl Default for Surfaces {
    fn default() -> Self {
        Self::new()
    }
}
