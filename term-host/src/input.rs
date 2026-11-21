//! Terminal input mapping helpers.

use term_input::Pad;

use crate::surfaces::HostSurface;

/// Pointer phase, independent of any window toolkit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Down,
    Move,
    Up,
}

pub fn cell_under(host: &HostSurface, x: f32, y: f32, scale: f32) -> Option<(u16, u16)> {
    term_input::cell_at(
        x,
        y,
        drawn_pitch(host.cell_px.0, scale),
        drawn_pitch(host.cell_px.1, scale),
        host.cols,
        host.rows,
        Pad::default(),
    )
}

/// A miss still scrolls. The cell is the last one on that side, or 0 when
/// the grid has no cells.
pub fn cell_for_wheel(host: &HostSurface, x: f32, y: f32, scale: f32) -> (u16, u16) {
    if let Some(cell) = cell_under(host, x, y, scale) {
        return cell;
    }
    (
        edge_cell(x, drawn_pitch(host.cell_px.0, scale), host.cols),
        edge_cell(y, drawn_pitch(host.cell_px.1, scale), host.rows),
    )
}

pub fn edge_cell(local: f32, pitch: f32, count: u16) -> u16 {
    if count == 0 || pitch.partial_cmp(&0.0) != Some(std::cmp::Ordering::Greater) || local < 0.0 {
        0
    } else {
        count - 1
    }
}

/// The first half of the cell is [`librio::Side::Left`]. A backwards drag
/// can include column 0 because the side follows the pointer.
pub fn side_at(x: f32, col: u16, pitch: f32) -> librio::Side {
    if pitch.partial_cmp(&0.0) != Some(std::cmp::Ordering::Greater) {
        return librio::Side::Left;
    }
    let into = x - f32::from(col) * pitch;
    if into >= pitch * 0.5 {
        librio::Side::Right
    } else {
        librio::Side::Left
    }
}

pub fn drawn_pitch(cell_px: u16, scale: f32) -> f32 {
    if !(scale.is_finite() && scale > 0.0) {
        return 0.0;
    }
    f32::from(cell_px) / scale
}
/// `Key::Command` is the logical form of a platform chord. The window also
/// sets `Tx::physical_key`; this covers the path that only has the logical key.
pub fn command_code(c: char) -> term_input::PhysicalKeyCode {
    use term_input::PhysicalKeyCode as C;
    match c.to_ascii_lowercase() {
        'a' => C::KeyA,
        'c' => C::KeyC,
        'f' => C::KeyF,
        'n' => C::KeyN,
        't' => C::KeyT,
        'v' => C::KeyV,
        'w' => C::KeyW,
        '1' => C::Digit1,
        '2' => C::Digit2,
        '3' => C::Digit3,
        '4' => C::Digit4,
        '5' => C::Digit5,
        '6' => C::Digit6,
        '7' => C::Digit7,
        '8' => C::Digit8,
        '9' => C::Digit9,
        _ => C::Unknown,
    }
}

pub fn tab_digit(code: term_input::PhysicalKeyCode) -> Option<u8> {
    use term_input::PhysicalKeyCode::*;
    Some(match code {
        Digit1 => 1,
        Digit2 => 2,
        Digit3 => 3,
        Digit4 => 4,
        Digit5 => 5,
        Digit6 => 6,
        Digit7 => 7,
        Digit8 => 8,
        Digit9 => 9,
        _ => return None,
    })
}
