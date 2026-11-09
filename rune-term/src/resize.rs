//! Frozen `apply` signature. The body floors the slot into whole cells and
//! does not write that count back into the slot. Every layout is `live_only`.

/// What the caller, which has `Tx`, must do. `apply` does not take `Tx`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GridMetrics {
    pub cols: u16,
    pub rows: u16,
    pub cell_px: (u16, u16),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ResizeEffect {
    pub cols: u16,
    pub rows: u16,
    pub pixel_width: u16,
    pub pixel_height: u16,
    pub cell_px: (u16, u16),
    /// Caller calls `Surface::resize` and stores true in `HostSurface::resized`.
    pub resize_surface: bool,
    /// Caller calls `Tx::live_only`. Independent of `resize_surface`.
    pub live_only: bool,
    /// `Some` only for `ResizeRequest::Font`. Caller calls
    /// `Tx::resize_window_instant`. Plain DPI and layout leave this `None`.
    pub window_points: Option<(f32, f32)>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ResizeRequest {
    Layout {
        slot_points: (f32, f32),
        scale: f32,
        cell_points: (f32, f32),
        current: GridMetrics,
    },
    Font {
        chrome_points: (f32, f32),
        cols: u16,
        rows: u16,
        cell_points: (f32, f32),
        scale: f32,
    },
    #[cfg_attr(not(test), allow(dead_code))]
    PlainDpi {
        slot_points: (f32, f32),
        scale: f32,
        cell_points: (f32, f32),
        current: GridMetrics,
    },
}

/// Rio's default content margin, in points, on every side. It sits outside
/// the cell grid, so the winsize stays `cols * cell_px`.
pub const CONTENT_MARGIN_PT: f32 = 2.0;

/// Device pixels of one margin edge. `round(2pt * scale)`.
pub fn margin_px(scale: f32) -> u16 {
    cell_px(CONTENT_MARGIN_PT, scale)
}

/// Rounded cell pitch. Raster size and winsize factor.
pub fn cell_px(cell_points: f32, scale: f32) -> u16 {
    let px = (cell_points * scale).round();
    if !px.is_finite() || px < 1.0 {
        1
    } else if px > u16::MAX as f32 {
        u16::MAX
    } else {
        px as u16
    }
}

/// Device pixels of a slot edge. Matches `frame::snap`: `(points * scale).round()`.
pub fn slot_device_px(slot_points: f32, scale: f32) -> u32 {
    let px = (slot_points * scale).round();
    if !px.is_finite() || px <= 0.0 {
        0
    } else if px >= u32::MAX as f32 {
        u32::MAX
    } else {
        px as u32
    }
}

/// How many cells fit in `device` pixels. At least one. `count * pitch` stays
/// inside `u16`.
fn fitted_device(device: u32, pitch: u32) -> u16 {
    if pitch == 0 {
        return 1;
    }
    let mut n = device / pitch;
    if n == 0 {
        n = 1;
    }
    if n > u32::from(u16::MAX) {
        n = u32::from(u16::MAX);
    }
    while n > 1 && n.saturating_mul(pitch) > u32::from(u16::MAX) {
        n -= 1;
    }
    n as u16
}

/// Point size whose snapped device size is `cols * cell_px`.
pub fn content_points(cols: u16, cell_points: f32, scale: f32) -> f32 {
    let pitch = f32::from(cell_px(cell_points, scale));
    (f32::from(cols) * pitch) / scale
}

pub fn apply(request: ResizeRequest) -> ResizeEffect {
    match request {
        ResizeRequest::Layout {
            slot_points,
            scale,
            cell_points,
            current,
        }
        | ResizeRequest::PlainDpi {
            slot_points,
            scale,
            cell_points,
            current,
        } => fit(slot_points, scale, cell_points, current),
        ResizeRequest::Font {
            chrome_points,
            cols,
            rows,
            cell_points,
            scale,
        } => {
            let pitch = (cell_px(cell_points.0, scale), cell_px(cell_points.1, scale));
            ResizeEffect {
                cols,
                rows,
                pixel_width: cols.saturating_mul(pitch.0),
                pixel_height: rows.saturating_mul(pitch.1),
                cell_px: pitch,
                resize_surface: false,
                live_only: false,
                window_points: Some((
                    chrome_points.0
                        + content_points(cols, cell_points.0, scale)
                        + CONTENT_MARGIN_PT * 2.0,
                    chrome_points.1
                        + content_points(rows, cell_points.1, scale)
                        + CONTENT_MARGIN_PT * 2.0,
                )),
            }
        }
    }
}

fn fit(
    slot_points: (f32, f32),
    scale: f32,
    cell_points: (f32, f32),
    current: GridMetrics,
) -> ResizeEffect {
    let cell_px = (cell_px(cell_points.0, scale), cell_px(cell_points.1, scale));
    let margin = u32::from(margin_px(scale)).saturating_mul(2);
    let cols = fitted_device(
        slot_device_px(slot_points.0, scale).saturating_sub(margin),
        u32::from(cell_px.0),
    );
    let rows = fitted_device(
        slot_device_px(slot_points.1, scale).saturating_sub(margin),
        u32::from(cell_px.1),
    );
    let changed = current.cols != cols || current.rows != rows || current.cell_px != cell_px;
    ResizeEffect {
        cols,
        rows,
        pixel_width: cols.saturating_mul(cell_px.0),
        pixel_height: rows.saturating_mul(cell_px.1),
        cell_px,
        resize_surface: changed,
        live_only: true,
        window_points: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn zero() -> GridMetrics {
        GridMetrics {
            cols: 0,
            rows: 0,
            cell_px: (0, 0),
        }
    }

    #[test]
    fn a_wide_slot_floors_to_the_cell_pitch_and_stays_there() {
        let slot = (80.0 * 7.4, 24.0 * 15.0);
        let first = apply(ResizeRequest::Layout {
            slot_points: slot,
            scale: 2.0,
            cell_points: (7.4, 15.0),
            current: zero(),
        });
        assert!(first.resize_surface);
        assert!(first.live_only);
        assert_eq!(first.window_points, None);
        assert_eq!(first.cell_px, (15, 30));
        assert_eq!(first.cols, 78);
        assert_eq!(first.pixel_width, 1170);
        assert_eq!(
            slot_device_px(slot.0, 2.0) - u32::from(first.pixel_width),
            14
        );
        assert!(u32::from(first.pixel_width) <= slot_device_px(slot.0, 2.0));
        assert!(u32::from(first.pixel_height) <= slot_device_px(slot.1, 2.0));

        let again = apply(ResizeRequest::Layout {
            slot_points: slot,
            scale: 2.0,
            cell_points: (7.4, 15.0),
            current: GridMetrics {
                cols: first.cols,
                rows: first.rows,
                cell_px: first.cell_px,
            },
        });
        assert!(!again.resize_surface);
        assert!(again.live_only);
        assert_eq!(again.cols, 78);
        assert_eq!(again.cell_px, (15, 30));
    }

    #[test]
    fn padding_is_the_leftover_device_pixels() {
        let effect = apply(ResizeRequest::Layout {
            slot_points: (1005.0, 710.0),
            scale: 1.0,
            cell_points: (10.0, 20.0),
            current: zero(),
        });
        assert_eq!((effect.cols, effect.rows), (100, 35));
        assert_eq!((effect.pixel_width, effect.pixel_height), (1000, 700));
        assert!(u32::from(effect.pixel_width) <= 1005);
        assert!(u32::from(effect.pixel_height) <= 710);
        assert!(effect.live_only);
    }

    #[test]
    fn a_font_change_snaps_the_window_to_whole_cells() {
        let effect = apply(ResizeRequest::Font {
            chrome_points: (10.0, 20.0),
            cols: 80,
            rows: 24,
            cell_points: (7.4, 15.0),
            scale: 2.0,
        });
        assert!(!effect.resize_surface);
        assert!(!effect.live_only);
        assert_eq!(
            effect.window_points,
            Some((10.0 + 600.0 + 4.0, 20.0 + 360.0 + 4.0))
        );

        let content = (
            effect.window_points.unwrap().0 - 10.0,
            effect.window_points.unwrap().1 - 20.0,
        );
        let follow = apply(ResizeRequest::Layout {
            slot_points: content,
            scale: 2.0,
            cell_points: (7.4, 15.0),
            current: zero(),
        });
        assert_eq!(follow.cols, 80);
        assert_eq!(follow.pixel_width, 1200);
        assert!(follow.resize_surface);

        let settled = apply(ResizeRequest::Layout {
            slot_points: content,
            scale: 2.0,
            cell_points: (7.4, 15.0),
            current: GridMetrics {
                cols: follow.cols,
                rows: follow.rows,
                cell_px: follow.cell_px,
            },
        });
        assert_eq!(settled.cols, 80);
        assert!(!settled.resize_surface);
        assert!(settled.live_only);
    }

    #[test]
    fn the_margin_sits_outside_the_winsize() {
        let effect = apply(ResizeRequest::Layout {
            slot_points: (800.0, 480.0),
            scale: 2.0,
            cell_points: (10.0, 20.0),
            current: zero(),
        });
        assert_eq!(effect.cell_px, (20, 40));
        assert_eq!((effect.cols, effect.rows), (79, 23));
        assert_eq!(effect.pixel_width, 79 * 20);
        assert_eq!(effect.pixel_height, 23 * 40);
        assert!(u32::from(effect.pixel_width) + 8 <= slot_device_px(800.0, 2.0));
        assert!(effect.live_only);
        assert_eq!(effect.window_points, None);
    }

    #[test]
    fn plain_dpi_refits_the_same_point_slot() {
        let effect = apply(ResizeRequest::PlainDpi {
            slot_points: (592.0, 360.0),
            scale: 1.0,
            cell_points: (7.4, 15.0),
            current: GridMetrics {
                cols: 78,
                rows: 24,
                cell_px: (15, 30),
            },
        });
        assert_eq!(effect.window_points, None);
        assert!(effect.live_only);
        assert_eq!(effect.cell_px, (7, 15));
        assert!(effect.resize_surface);
    }
}
