//! A bar beside the pane. The thumb is painted in the compositor, so a
//! wheel can move it without committing the view. The grid is not an
//! `El::scroll`.

use rune::{effect, El, Phase, Tx};
use term_grid::SolidSlot;

use crate::model::Model;
use crate::theme;

/// Points. Reserved on every pane so history appearing does not resize the grid.
pub const WIDTH: f32 = 8.0;
/// The single-pane bar. It follows the selected tab, not a leaf mark.
pub const SHOWN_PANE: u32 = 0;
const MIN_THUMB: f32 = 24.0;
const SCROLL_BIT: u32 = 0x8000_0000;

pub struct Thumb {
    pub top: f32,
    pub height: f32,
}

/// `track`, `top`, and `height` share one unit. Offset 0 sits on the bottom.
pub fn thumb(track: f32, history: usize, rows: usize, offset: usize, min_thumb: f32) -> Thumb {
    if !(track > 0.0) || rows == 0 {
        return Thumb {
            top: 0.0,
            height: 0.0,
        };
    }
    if history == 0 {
        return Thumb {
            top: 0.0,
            height: track,
        };
    }
    let total = history + rows;
    let height = (track * rows as f32 / total as f32)
        .max(min_thumb)
        .min(track);
    let travel = (track - height).max(0.0);
    let ratio = (offset as f32 / history as f32).clamp(0.0, 1.0);
    Thumb {
        top: (1.0 - ratio) * travel,
        height,
    }
}

/// `pointer_y` and `grab` are in the same unit as `track`. `grab` is the
/// distance from the thumb's top to the pointer.
pub fn offset_at(
    track: f32,
    history: usize,
    rows: usize,
    pointer_y: f32,
    grab: f32,
    min_thumb: f32,
) -> usize {
    if history == 0 {
        return 0;
    }
    let height = thumb(track, history, rows, 0, min_thumb).height;
    let travel = (track - height).max(0.0);
    if travel <= 0.0 {
        return 0;
    }
    let top = (pointer_y - grab).clamp(0.0, travel);
    let ratio = 1.0 - top / travel;
    ((ratio * history as f32).round() as usize).min(history)
}

pub fn mark(pane: u32) -> u32 {
    pane | SCROLL_BIT
}

pub fn is_scroll(mark: u32) -> bool {
    mark & SCROLL_BIT != 0
}

fn pane_of(mark: u32) -> u32 {
    mark & !SCROLL_BIT
}

fn host_ref(model: &Model, pane: u32) -> Option<&crate::surfaces::HostSurface> {
    if pane == SHOWN_PANE {
        model.surfaces.primary()
    } else {
        model.surfaces.by_mark(pane)
    }
}

fn host_mut(model: &mut Model, pane: u32) -> Option<&mut crate::surfaces::HostSurface> {
    if pane == SHOWN_PANE {
        model.surfaces.primary_mut()
    } else {
        model.surfaces.by_mark_mut(pane)
    }
}

/// The track. Its thumb is not a child: the compositor paints that from
/// `display_offset`, so the view stays put while the wheel scrolls.
pub fn column(pane: u32) -> El {
    effect(mark(pane))
        .key(format!("scroll-{pane}"))
        .w(WIDTH)
        .h_full()
        .on_drag(move |model: &mut Model, drag, tx| {
            drag.data = pointer(model, pane, drag.phase, drag.y, drag.h, drag.data, tx);
        })
}

/// Returns the grab to keep for the rest of the gesture.
pub(crate) fn pointer(
    model: &mut Model,
    pane: u32,
    phase: Phase,
    y: f32,
    track: f32,
    grab: f32,
    tx: &mut Tx,
) -> f32 {
    let Some(host) = host_mut(model, pane) else {
        tx.live_only();
        return grab;
    };
    host.render.update();
    let history = host.surface.history_size();
    let rows = host.render.lines();
    let offset = host.render.display_offset();
    if history == 0 || rows == 0 || !(track > 0.0) {
        tx.live_only();
        return grab;
    }
    let geom = thumb(track, history, rows, offset, MIN_THUMB);
    let grab = if phase == Phase::Down {
        if y >= geom.top && y <= geom.top + geom.height {
            y - geom.top
        } else {
            geom.height * 0.5
        }
    } else {
        grab
    };
    let target = offset_at(track, history, rows, y, grab, MIN_THUMB);
    let delta = target as i32 - offset as i32;
    if delta != 0 {
        host.surface.scroll(delta);
        host.render.update();
    }
    tx.live_only();
    grab
}

pub struct Bars {
    track: SolidSlot,
    thumb: SolidSlot,
}

impl Bars {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        Self {
            track: SolidSlot::new(device, format),
            thumb: SolidSlot::new(device, format),
        }
    }

    pub fn prepare(&self, queue: &wgpu::Queue, srgb_blend: bool) {
        self.track
            .write_color(queue, premul(theme::TAB_BG, srgb_blend));
        self.thumb
            .write_color(queue, premul(theme::MUTED, srgb_blend));
    }

    pub fn draw(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        site: &rune::render::EffectSite<'_>,
        model: &Model,
    ) {
        let Some(host) = host_ref(model, pane_of(site.mark)) else {
            return;
        };
        let Some((x, y, w, h)) = scissor(site, site.instance.rect) else {
            return;
        };
        pass.set_scissor_rect(x, y, w, h);
        self.track.draw(pass);
        let history = host.surface.history_size();
        let rows = host.render.lines();
        let offset = host.render.display_offset();
        if history == 0 || rows == 0 {
            return;
        }
        let scale = if site.scale.is_finite() && site.scale > 0.0 {
            site.scale
        } else {
            1.0
        };
        let rect = site.instance.rect;
        let geom = thumb(rect[3], history, rows, offset, MIN_THUMB * scale);
        let thumb_rect = [rect[0], rect[1] + geom.top, rect[2], geom.height];
        let Some((x, y, w, h)) = scissor(site, thumb_rect) else {
            return;
        };
        pass.set_scissor_rect(x, y, w, h);
        self.thumb.draw(pass);
    }
}

fn premul(value: u32, srgb_blend: bool) -> [f32; 4] {
    let color = theme::linear(value);
    if srgb_blend {
        color.premul_srgb(1.0)
    } else {
        color.premul(1.0)
    }
}

/// `rect` is x, y, width, height in window device pixels.
fn scissor(site: &rune::render::EffectSite<'_>, rect: [f32; 4]) -> Option<(u32, u32, u32, u32)> {
    let clip = site.instance.clip;
    let x0 = rect[0].max(clip[0]);
    let y0 = rect[1].max(clip[1]);
    let x1 = (rect[0] + rect[2]).min(clip[2]);
    let y1 = (rect[1] + rect[3]).min(clip[3]);
    let edge = |v: f32, origin: f32, max: u32| (v - origin).clamp(0.0, max as f32) as u32;
    let (ox, oy) = (site.origin[0], site.origin[1]);
    let (sx0, sy0) = (
        edge(x0.floor(), ox, site.size[0]),
        edge(y0.floor(), oy, site.size[1]),
    );
    let (sx1, sy1) = (
        edge(x1.ceil(), ox, site.size[0]),
        edge(y1.ceil(), oy, site.size[1]),
    );
    if sx1 <= sx0 || sy1 <= sy0 {
        return None;
    }
    Some((sx0, sy0, sx1 - sx0, sy1 - sy0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_thumb_sits_on_the_bottom_at_the_live_screen() {
        let geom = thumb(100.0, 50, 25, 0, 10.0);
        assert!((geom.top + geom.height - 100.0).abs() < 1e-3);
    }

    #[test]
    fn the_thumb_sits_on_the_top_at_the_oldest_line() {
        let geom = thumb(100.0, 50, 25, 50, 10.0);
        assert!(geom.top.abs() < 1e-3);
    }

    #[test]
    fn no_history_fills_the_track() {
        let geom = thumb(100.0, 0, 25, 0, 10.0);
        assert_eq!(geom.top, 0.0);
        assert_eq!(geom.height, 100.0);
    }

    #[test]
    fn a_thumb_position_rounds_back_to_its_offset() {
        for offset in [0, 1, 10, 49, 50] {
            let geom = thumb(400.0, 50, 20, offset, 24.0);
            let back = offset_at(400.0, 50, 20, geom.top, 0.0, 24.0);
            assert_eq!(back, offset, "offset {offset} top {}", geom.top);
        }
    }
}

#[cfg(all(test, not(feature = "pty")))]
mod viewport {
    use super::*;
    use crate::chrome::PANE_MARK;
    use crate::delegate;
    use crate::model;

    fn host() -> Model {
        let mut model = model::init();
        delegate::setup(&mut model, std::sync::Arc::new(|| {}));
        let mut bytes = b"TOPLINE\r\n".to_vec();
        for _ in 0..40 {
            bytes.extend_from_slice(b"filler\r\n");
        }
        bytes.extend_from_slice(b"BOTLINE\r\n");
        model
            .surfaces
            .primary_mut()
            .unwrap()
            .surface
            .inject_output(&bytes);
        model
    }

    #[test]
    fn dragging_to_the_top_shows_history_and_hides_the_cursor() {
        let mut model = host();
        {
            let host = model.surfaces.primary_mut().unwrap();
            host.render.update();
            assert!(host.render.cursor_visible());
            assert!(
                !host.render.text_row(0).contains("TOPLINE"),
                "the first line should have left the screen: {:?}",
                host.render.text_row(0)
            );
        }
        let track = 400.0;
        let grab = pointer(
            &mut model,
            PANE_MARK,
            Phase::Down,
            track - 1.0,
            track,
            0.0,
            &mut Tx::default(),
        );
        pointer(
            &mut model,
            PANE_MARK,
            Phase::Move,
            0.0,
            track,
            grab,
            &mut Tx::default(),
        );
        let host = model.surfaces.by_mark(PANE_MARK).unwrap();
        assert!(host.render.display_offset() > 0);
        assert!(!host.render.cursor_visible());
        let top = host.render.text_row(0);
        assert!(top.contains("TOPLINE"), "{top:?}");
    }
}
