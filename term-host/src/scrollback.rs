//! Scrollback geometry and scrollbar state/behavior shared by the terminal component.

use crate::surfaces::HostSurface;

pub const MIN_THUMB: f32 = 24.0;
pub const HIDE_DELAY_SECS: f64 = 0.4;
pub const FADE_DUR_SECS: f64 = 0.18;

#[derive(Clone, Copy, Debug)]
pub struct ScrollbarState {
    pub hover: bool,
    pub active_until: f64,
    pub opacity: f32,
    pub fade_start: f64,
    pub fade_from: f32,
}

impl Default for ScrollbarState {
    fn default() -> Self {
        Self {
            hover: false,
            active_until: 0.0,
            opacity: 0.0,
            fade_start: 0.0,
            fade_from: 0.0,
        }
    }
}

impl ScrollbarState {
    pub fn note_activity(&mut self) {
        self.active_until = f64::INFINITY;
        self.opacity = 1.0;
        self.fade_start = 0.0;
        self.fade_from = 1.0;
    }

    pub fn show_immediate(&mut self) {
        self.active_until = 0.0;
        self.opacity = 1.0;
        self.fade_start = 0.0;
        self.fade_from = 1.0;
    }
}

pub struct Thumb {
    pub top: f32,
    pub height: f32,
}

/// `track`, `top`, and `height` share one unit. Offset 0 sits on the bottom.
pub fn thumb(track: f32, history: usize, rows: usize, offset: usize, min_thumb: f32) -> Thumb {
    if track.partial_cmp(&0.0) != Some(std::cmp::Ordering::Greater) || rows == 0 {
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

/// Refresh scrollbar visibility/opacity for one host. Returns true if animation wake is needed.
pub fn refresh_host(host: &mut HostSurface, now: f64) -> bool {
    let scrollable = host.surface.history_size() > 0 || host.render.display_offset() > 0;
    let sb = &mut host.scrollbar;
    if !scrollable {
        return fade_opacity(sb, now);
    }
    if sb.hover || host.render.display_offset() > 0 {
        sb.show_immediate();
        return false;
    }
    if sb.active_until.is_infinite() {
        sb.active_until = now + HIDE_DELAY_SECS;
        sb.opacity = 1.0;
        sb.fade_start = 0.0;
        return true;
    }
    if sb.active_until > now {
        sb.opacity = 1.0;
        sb.fade_start = 0.0;
        return true;
    }
    sb.active_until = 0.0;
    fade_opacity(sb, now)
}

fn fade_opacity(sb: &mut ScrollbarState, now: f64) -> bool {
    if sb.opacity <= 0.0 {
        sb.opacity = 0.0;
        sb.fade_start = 0.0;
        return false;
    }
    if sb.fade_start == 0.0 {
        sb.fade_start = now;
        sb.fade_from = sb.opacity;
    }
    let t = ((now - sb.fade_start) / FADE_DUR_SECS).clamp(0.0, 1.0);
    let inv = 1.0 - t;
    sb.opacity = sb.fade_from * (inv * inv * inv) as f32;
    if sb.opacity <= 0.0005 {
        sb.opacity = 0.0;
        sb.fade_start = 0.0;
        false
    } else {
        true
    }
}

/// Apply a scrollbar drag/pointer interaction to a host. Returns the grab to keep.
pub fn pointer_scroll(
    host: &mut HostSurface,
    phase_down: bool,
    y: f32,
    track: f32,
    grab: f32,
) -> f32 {
    host.scrollbar.note_activity();
    host.render.update();
    let history = host.surface.history_size();
    let rows = host.render.lines();
    let offset = host.render.display_offset();
    if history == 0 || rows == 0 || track.partial_cmp(&0.0) != Some(std::cmp::Ordering::Greater) {
        return grab;
    }
    let geom = thumb(track, history, rows, offset, MIN_THUMB);
    let grab = if phase_down {
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
    grab
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
