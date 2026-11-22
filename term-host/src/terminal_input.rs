//! Terminal input delivery against a host surface.

use crate::hyperlink::{self, HoverLink, OpenDecision};
use crate::input;
use crate::input::Phase;
use crate::selection;
use crate::surfaces::HostSurface;
use term_vt::Modifiers;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LinkAction {
    None,
    /// A web or mail link, for `Tx::open_url`.
    Open(String),
    Refused,
    Confirm(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PointerMoveOutcome {
    pub changed_hover: bool,
    pub moved: bool,
}

pub fn focus_lost(host: &mut HostSurface) {
    host.wheel_remainder = 0.0;
    host.pointer_button = None;
    host.selecting = false;
    host.hover_link = None;
    host.hover_probe = None;
}

/// Returns `(scrolled_lines, consumed_event)`.
#[allow(clippy::too_many_arguments)]
pub fn wheel(
    host: &mut HostSurface,
    x: f32,
    y: f32,
    dx: f32,
    dy: f32,
    shift: bool,
    scale: f32,
    mods: Modifiers,
) -> (i32, bool) {
    let pitch_y = input::drawn_pitch(host.cell_px.1, scale);
    if !(pitch_y.is_finite() && pitch_y > 0.0) {
        return (0, true);
    }
    let points = if dy != 0.0 {
        dy
    } else if !cfg!(target_os = "macos") && shift && dx != 0.0 {
        dx
    } else {
        0.0
    };
    host.wheel_remainder += points / pitch_y;
    let lines = host.wheel_remainder.trunc() as i32;
    host.wheel_remainder -= lines as f32;
    if lines != 0 {
        let (col, row) = input::cell_for_wheel(host, x, y, scale);
        host.surface.scroll_wheel(lines, col, row, mods);
        host.scrollbar.note_activity();
    }
    (lines, true)
}

pub fn try_open_link_at(host: &mut HostSurface, x: f32, y: f32, scale: f32) -> LinkAction {
    let cell = input::cell_under(host, x, y, scale);
    let Some(link) = cell.and_then(|(col, row)| {
        hyperlink::resolve_surface(
            &host.surface,
            &host.render,
            usize::from(row),
            usize::from(col),
        )
    }) else {
        return LinkAction::None;
    };
    host.surface.selection_clear();
    host.pointer_button = None;
    host.selecting = false;
    host.hover_link = None;
    host.hover_probe = None;
    match hyperlink::open_decision(&link.uri) {
        OpenDecision::Open => LinkAction::Open(link.uri),
        OpenDecision::Confirm => LinkAction::Confirm(link.uri),
        OpenDecision::Refuse => {
            eprintln!("not opening link: {}", link.uri);
            LinkAction::Refused
        }
    }
}

fn begin_local_selection(
    host: &mut HostSurface,
    cell: Option<(u16, u16)>,
    x: f32,
    clicks: u32,
    scale: f32,
) -> bool {
    let Some((col, row)) = cell else {
        return false;
    };
    host.surface.selection_begin(
        i32::from(row),
        usize::from(col),
        selection::kind(clicks),
        input::side_at(x, col, input::drawn_pitch(host.cell_px.0, scale)),
    );
    true
}

fn update_local_selection(host: &mut HostSurface, cell: Option<(u16, u16)>, x: f32, scale: f32) {
    let Some((col, row)) = cell else {
        return;
    };
    host.surface.selection_update(
        i32::from(row),
        usize::from(col),
        input::side_at(x, col, input::drawn_pitch(host.cell_px.0, scale)),
    );
}

#[allow(clippy::too_many_arguments)]
pub fn gesture(
    host: &mut HostSurface,
    phase: Phase,
    x: f32,
    y: f32,
    clicks: u32,
    button: u8,
    mods: Modifiers,
    scale: f32,
    open_link_on_mod_click: bool,
) -> LinkAction {
    if phase == Phase::Down && button == 0 && open_link_on_mod_click && hyperlink::open_mods(mods) {
        let action = try_open_link_at(host, x, y, scale);
        if action != LinkAction::None {
            return action;
        }
    }
    let cell = input::cell_under(host, x, y, scale);
    match phase {
        Phase::Down => {
            host.pointer_button = Some(button);
            let taken = match cell {
                Some((col, row)) => host.surface.mouse_button(col, row, button, true, mods),
                None => false,
            };
            if button == 0 && !taken {
                host.selecting = begin_local_selection(host, cell, x, clicks, scale);
            } else {
                if taken {
                    host.surface.selection_clear();
                }
                host.selecting = false;
            }
        }
        Phase::Move => {
            if host.selecting {
                update_local_selection(host, cell, x, scale);
            } else if let (Some(held), Some((col, row))) = (host.pointer_button, cell) {
                host.surface.mouse_motion(col, row, held, mods);
            }
        }
        Phase::Up => {
            let selecting = host.selecting;
            host.pointer_button = None;
            host.selecting = false;
            if selecting {
                update_local_selection(host, cell, x, scale);
            } else if let Some((col, row)) = cell {
                host.surface.mouse_button(col, row, button, false, mods);
            }
        }
    }
    LinkAction::None
}

pub fn pointer_move(
    host: &mut HostSurface,
    x: f32,
    y: f32,
    scale: f32,
    mods: Modifiers,
) -> PointerMoveOutcome {
    if host.pointer_button.is_some() {
        return PointerMoveOutcome {
            changed_hover: false,
            moved: false,
        };
    }
    let cell = input::cell_under(host, x, y, scale);
    let hover = match cell {
        Some((col, row)) => {
            let (line, column) = (usize::from(row), usize::from(col));
            if host.hover_probe.as_ref().map(|(l, c, _)| (*l, *c)) == Some((line, column)) {
                host.hover_probe
                    .as_ref()
                    .and_then(|(_, _, link): &(usize, usize, Option<HoverLink>)| link.clone())
            } else {
                let link = hyperlink::resolve_surface(&host.surface, &host.render, line, column);
                host.hover_probe = Some((line, column, link.clone()));
                link
            }
        }
        None => {
            host.hover_probe = None;
            None
        }
    };
    let changed_hover = host.hover_link != hover;
    if changed_hover {
        host.hover_link = hover;
    }
    let mut moved = false;
    if let Some((col, row)) = cell {
        host.surface.mouse_motion(col, row, 3, mods);
        moved = true;
    }
    PointerMoveOutcome {
        changed_hover,
        moved,
    }
}

pub fn secondary_press(
    host: &mut HostSurface,
    x: f32,
    y: f32,
    scale: f32,
    mods: Modifiers,
    report: bool,
) -> bool {
    let mouse_reporting = host.surface.mode_bits() & 1 != 0;
    if report && mouse_reporting && !mods.contains(Modifiers::SHIFT) {
        let Some((col, row)) = input::cell_under(host, x, y, scale) else {
            return true;
        };
        host.surface.mouse_button(col, row, 2, true, mods);
        host.surface.mouse_button(col, row, 2, false, mods);
        return true;
    }
    false
}

pub fn scroll_by_lines(host: &mut HostSurface, lines: i32) {
    host.surface.scroll(lines);
}

pub fn deliver_mapped_key(host: &mut HostSurface, mapped: &term_input::KeyEvent) -> bool {
    if mapped.key.is_none() && mapped.text.is_none() {
        return false;
    }
    let event = term_vt::KeyEvent {
        action: match mapped.action {
            term_input::KeyAction::Press => term_vt::KeyAction::Press,
            term_input::KeyAction::Repeat => term_vt::KeyAction::Repeat,
            term_input::KeyAction::Release => term_vt::KeyAction::Release,
        },
        key: mapped.key.map(key),
        mods: mods(mapped.mods),
        consumed_mods: mods(mapped.consumed_mods),
        text: mapped.text.clone(),
        composing: mapped.composing,
    };
    host.surface.key(&event)
}

fn key(key: term_input::Key) -> term_vt::Key {
    use term_input::Key as K;
    match key {
        K::Char(c) => term_vt::Key::Char(c),
        K::Enter => term_vt::Key::Enter,
        K::Tab => term_vt::Key::Tab,
        K::Backspace => term_vt::Key::Backspace,
        K::Escape => term_vt::Key::Escape,
        K::Up => term_vt::Key::Up,
        K::Down => term_vt::Key::Down,
        K::Left => term_vt::Key::Left,
        K::Right => term_vt::Key::Right,
        K::Home => term_vt::Key::Home,
        K::End => term_vt::Key::End,
        K::PageUp => term_vt::Key::PageUp,
        K::PageDown => term_vt::Key::PageDown,
        K::Insert => term_vt::Key::Insert,
        K::Delete => term_vt::Key::Delete,
        K::F(n) => term_vt::Key::F(n),
        K::Keypad(k) => term_vt::Key::Keypad(k),
        K::CapsLock => term_vt::Key::CapsLock,
        K::ShiftLeft => term_vt::Key::ShiftLeft,
        K::ShiftRight => term_vt::Key::ShiftRight,
        K::ControlLeft => term_vt::Key::ControlLeft,
        K::ControlRight => term_vt::Key::ControlRight,
        K::AltLeft => term_vt::Key::AltLeft,
        K::AltRight => term_vt::Key::AltRight,
        K::SuperLeft => term_vt::Key::SuperLeft,
        K::SuperRight => term_vt::Key::SuperRight,
    }
}

fn mods(mods: term_input::Modifiers) -> term_vt::Modifiers {
    let mut out = term_vt::Modifiers::empty();
    if mods.contains(term_input::Modifiers::SHIFT) {
        out |= term_vt::Modifiers::SHIFT;
    }
    if mods.contains(term_input::Modifiers::CTRL) {
        out |= term_vt::Modifiers::CTRL;
    }
    if mods.contains(term_input::Modifiers::ALT) {
        out |= term_vt::Modifiers::ALT;
    }
    if mods.contains(term_input::Modifiers::SUPER) {
        out |= term_vt::Modifiers::SUPER;
    }
    out
}
