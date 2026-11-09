//! The focused pane maps physical keys, the wheel, and the pointer onto the shell.
//! App chords return false, except copy, paste, search, and tabs, which this shell handles.

use std::time::Duration;

use librio::Modifiers;
use rune::{
    Drag, Key, KeyEvent, Mods, Phase, PhysicalInput, PhysicalKeyCode, PointerButton, PointerEvent,
    Tx, WheelEvent,
};

use crate::model::Model;
use crate::surfaces::HostSurface;

#[cfg_attr(not(test), allow(dead_code))]
pub fn on_physical(model: &mut Model, event: &PhysicalInput, tx: &mut Tx) -> bool {
    on_physical_mark(model, crate::chrome::PANE_MARK, event, tx)
}

pub fn on_physical_mark(model: &mut Model, mark: u32, event: &PhysicalInput, tx: &mut Tx) -> bool {
    match event {
        PhysicalInput::Key {
            key,
            mods,
            pressed,
            repeat,
            text,
            consumed_mods,
        } => deliver(
            model,
            mark,
            code(*key),
            mods_of(*mods),
            *pressed,
            *repeat,
            text.as_deref(),
            mods_of(*consumed_mods),
            tx,
        ),
        PhysicalInput::FocusLost => {
            if let Some(host) = target_host(model, mark) {
                host.wheel_remainder = 0.0;
                host.pointer_button = None;
                host.selecting = false;
                host.hover_link = None;
                host.hover_probe = None;
            }
            false
        }
        PhysicalInput::Modifiers(_) => false,
    }
}

/// Wheel points become fractional lines of the drawn row. A zero-line event
/// still returns true, so an ancestor scroll does not also move.
pub fn on_wheel_mark(
    model: &mut Model,
    mark: u32,
    event: WheelEvent,
    scale: f32,
    tx: &mut Tx,
) -> bool {
    let Some(host) = model.surfaces.by_mark_mut(mark) else {
        return false;
    };
    let pitch_y = drawn_pitch(host.cell_px.1, scale);
    if !(pitch_y.is_finite() && pitch_y > 0.0) {
        tx.live_only();
        return true;
    }
    let points = if event.dy != 0.0 {
        event.dy
    } else if !cfg!(target_os = "macos") && event.mods.shift && event.dx != 0.0 {
        event.dx
    } else {
        0.0
    };
    host.wheel_remainder += points / pitch_y;
    let lines = host.wheel_remainder.trunc() as i32;
    host.wheel_remainder -= lines as f32;
    if lines != 0 {
        let (col, row) = cell_for_wheel(host, event.x, event.y, scale);
        let mods = modifiers(event.mods);
        host.surface.scroll_wheel(lines, col, row, mods);
        crate::scrollback::note_activity(host, tx);
    }
    tx.live_only();
    true
}

pub fn on_drag_mark(model: &mut Model, mark: u32, drag: &mut Drag, scale: f32, tx: &mut Tx) {
    let Some(button) = button_code(tx.button()) else {
        return;
    };
    gesture(
        model,
        mark,
        drag.phase,
        drag.x,
        drag.y,
        drag.clicks,
        button,
        modifiers(tx.mods()),
        scale,
        tx,
    );
}

/// The program takes a click when `mouse_button` returns true. Otherwise the
/// left button starts a local selection. Shift makes that return false.
fn gesture(
    model: &mut Model,
    mark: u32,
    phase: Phase,
    x: f32,
    y: f32,
    clicks: u32,
    button: u8,
    mods: Modifiers,
    scale: f32,
    tx: &mut Tx,
) {
    if phase == Phase::Down && button == 0 && crate::hyperlink::open_mods(mods) {
        enum LinkAction {
            None,
            Opened,
            Confirm(String),
        }
        let action = {
            let Some(host) = model.surfaces.by_mark_mut(mark) else {
                return;
            };
            let cell = cell_under(host, x, y, scale);
            match cell.and_then(|(col, row)| {
                crate::hyperlink::resolve(host, usize::from(row), usize::from(col))
            }) {
                Some(link) => {
                    host.surface.selection_clear();
                    host.pointer_button = None;
                    host.selecting = false;
                    host.hover_link = None;
                    host.hover_probe = None;
                    if crate::hyperlink::open_decision(&link.uri)
                        == crate::hyperlink::OpenDecision::WarnAndOpen
                    {
                        LinkAction::Confirm(link.uri)
                    } else {
                        let _ = crate::hyperlink::open_uri(&link.uri);
                        LinkAction::Opened
                    }
                }
                None => LinkAction::None,
            }
        };
        match action {
            LinkAction::Opened => {
                tx.wake_after(Duration::ZERO);
                return;
            }
            LinkAction::Confirm(uri) => {
                model.pending_link_open = Some(uri);
                tx.wake_after(Duration::ZERO);
                return;
            }
            LinkAction::None => {}
        }
    }
    let Some(host) = model.surfaces.by_mark_mut(mark) else {
        return;
    };
    let cell = cell_under(host, x, y, scale);
    match phase {
        Phase::Down => {
            host.pointer_button = Some(button);
            let taken = match cell {
                Some((col, row)) => host.surface.mouse_button(col, row, button, true, mods),
                None => false,
            };
            if button == 0 && !taken {
                if let Some((col, row)) = cell {
                    host.surface.selection_begin(
                        i32::from(row),
                        usize::from(col),
                        crate::selection::kind(clicks),
                        side_at(x, col, drawn_pitch(host.cell_px.0, scale)),
                    );
                    host.selecting = true;
                } else {
                    host.selecting = false;
                }
            } else {
                if taken {
                    host.surface.selection_clear();
                }
                host.selecting = false;
            }
        }
        Phase::Move => {
            if host.selecting {
                if let Some((col, row)) = cell {
                    host.surface.selection_update(
                        i32::from(row),
                        usize::from(col),
                        side_at(x, col, drawn_pitch(host.cell_px.0, scale)),
                    );
                }
            } else if let (Some(held), Some((col, row))) = (host.pointer_button, cell) {
                host.surface.mouse_motion(col, row, held, mods);
            }
        }
        Phase::Up => {
            let selecting = host.selecting;
            host.pointer_button = None;
            host.selecting = false;
            if selecting {
                if let Some((col, row)) = cell {
                    host.surface.selection_update(
                        i32::from(row),
                        usize::from(col),
                        side_at(x, col, drawn_pitch(host.cell_px.0, scale)),
                    );
                }
            } else if let Some((col, row)) = cell {
                host.surface.mouse_button(col, row, button, false, mods);
            }
        }
    }
    tx.live_only();
}

/// Bare motion is button 3. A drag already reported the held button.
pub fn on_pointer_move_mark(
    model: &mut Model,
    mark: u32,
    point: PointerEvent,
    scale: f32,
    tx: &mut Tx,
) {
    let Some(host) = model.surfaces.by_mark_mut(mark) else {
        return;
    };
    if host.pointer_button.is_none() {
        let mods = modifiers(tx.mods());
        let cell = cell_under(host, point.x, point.y, scale);
        let hover = match cell {
            Some((col, row)) => {
                let (line, column) = (usize::from(row), usize::from(col));
                if host.hover_probe.as_ref().map(|(l, c, _)| (*l, *c)) == Some((line, column)) {
                    host.hover_probe
                        .as_ref()
                        .and_then(|(_, _, link)| link.clone())
                } else {
                    let link = crate::hyperlink::resolve(host, line, column);
                    host.hover_probe = Some((line, column, link.clone()));
                    link
                }
            }
            None => {
                host.hover_probe = None;
                None
            }
        };
        let changed = host.hover_link != hover;
        if changed {
            host.hover_link = hover;
        }
        if let Some((col, row)) = cell {
            host.surface.mouse_motion(col, row, 3, mods);
        }
        if changed {
            tx.wake_after(Duration::ZERO);
        } else {
            tx.live_only();
        }
    } else {
        tx.live_only();
    }
}

pub fn on_secondary_mark(
    model: &mut Model,
    mark: u32,
    point: PointerEvent,
    scale: f32,
    tx: &mut Tx,
) {
    let Some(host) = model.surfaces.by_mark_mut(mark) else {
        return;
    };
    let Some((col, row)) = cell_under(host, point.x, point.y, scale) else {
        return;
    };
    let mods = modifiers(tx.mods());
    // Rune delivers the secondary button only on release.
    host.surface.mouse_button(col, row, 2, true, mods);
    host.surface.mouse_button(col, row, 2, false, mods);
    tx.live_only();
}

fn cell_under(host: &HostSurface, x: f32, y: f32, scale: f32) -> Option<(u16, u16)> {
    term_input::cell_at(
        x,
        y,
        drawn_pitch(host.cell_px.0, scale),
        drawn_pitch(host.cell_px.1, scale),
        host.cols,
        host.rows,
        term_input::Pad::default(),
    )
}

/// A miss still scrolls. The cell is the last one on that side, or 0 when
/// the grid has no cells.
fn cell_for_wheel(host: &HostSurface, x: f32, y: f32, scale: f32) -> (u16, u16) {
    if let Some(cell) = cell_under(host, x, y, scale) {
        return cell;
    }
    (
        edge_cell(x, drawn_pitch(host.cell_px.0, scale), host.cols),
        edge_cell(y, drawn_pitch(host.cell_px.1, scale), host.rows),
    )
}

fn edge_cell(local: f32, pitch: f32, count: u16) -> u16 {
    if count == 0 || !(pitch > 0.0) || local < 0.0 {
        0
    } else {
        count - 1
    }
}

/// The first half of the cell is [`librio::Side::Left`]. A backwards drag
/// can include column 0 because the side follows the pointer.
fn side_at(x: f32, col: u16, pitch: f32) -> librio::Side {
    if !(pitch > 0.0) {
        return librio::Side::Left;
    }
    let into = x - f32::from(col) * pitch;
    if into >= pitch * 0.5 {
        librio::Side::Right
    } else {
        librio::Side::Left
    }
}

fn drawn_pitch(cell_px: u16, scale: f32) -> f32 {
    if !(scale.is_finite() && scale > 0.0) {
        return 0.0;
    }
    f32::from(cell_px) / scale
}

fn button_code(button: PointerButton) -> Option<u8> {
    match button {
        PointerButton::Primary => Some(0),
        PointerButton::Middle => Some(1),
        PointerButton::Secondary => Some(2),
        _ => None,
    }
}

fn modifiers(mods: Mods) -> Modifiers {
    let mut out = Modifiers::empty();
    if mods.shift {
        out |= Modifiers::SHIFT;
    }
    if mods.ctrl {
        out |= Modifiers::CTRL;
    }
    if mods.alt {
        out |= Modifiers::ALT;
    }
    if mods.logo {
        out |= Modifiers::SUPER;
    }
    out
}

/// Keys that arrive before a click focuses the pane. The window still fills
/// `Tx::physical_key` and `Tx::key_base`. Once the pane is focused,
/// `on_physical` is the only path, so this does not write the key twice.
pub fn on_key(model: &mut Model, event: &KeyEvent, tx: &mut Tx) -> bool {
    if crate::search::is_chord(&event.key, event.mods) {
        crate::search::open(model);
        return true;
    }
    if tx.focused().is_some() {
        return false;
    }
    let text = match &event.key {
        Key::Text(text) => Some(text.clone()),
        Key::Command(c) => Some(c.to_string()),
        _ => None,
    };
    let code = tx
        .physical_key()
        .map(code)
        .unwrap_or_else(|| logical_code(&event.key));
    let mark = model
        .surfaces
        .primary()
        .map(|host| host.mark)
        .unwrap_or(crate::chrome::PANE_MARK);
    deliver(
        model,
        mark,
        code,
        mods_of(event.mods),
        true,
        tx.key_repeat(),
        text.as_deref(),
        mods_of(consumed_logical(event.mods, text.as_deref(), tx.key_base())),
        tx,
    )
}

fn target_host(model: &mut Model, mark: u32) -> Option<&mut HostSurface> {
    if model.surfaces.by_mark(mark).is_some() {
        model.surfaces.by_mark_mut(mark)
    } else {
        model.surfaces.primary_mut()
    }
}

fn deliver(
    model: &mut Model,
    mark: u32,
    code: term_input::PhysicalKeyCode,
    mods: term_input::Mods,
    pressed: bool,
    repeat: bool,
    text: Option<&str>,
    consumed: term_input::Mods,
    tx: &mut Tx,
) -> bool {
    if term_input::is_app_chord(code, mods) {
        // A Ctrl+Tab release is not a tab character. Other chord releases are ignored.
        if !pressed {
            return code == term_input::PhysicalKeyCode::Tab;
        }
        if code == term_input::PhysicalKeyCode::KeyC {
            crate::selection::copy(model, tx);
            return true;
        }
        if code == term_input::PhysicalKeyCode::KeyF {
            crate::search::open(model);
            return true;
        }
        if code == term_input::PhysicalKeyCode::Comma {
            model.settings_open = true;
            return true;
        }
        if code == term_input::PhysicalKeyCode::KeyV {
            tx.request_paste();
            return true;
        }
        if code == term_input::PhysicalKeyCode::KeyT {
            crate::tabs::open(model);
            crate::tabs::reveal(model, tx);
            return true;
        }
        if code == term_input::PhysicalKeyCode::KeyW {
            crate::tabs::close_active(model, tx);
            crate::tabs::reveal(model, tx);
            return true;
        }
        if let Some(digit) = tab_digit(code) {
            crate::tabs::select_digit(model, digit);
            crate::tabs::reveal(model, tx);
            return true;
        }
        if code == term_input::PhysicalKeyCode::Tab {
            let delta = if mods.shift { -1 } else { 1 };
            crate::tabs::cycle(model, delta);
            crate::tabs::reveal(model, tx);
            return true;
        }
        return false;
    }
    if let Some(sign) = term_input::scroll_chord(code, mods) {
        if pressed {
            if let Some(host) = target_host(model, mark) {
                host.surface
                    .scroll(sign.saturating_mul(i32::from(host.rows)));
            }
            tx.live_only();
        }
        return true;
    }
    let mapped = term_input::map_key(code, mods, pressed, repeat, text, consumed);
    if mapped.key.is_none() && mapped.text.is_none() {
        return false;
    }
    let Some(host) = target_host(model, mark) else {
        return false;
    };
    let wrote = host.surface.key(&mapped);
    if wrote {
        tx.wake_after(Duration::ZERO);
    }
    wrote
}

/// Same rules as the window's `consumed_mods`: alt and shift are spent when
/// the platform text differs from the unmodified key. Ctrl stays, so
/// `encode` can apply `ctrl_byte`. `logo` is never spent.
fn consumed_logical(mods: Mods, text: Option<&str>, base: Option<char>) -> Mods {
    let unmodified = base.map(|c| c.to_string());
    let differs = match (text, unmodified.as_deref()) {
        (Some(text), Some(base)) => text != base,
        (Some(_), None) => true,
        _ => false,
    };
    let mut consumed = Mods::default();
    if cfg!(target_os = "windows") && mods.ctrl && mods.alt && text.is_some() {
        consumed.ctrl = true;
        consumed.alt = true;
        return consumed;
    }
    if mods.alt && differs {
        consumed.alt = true;
    }
    if mods.shift && !mods.ctrl && differs {
        consumed.shift = true;
    }
    consumed
}

fn logical_code(key: &Key) -> term_input::PhysicalKeyCode {
    use term_input::PhysicalKeyCode as C;
    match key {
        Key::Enter => C::Enter,
        Key::Backspace => C::Backspace,
        Key::Delete => C::Delete,
        Key::Left => C::ArrowLeft,
        Key::Right => C::ArrowRight,
        Key::Up => C::ArrowUp,
        Key::Down => C::ArrowDown,
        Key::Home => C::Home,
        Key::End => C::End,
        Key::PageUp => C::PageUp,
        Key::PageDown => C::PageDown,
        Key::Insert => C::Insert,
        Key::Escape => C::Escape,
        Key::Tab => C::Tab,
        Key::Command(c) => command_code(*c),
        Key::Function(1) => C::F1,
        Key::Function(2) => C::F2,
        Key::Function(3) => C::F3,
        Key::Function(4) => C::F4,
        Key::Function(5) => C::F5,
        Key::Function(6) => C::F6,
        Key::Function(7) => C::F7,
        Key::Function(8) => C::F8,
        Key::Function(9) => C::F9,
        Key::Function(10) => C::F10,
        Key::Function(11) => C::F11,
        Key::Function(12) => C::F12,
        Key::Function(13) => C::F13,
        Key::Function(14) => C::F14,
        Key::Function(15) => C::F15,
        Key::Function(16) => C::F16,
        Key::Function(17) => C::F17,
        Key::Function(18) => C::F18,
        Key::Function(19) => C::F19,
        Key::Function(20) => C::F20,
        Key::Function(21) => C::F21,
        Key::Function(22) => C::F22,
        Key::Function(23) => C::F23,
        Key::Function(24) => C::F24,
        _ => C::Unknown,
    }
}

/// `Key::Command` is the logical form of a platform chord. The window also
/// sets `Tx::physical_key`; this covers the path that only has the logical key.
fn command_code(c: char) -> term_input::PhysicalKeyCode {
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

fn tab_digit(code: term_input::PhysicalKeyCode) -> Option<u8> {
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

fn mods_of(mods: Mods) -> term_input::Mods {
    term_input::Mods {
        shift: mods.shift,
        cmd: mods.cmd,
        alt: mods.alt,
        ctrl: mods.ctrl,
        logo: mods.logo,
    }
}

fn code(key: PhysicalKeyCode) -> term_input::PhysicalKeyCode {
    use rune::PhysicalKeyCode as W;
    use term_input::PhysicalKeyCode as C;
    match key {
        W::KeyA => C::KeyA,
        W::KeyB => C::KeyB,
        W::KeyC => C::KeyC,
        W::KeyD => C::KeyD,
        W::KeyE => C::KeyE,
        W::KeyF => C::KeyF,
        W::KeyG => C::KeyG,
        W::KeyH => C::KeyH,
        W::KeyI => C::KeyI,
        W::KeyJ => C::KeyJ,
        W::KeyK => C::KeyK,
        W::KeyL => C::KeyL,
        W::KeyM => C::KeyM,
        W::KeyN => C::KeyN,
        W::KeyO => C::KeyO,
        W::KeyP => C::KeyP,
        W::KeyQ => C::KeyQ,
        W::KeyR => C::KeyR,
        W::KeyS => C::KeyS,
        W::KeyT => C::KeyT,
        W::KeyU => C::KeyU,
        W::KeyV => C::KeyV,
        W::KeyW => C::KeyW,
        W::KeyX => C::KeyX,
        W::KeyY => C::KeyY,
        W::KeyZ => C::KeyZ,
        W::Digit0 => C::Digit0,
        W::Digit1 => C::Digit1,
        W::Digit2 => C::Digit2,
        W::Digit3 => C::Digit3,
        W::Digit4 => C::Digit4,
        W::Digit5 => C::Digit5,
        W::Digit6 => C::Digit6,
        W::Digit7 => C::Digit7,
        W::Digit8 => C::Digit8,
        W::Digit9 => C::Digit9,
        W::Minus => C::Minus,
        W::Equal => C::Equal,
        W::BracketLeft => C::BracketLeft,
        W::BracketRight => C::BracketRight,
        W::Quote => C::Quote,
        W::Comma => C::Comma,
        W::Period => C::Period,
        W::Slash => C::Slash,
        W::Backquote => C::Backquote,
        W::Backslash => C::Backslash,
        W::Semicolon => C::Semicolon,
        W::Space => C::Space,
        W::Enter => C::Enter,
        W::Tab => C::Tab,
        W::Backspace => C::Backspace,
        W::Escape => C::Escape,
        W::ArrowUp => C::ArrowUp,
        W::ArrowDown => C::ArrowDown,
        W::ArrowLeft => C::ArrowLeft,
        W::ArrowRight => C::ArrowRight,
        W::Home => C::Home,
        W::End => C::End,
        W::PageUp => C::PageUp,
        W::PageDown => C::PageDown,
        W::Insert => C::Insert,
        W::Delete => C::Delete,
        W::F1 => C::F1,
        W::F2 => C::F2,
        W::F3 => C::F3,
        W::F4 => C::F4,
        W::F5 => C::F5,
        W::F6 => C::F6,
        W::F7 => C::F7,
        W::F8 => C::F8,
        W::F9 => C::F9,
        W::F10 => C::F10,
        W::F11 => C::F11,
        W::F12 => C::F12,
        W::F13 => C::F13,
        W::F14 => C::F14,
        W::F15 => C::F15,
        W::F16 => C::F16,
        W::F17 => C::F17,
        W::F18 => C::F18,
        W::F19 => C::F19,
        W::F20 => C::F20,
        W::F21 => C::F21,
        W::F22 => C::F22,
        W::F23 => C::F23,
        W::F24 => C::F24,
        W::ShiftLeft => C::ShiftLeft,
        W::ShiftRight => C::ShiftRight,
        W::ControlLeft => C::ControlLeft,
        W::ControlRight => C::ControlRight,
        W::AltLeft => C::AltLeft,
        W::AltRight => C::AltRight,
        W::SuperLeft => C::SuperLeft,
        W::SuperRight => C::SuperRight,
        W::CapsLock => C::CapsLock,
        W::Numpad0 => C::Numpad0,
        W::Numpad1 => C::Numpad1,
        W::Numpad2 => C::Numpad2,
        W::Numpad3 => C::Numpad3,
        W::Numpad4 => C::Numpad4,
        W::Numpad5 => C::Numpad5,
        W::Numpad6 => C::Numpad6,
        W::Numpad7 => C::Numpad7,
        W::Numpad8 => C::Numpad8,
        W::Numpad9 => C::Numpad9,
        W::NumpadDecimal => C::NumpadDecimal,
        W::NumpadAdd => C::NumpadAdd,
        W::NumpadSubtract => C::NumpadSubtract,
        W::NumpadMultiply => C::NumpadMultiply,
        W::NumpadDivide => C::NumpadDivide,
        W::NumpadEnter => C::NumpadEnter,
        _ => C::Unknown,
    }
}

#[cfg(all(test, not(feature = "pty")))]
mod tests {
    use super::*;
    use crate::delegate;
    use crate::model;
    use rune::{Env, PhysicalKeyCode as Code, Runtime, Shell};

    fn host() -> Model {
        let mut model = model::init();
        delegate::setup(&mut model, std::sync::Arc::new(|| {}));
        model
    }

    fn press(model: &mut Model, key: Code, mods: Mods, text: Option<&str>, consumed: Mods) -> bool {
        let event = PhysicalInput::Key {
            key,
            mods,
            pressed: true,
            repeat: false,
            text: text.map(str::to_owned),
            consumed_mods: consumed,
        };
        on_physical(model, &event, &mut Tx::default())
    }

    #[test]
    fn a_letter_reaches_the_shell() {
        let mut model = host();
        assert!(press(
            &mut model,
            Code::KeyA,
            Mods::default(),
            Some("a"),
            Mods::default()
        ));
        assert_eq!(model.shared.take_output(), b"a");
    }

    #[test]
    fn consumed_alt_reaches_the_shell_as_text() {
        let mut model = host();
        let alt = Mods {
            alt: true,
            ..Mods::default()
        };
        assert!(press(&mut model, Code::KeyA, alt, Some("å"), alt));
        assert_eq!(model.shared.take_output(), "å".as_bytes());
    }

    #[test]
    fn a_numpad_digit_reaches_the_shell() {
        let mut model = host();
        assert!(press(
            &mut model,
            Code::Numpad5,
            Mods::default(),
            Some("5"),
            Mods::default()
        ));
        assert_eq!(model.shared.take_output(), b"5");
    }

    #[test]
    fn ctrl_c_reaches_the_shell() {
        let mut model = host();
        let ctrl = Mods {
            ctrl: true,
            cmd: !cfg!(target_os = "macos"),
            ..Mods::default()
        };
        assert!(press(
            &mut model,
            Code::KeyC,
            ctrl,
            Some("c"),
            Mods::default()
        ));
        assert_eq!(model.shared.take_output(), [0x03]);
    }

    fn leaf_surface(model: &Model, index: usize) -> librio::SurfaceId {
        let crate::chrome::Node::Leaf(leaf) = &model.tabs[index].root else {
            panic!("tab {index} is not one shell");
        };
        leaf.surface
    }

    #[test]
    fn command_t_opens_a_tab() {
        let mut model = host();
        let first = model.surfaces.primary().unwrap().surface.id();
        assert!(press(
            &mut model,
            Code::KeyT,
            copy_mods(),
            None,
            Mods::default()
        ));
        assert!(model.shared.take_output().is_empty());
        assert_eq!(model.tabs.len(), 2);
        assert_eq!(model.surfaces.len(), 2);
        assert_eq!(model.active, 1);
        let second = model.surfaces.primary().unwrap().surface.id();
        assert_ne!(first, second);
        assert_eq!(leaf_surface(&model, 1), second);
        assert!(press(
            &mut model,
            Code::KeyA,
            Mods::default(),
            Some("a"),
            Mods::default()
        ));
        assert_eq!(model.shared.take_output(), b"a");
    }

    #[test]
    fn logo_and_cmd_open_a_tab_only_on_macos() {
        let mut model = host();
        let cmd = Mods {
            logo: true,
            cmd: true,
            ..Mods::default()
        };
        let took = press(&mut model, Code::KeyT, cmd, None, Mods::default());
        assert!(model.shared.take_output().is_empty());
        if cfg!(target_os = "macos") {
            assert!(took);
            assert_eq!(model.tabs.len(), 2);
            assert_eq!(model.active, 1);
        } else {
            assert!(!took);
            assert_eq!(model.tabs.len(), 1);
        }
    }

    #[test]
    fn command_1_selects_the_first_tab_on_macos() {
        let mut model = host();
        let first = model.surfaces.primary().unwrap().surface.id();
        assert!(press(
            &mut model,
            Code::KeyT,
            copy_mods(),
            None,
            Mods::default()
        ));
        let cmd = Mods {
            logo: true,
            cmd: true,
            ..Mods::default()
        };
        let took = press(&mut model, Code::Digit1, cmd, None, Mods::default());
        assert!(model.shared.take_output().is_empty());
        if cfg!(target_os = "macos") {
            assert!(took);
            assert_eq!(model.active, 0);
            assert_eq!(model.surfaces.primary().unwrap().surface.id(), first);
            assert!(press(&mut model, Code::Digit9, cmd, None, Mods::default()));
            assert_eq!(model.active, 0);
            assert!(model.shared.take_output().is_empty());
        } else {
            assert!(!took);
            assert_eq!(model.active, 1);
        }
    }

    #[test]
    fn ctrl_tab_cycles_tabs() {
        let mut model = host();
        let first = model.surfaces.primary().unwrap().surface.id();
        assert!(press(
            &mut model,
            Code::KeyT,
            copy_mods(),
            None,
            Mods::default()
        ));
        let second = model.surfaces.primary().unwrap().surface.id();
        let ctrl = Mods {
            ctrl: true,
            ..Mods::default()
        };
        assert!(press(&mut model, Code::Tab, ctrl, None, Mods::default()));
        assert_eq!(model.active, 0);
        assert_eq!(model.surfaces.primary().unwrap().surface.id(), first);
        let back = Mods {
            ctrl: true,
            shift: true,
            ..Mods::default()
        };
        assert!(press(&mut model, Code::Tab, back, None, Mods::default()));
        assert_eq!(model.active, 1);
        assert_eq!(model.surfaces.primary().unwrap().surface.id(), second);
        assert!(model.shared.take_output().is_empty());
    }

    #[test]
    fn command_w_closes_the_selected_tab() {
        let mut model = host();
        let first = model.surfaces.primary().unwrap().surface.id();
        assert!(press(
            &mut model,
            Code::KeyT,
            copy_mods(),
            None,
            Mods::default()
        ));
        assert!(press(
            &mut model,
            Code::KeyW,
            copy_mods(),
            None,
            Mods::default()
        ));
        assert!(model.shared.take_output().is_empty());
        assert_eq!(model.tabs.len(), 1);
        assert_eq!(model.surfaces.len(), 1);
        assert_eq!(model.active, 0);
        assert_eq!(model.surfaces.primary().unwrap().surface.id(), first);
    }

    #[test]
    fn the_last_tab_closes_the_window() {
        let mut rt = Runtime::new(
            host(),
            crate::chrome::view,
            (800.0, 500.0),
            Env::new(true, 1.0),
        );
        rt.update(|model, tx| {
            let event = PhysicalInput::Key {
                key: Code::KeyW,
                mods: copy_mods(),
                pressed: true,
                repeat: false,
                text: None,
                consumed_mods: Mods::default(),
            };
            assert!(on_physical(model, &event, tx));
        });
        assert!(rt.state.tabs.is_empty());
        assert_eq!(rt.state.surfaces.len(), 0);
        assert_eq!(rt.shell_request, Some(Shell::Close));
    }

    #[test]
    fn shift_page_up_scrolls_and_writes_nothing() {
        let mut model = host();
        {
            let host = model.surfaces.primary_mut().unwrap();
            host.rows = 24;
            let mut bytes = Vec::new();
            for _ in 0..40 {
                bytes.extend_from_slice(b"line\r\n");
            }
            host.surface.inject_output(&bytes);
        }
        let shift = Mods {
            shift: true,
            ..Mods::default()
        };
        assert!(press(
            &mut model,
            Code::PageUp,
            shift,
            None,
            Mods::default()
        ));
        assert!(model.shared.take_output().is_empty());
        let host = model.surfaces.primary_mut().unwrap();
        host.render.update();
        assert!(host.render.display_offset() > 0);
    }

    fn type_key(model: &mut Model, event: KeyEvent) -> bool {
        on_key(model, &event, &mut Tx::default())
    }

    #[test]
    fn an_unfocused_letter_reaches_the_shell() {
        let mut model = host();
        assert!(type_key(
            &mut model,
            KeyEvent {
                key: Key::Text("a".into()),
                mods: Mods::default(),
            }
        ));
        assert_eq!(model.shared.take_output(), b"a");
    }

    #[test]
    fn an_unfocused_option_letter_reaches_the_shell_as_text() {
        let mut model = host();
        assert!(type_key(
            &mut model,
            KeyEvent {
                key: Key::Text("å".into()),
                mods: Mods {
                    alt: true,
                    ..Mods::default()
                },
            }
        ));
        assert_eq!(model.shared.take_output(), "å".as_bytes());
    }

    #[test]
    fn an_unfocused_command_t_opens_a_tab_on_macos() {
        let mut model = host();
        let took = type_key(
            &mut model,
            KeyEvent {
                key: Key::Command('t'),
                mods: Mods {
                    logo: true,
                    cmd: true,
                    ..Mods::default()
                },
            },
        );
        assert!(model.shared.take_output().is_empty());
        if cfg!(target_os = "macos") {
            assert!(took);
            assert_eq!(model.tabs.len(), 2);
            assert_eq!(model.active, 1);
        } else {
            assert!(!took);
            assert_eq!(model.tabs.len(), 1);
        }
    }

    fn sized(model: &mut Model) {
        let host = model.surfaces.primary_mut().unwrap();
        host.cell_px = (10, 20);
        host.cols = 80;
        host.rows = 24;
    }

    fn history(model: &mut Model) {
        let host = model.surfaces.primary_mut().unwrap();
        let mut bytes = Vec::new();
        for _ in 0..40 {
            bytes.extend_from_slice(b"line\r\n");
        }
        host.surface.inject_output(&bytes);
    }

    fn wheel(model: &mut Model, dx: f32, dy: f32, mods: Mods) -> bool {
        on_wheel_mark(
            model,
            crate::chrome::PANE_MARK,
            WheelEvent::new(dx, dy).mods(mods),
            1.0,
            &mut Tx::default(),
        )
    }

    fn offset(model: &mut Model) -> usize {
        let host = model.surfaces.primary_mut().unwrap();
        host.render.update();
        host.render.display_offset()
    }

    #[test]
    fn a_fraction_of_a_row_stays_in_the_remainder() {
        let mut model = host();
        sized(&mut model);
        history(&mut model);
        assert!(wheel(&mut model, 0.0, 2.0, Mods::default()));
        let remainder = model.surfaces.primary().unwrap().wheel_remainder;
        assert!((remainder - 0.1).abs() < 1e-5, "{remainder}");
        assert_eq!(offset(&mut model), 0);
        assert!(model.shared.take_output().is_empty());
    }

    #[test]
    fn five_short_wheel_events_make_one_line() {
        let mut model = host();
        sized(&mut model);
        history(&mut model);
        for _ in 0..5 {
            assert!(wheel(&mut model, 0.0, 6.0, Mods::default()));
        }
        let remainder = model.surfaces.primary().unwrap().wheel_remainder;
        assert!((remainder - 0.5).abs() < 1e-4, "{remainder}");
        assert_eq!(offset(&mut model), 1);
    }

    #[test]
    fn a_positive_wheel_moves_toward_history() {
        let mut model = host();
        sized(&mut model);
        history(&mut model);
        assert!(wheel(&mut model, 0.0, 20.0, Mods::default()));
        assert!(offset(&mut model) > 0);
    }

    #[test]
    fn a_shifted_horizontal_notch_scrolls_off_macos() {
        let mut model = host();
        sized(&mut model);
        history(&mut model);
        let shift = Mods {
            shift: true,
            ..Mods::default()
        };
        assert!(wheel(&mut model, 40.0, 0.0, shift));
        let moved = offset(&mut model);
        if cfg!(target_os = "macos") {
            assert_eq!(moved, 0);
            assert_eq!(model.surfaces.primary().unwrap().wheel_remainder, 0.0);
        } else {
            assert_eq!(moved, 2);
        }
    }

    #[test]
    fn focus_loss_drops_the_wheel_fraction() {
        let mut model = host();
        sized(&mut model);
        assert!(wheel(&mut model, 0.0, 2.0, Mods::default()));
        assert!(!on_physical(
            &mut model,
            &PhysicalInput::FocusLost,
            &mut Tx::default()
        ));
        assert_eq!(model.surfaces.primary().unwrap().wheel_remainder, 0.0);
        assert!(model.surfaces.primary().unwrap().pointer_button.is_none());
    }

    #[test]
    fn hit_testing_uses_the_drawn_pitch() {
        let mut model = host();
        {
            let host = model.surfaces.primary_mut().unwrap();
            host.cell_px = (15, 15);
            host.cols = 78;
            host.rows = 78;
        }
        let host = model.surfaces.primary().unwrap();
        let scale = 2.0;
        for i in 0..78 {
            let center = (i as f32 + 0.5) * 7.5;
            assert_eq!(cell_under(host, center, 0.0, scale), Some((i, 0)));
            assert_eq!(cell_under(host, 0.0, center, scale), Some((0, i)));
        }
        assert_eq!(cell_under(host, 588.5, 0.0, scale), None);
        assert_eq!(cell_under(host, 0.0, 588.5, scale), None);
    }

    fn gesture_at(
        model: &mut Model,
        phase: Phase,
        x: f32,
        y: f32,
        clicks: u32,
        mods: librio::Modifiers,
    ) {
        let mark = model
            .surfaces
            .primary()
            .map(|host| host.mark)
            .unwrap_or(crate::chrome::PANE_MARK);
        gesture(
            model,
            mark,
            phase,
            x,
            y,
            clicks,
            0,
            mods,
            1.0,
            &mut Tx::default(),
        );
    }

    fn select_hello(model: &mut Model) {
        sized(model);
        model
            .surfaces
            .primary_mut()
            .unwrap()
            .surface
            .inject_output(b"HELLO WORLD");
        // Pitch is 10. Column 0's left half, then column 4's right half.
        gesture_at(model, Phase::Down, 1.0, 1.0, 1, librio::Modifiers::empty());
        gesture_at(model, Phase::Move, 47.0, 1.0, 1, librio::Modifiers::empty());
        gesture_at(model, Phase::Up, 47.0, 1.0, 1, librio::Modifiers::empty());
    }

    #[test]
    fn a_drag_selects_the_cells_under_the_pointer() {
        let mut model = host();
        select_hello(&mut model);
        let text = model
            .surfaces
            .primary()
            .unwrap()
            .surface
            .selection_text()
            .unwrap_or_default();
        assert!(text.contains("HELLO"), "{text:?}");
        assert!(!text.contains("WORLD"), "{text:?}");
        assert!(!model.surfaces.primary().unwrap().selecting);
    }

    #[test]
    fn two_clicks_select_the_word() {
        let mut model = host();
        sized(&mut model);
        model
            .surfaces
            .primary_mut()
            .unwrap()
            .surface
            .inject_output(b"HELLO WORLD");
        gesture_at(
            &mut model,
            Phase::Down,
            11.0,
            1.0,
            2,
            librio::Modifiers::empty(),
        );
        gesture_at(
            &mut model,
            Phase::Up,
            11.0,
            1.0,
            2,
            librio::Modifiers::empty(),
        );
        let text = model
            .surfaces
            .primary()
            .unwrap()
            .surface
            .selection_text()
            .unwrap_or_default();
        assert!(text.contains("HELLO"), "{text:?}");
        assert!(!text.contains("WORLD"), "{text:?}");
    }

    #[test]
    fn three_clicks_select_the_line() {
        let mut model = host();
        sized(&mut model);
        model
            .surfaces
            .primary_mut()
            .unwrap()
            .surface
            .inject_output(b"HELLO WORLD");
        gesture_at(
            &mut model,
            Phase::Down,
            11.0,
            1.0,
            3,
            librio::Modifiers::empty(),
        );
        gesture_at(
            &mut model,
            Phase::Up,
            11.0,
            1.0,
            3,
            librio::Modifiers::empty(),
        );
        let text = model
            .surfaces
            .primary()
            .unwrap()
            .surface
            .selection_text()
            .unwrap_or_default();
        assert!(text.contains("HELLO WORLD"), "{text:?}");
    }

    #[test]
    fn a_backwards_drag_includes_the_first_column() {
        let mut model = host();
        sized(&mut model);
        model
            .surfaces
            .primary_mut()
            .unwrap()
            .surface
            .inject_output(b"ABCDEF");
        gesture_at(
            &mut model,
            Phase::Down,
            55.0,
            1.0,
            1,
            librio::Modifiers::empty(),
        );
        gesture_at(
            &mut model,
            Phase::Up,
            1.0,
            1.0,
            1,
            librio::Modifiers::empty(),
        );
        let text = model
            .surfaces
            .primary()
            .unwrap()
            .surface
            .selection_text()
            .unwrap_or_default();
        assert!(text.starts_with('A'), "{text:?}");
    }

    #[test]
    fn a_mouse_report_does_not_start_a_selection() {
        let mut model = host();
        sized(&mut model);
        model
            .surfaces
            .primary_mut()
            .unwrap()
            .surface
            .inject_output(b"HELLO\x1b[?1000h\x1b[?1006h");
        gesture_at(
            &mut model,
            Phase::Down,
            1.0,
            1.0,
            1,
            librio::Modifiers::empty(),
        );
        gesture_at(
            &mut model,
            Phase::Up,
            21.0,
            1.0,
            1,
            librio::Modifiers::empty(),
        );
        assert!(model
            .surfaces
            .primary()
            .unwrap()
            .surface
            .selection_text()
            .is_none());
        assert!(model.shared.take_output().starts_with(b"\x1b[<"));
    }

    #[test]
    fn shift_selects_while_the_program_wants_the_mouse() {
        let mut model = host();
        sized(&mut model);
        model
            .surfaces
            .primary_mut()
            .unwrap()
            .surface
            .inject_output(b"HELLO\x1b[?1000h\x1b[?1006h");
        gesture_at(
            &mut model,
            Phase::Down,
            1.0,
            1.0,
            1,
            librio::Modifiers::SHIFT,
        );
        gesture_at(
            &mut model,
            Phase::Up,
            47.0,
            1.0,
            1,
            librio::Modifiers::SHIFT,
        );
        let text = model
            .surfaces
            .primary()
            .unwrap()
            .surface
            .selection_text()
            .unwrap_or_default();
        assert!(text.contains("HELLO"), "{text:?}");
        assert!(model.shared.take_output().is_empty());
    }

    #[test]
    fn mod_click_opens_link_without_mouse_report() {
        let mut model = host();
        sized(&mut model);
        {
            let host = model.surfaces.primary_mut().unwrap();
            host.surface.inject_output(
                b"\x1b]8;;https://example.com\x07link\x1b]8;;\x07\x1b[?1000h\x1b[?1006h",
            );
            host.render.update();
        }
        let (_capture, _opened) = crate::hyperlink::capture_opens();
        let mods = if cfg!(target_os = "macos") {
            librio::Modifiers::SUPER
        } else {
            librio::Modifiers::CTRL
        };
        gesture_at(&mut model, Phase::Down, 1.0, 1.0, 1, mods);
        let opened = crate::hyperlink::take_captured_opens();
        assert_eq!(opened, vec!["https://example.com".to_string()]);
        assert!(
            !model.shared.take_output().starts_with(b"\x1b[<"),
            "mod+click must not produce mouse report"
        );
        assert!(model
            .surfaces
            .primary()
            .unwrap()
            .surface
            .selection_text()
            .is_none());
    }

    #[test]
    fn hover_probe_caches_by_cell() {
        let mut model = host();
        sized(&mut model);
        {
            let host = model.surfaces.primary_mut().unwrap();
            host.surface
                .inject_output(b"\x1b]8;;https://example.com\x07link\x1b]8;;\x07");
            host.render.update();
        }
        let mark = crate::chrome::PANE_MARK;
        let mut point = PointerEvent::default();
        point.x = 1.0;
        point.y = 1.0;
        on_pointer_move_mark(&mut model, mark, point, 1.0, &mut Tx::default());
        let probe = model.surfaces.primary().and_then(|h| h.hover_probe.clone());
        assert!(probe.is_some());
        let mut point = PointerEvent::default();
        point.x = 2.0;
        point.y = 2.0;
        on_pointer_move_mark(&mut model, mark, point, 1.0, &mut Tx::default());
        assert_eq!(
            model.surfaces.primary().and_then(|h| h.hover_probe.clone()),
            probe
        );
    }

    fn copy_mods() -> Mods {
        if cfg!(target_os = "macos") {
            Mods {
                logo: true,
                cmd: true,
                ..Mods::default()
            }
        } else {
            Mods {
                ctrl: true,
                shift: true,
                ..Mods::default()
            }
        }
    }

    #[test]
    fn the_copy_chord_writes_nothing_to_the_shell() {
        let mut model = host();
        select_hello(&mut model);
        assert!(press(
            &mut model,
            Code::KeyC,
            copy_mods(),
            None,
            Mods::default()
        ));
        assert!(model.shared.take_output().is_empty());
        let text = model
            .surfaces
            .primary()
            .unwrap()
            .surface
            .selection_text()
            .unwrap_or_default();
        assert!(text.contains("HELLO"), "{text:?}");
    }

    #[test]
    fn paste_reaches_the_shell() {
        let mut model = host();
        crate::selection::on_paste(&mut model, "pasted", &mut Tx::default());
        assert_eq!(model.shared.take_output(), b"pasted");
    }

    #[test]
    fn the_paste_chord_reads_the_clipboard_into_the_shell() {
        let board = rune::clipboard::memory();
        board.set_text("pasted");
        let mut rt = Runtime::new(
            host(),
            crate::chrome::view,
            (800.0, 500.0),
            Env::new(true, 1.0),
        );
        rt.use_clipboard(&board);
        rt.frame(0.0);
        let (x, y) = rt.find("pane").expect("pane");
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
        rt.frame(0.0);
        rt.physical_input(PhysicalInput::Key {
            key: Code::KeyV,
            mods: copy_mods(),
            pressed: true,
            repeat: false,
            text: None,
            consumed_mods: Mods::default(),
        });
        rt.frame(0.0);
        rt.frame(0.0);
        assert_eq!(rt.state.shared.take_output(), b"pasted");
    }
}
