//! Rune-side event conversions over `term-host`.

pub use term_host::input::*;

use rune::{Key, Mods, PhysicalKeyCode, PointerButton};
use term_vt::Modifiers;

/// The terminal button number for a pointer button.
pub fn button_code(button: PointerButton) -> Option<u8> {
    match button {
        PointerButton::Primary => Some(0),
        PointerButton::Middle => Some(1),
        PointerButton::Secondary => Some(2),
        _ => None,
    }
}

/// The host's generic modifier set as term-vt modifiers.
pub fn host_modifiers(mods: term_input::Mods) -> Modifiers {
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

/// The host modifier set as terminal modifiers.
pub fn modifiers(mods: Mods) -> Modifiers {
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

/// Same rules as the window's `consumed_mods`: alt and shift are spent when
/// the platform text differs from the unmodified key. Ctrl stays, so
/// `encode` can apply `ctrl_byte`. `logo` is never spent.
pub fn consumed_logical(mods: Mods, text: Option<&str>, base: Option<char>) -> Mods {
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

/// The physical key that produced a logical key event.
pub fn logical_code(key: &Key) -> term_input::PhysicalKeyCode {
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
pub fn mods_of(mods: Mods) -> term_input::Mods {
    term_input::Mods {
        shift: mods.shift,
        cmd: mods.cmd,
        alt: mods.alt,
        ctrl: mods.ctrl,
        logo: mods.logo,
    }
}

/// The terminal key a physical key maps to.
pub fn code(key: PhysicalKeyCode) -> term_input::PhysicalKeyCode {
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

/// Convert a window pointer phase to the generic host phase.
pub fn phase(phase: rune::Phase) -> term_host::input::Phase {
    match phase {
        rune::Phase::Down => term_host::input::Phase::Down,
        rune::Phase::Move => term_host::input::Phase::Move,
        rune::Phase::Up => term_host::input::Phase::Up,
    }
}
