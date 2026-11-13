//! Scrollback geometry and scrollbar state shared by the terminal component.

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
