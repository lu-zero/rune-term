//! The focused pane maps physical keys onto the shell.
//! App chords return false so Rune shortcuts still run.

use rune::{Key, KeyEvent, Mods, PhysicalInput, PhysicalKeyCode, Tx};

use crate::model::Model;

pub fn on_physical(model: &mut Model, event: &PhysicalInput, tx: &mut Tx) -> bool {
    let PhysicalInput::Key {
        key,
        mods,
        pressed,
        repeat,
        text,
        consumed_mods,
    } = event
    else {
        return false;
    };
    deliver(
        model,
        code(*key),
        mods_of(*mods),
        *pressed,
        *repeat,
        text.as_deref(),
        mods_of(*consumed_mods),
        tx,
    )
}

/// Keys that arrive before a click focuses the pane. The window still fills
/// `Tx::physical_key` and `Tx::key_base`. Once the pane is focused,
/// `on_physical` is the only path, so this does not write the key twice.
pub fn on_key(model: &mut Model, event: &KeyEvent, tx: &mut Tx) -> bool {
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
    deliver(
        model,
        code,
        mods_of(event.mods),
        true,
        tx.key_repeat(),
        text.as_deref(),
        mods_of(consumed_logical(event.mods, text.as_deref(), tx.key_base())),
        tx,
    )
}

fn deliver(
    model: &mut Model,
    code: term_input::PhysicalKeyCode,
    mods: term_input::Mods,
    pressed: bool,
    repeat: bool,
    text: Option<&str>,
    consumed: term_input::Mods,
    tx: &mut Tx,
) -> bool {
    if term_input::is_app_chord(code, mods) {
        return false;
    }
    if let Some(sign) = term_input::scroll_chord(code, mods) {
        if pressed {
            if let Some(host) = model.surfaces.primary_mut() {
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
    let Some(host) = model.surfaces.primary_mut() else {
        return false;
    };
    let wrote = host.surface.key(&mapped);
    if wrote {
        tx.live_only();
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
    use rune::PhysicalKeyCode as Code;

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

    #[test]
    fn command_t_is_left_for_the_app_on_macos() {
        let mut model = host();
        let cmd = Mods {
            logo: true,
            cmd: true,
            ..Mods::default()
        };
        let took = press(&mut model, Code::KeyT, cmd, None, Mods::default());
        assert_eq!(took, !cfg!(target_os = "macos"));
        if cfg!(target_os = "macos") {
            assert!(model.shared.take_output().is_empty());
        }
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
    fn an_unfocused_command_chord_is_not_written_on_macos() {
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
        assert!(!took);
        assert!(model.shared.take_output().is_empty());
    }
}
