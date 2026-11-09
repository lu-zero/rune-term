//! Physical keys and cell hit testing. This crate knows `librio::KeyEvent`
//! and does not know the window. `map_key` copies platform `text` and
//! `consumed_mods`. It does not synthesize a US-QWERTY string.

use librio::{Key, KeyAction, KeyEvent, Modifiers};

/// Same fields as Rune's `Mods`. The app copies them; this crate does not
/// depend on Rune. `cmd` is not translated onto `librio::Modifiers`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Mods {
    pub shift: bool,
    pub cmd: bool,
    pub alt: bool,
    pub ctrl: bool,
    pub logo: bool,
}

/// The physical keys `map_key` names. Anything else is [`PhysicalKeyCode::Unknown`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PhysicalKeyCode {
    #[default]
    Unknown,
    KeyA,
    KeyB,
    KeyC,
    KeyD,
    KeyE,
    KeyF,
    KeyG,
    KeyH,
    KeyI,
    KeyJ,
    KeyK,
    KeyL,
    KeyM,
    KeyN,
    KeyO,
    KeyP,
    KeyQ,
    KeyR,
    KeyS,
    KeyT,
    KeyU,
    KeyV,
    KeyW,
    KeyX,
    KeyY,
    KeyZ,
    Digit0,
    Digit1,
    Digit2,
    Digit3,
    Digit4,
    Digit5,
    Digit6,
    Digit7,
    Digit8,
    Digit9,
    Minus,
    Equal,
    BracketLeft,
    BracketRight,
    Quote,
    Comma,
    Period,
    Slash,
    Backquote,
    Backslash,
    Semicolon,
    Space,
    Enter,
    Tab,
    Backspace,
    Escape,
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    Home,
    End,
    PageUp,
    PageDown,
    Insert,
    Delete,
    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,
    F13,
    F14,
    F15,
    F16,
    F17,
    F18,
    F19,
    F20,
    F21,
    F22,
    F23,
    F24,
    ShiftLeft,
    ShiftRight,
    ControlLeft,
    ControlRight,
    AltLeft,
    AltRight,
    SuperLeft,
    SuperRight,
    CapsLock,
    Numpad0,
    Numpad1,
    Numpad2,
    Numpad3,
    Numpad4,
    Numpad5,
    Numpad6,
    Numpad7,
    Numpad8,
    Numpad9,
    NumpadDecimal,
    NumpadAdd,
    NumpadSubtract,
    NumpadMultiply,
    NumpadDivide,
    NumpadEnter,
}

/// Layout points of the grid's leftover padding, already divided by scale.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Pad {
    pub left: f32,
    pub top: f32,
}

/// Copies `text` and `consumed_mods` onto a `KeyEvent`.
/// The function does not synthesize a US-QWERTY string.
pub fn map_key(
    key: PhysicalKeyCode,
    mods: Mods,
    pressed: bool,
    repeat: bool,
    text: Option<&str>,
    consumed_mods: Mods,
) -> KeyEvent {
    let action = if !pressed {
        KeyAction::Release
    } else if repeat {
        KeyAction::Repeat
    } else {
        KeyAction::Press
    };
    let text = text.filter(|text| !text.is_empty()).map(str::to_owned);
    let identified = identify(key);
    let key = if identified.is_some() {
        identified
    } else {
        match text.as_deref().map(|text| {
            let mut chars = text.chars();
            (chars.next(), chars.next())
        }) {
            Some((Some(c), None)) => Some(Key::Char(c)),
            _ => None,
        }
    };
    KeyEvent {
        action,
        key,
        mods: to_modifiers(mods),
        consumed_mods: to_modifiers(consumed_mods),
        text,
        composing: false,
    }
}

pub fn is_app_chord(key: PhysicalKeyCode, mods: Mods) -> bool {
    if matches!(key, PhysicalKeyCode::Tab) && mods.ctrl && !mods.alt && !mods.logo {
        return true;
    }
    if cfg!(target_os = "macos") {
        mods.logo && !mods.ctrl && !mods.alt && !mods.shift && cmd_chord_key(key)
    } else {
        mods.ctrl && mods.shift && !mods.alt && !mods.logo && ctrl_shift_chord_key(key)
    }
}

/// Shift+PageUp is `Some(1)` (one page toward history). Shift+PageDown is
/// `Some(-1)`. Plain PageUp is `None`.
pub fn scroll_chord(key: PhysicalKeyCode, mods: Mods) -> Option<i32> {
    if !mods.shift || mods.ctrl || mods.alt || mods.logo || mods.cmd {
        return None;
    }
    match key {
        PhysicalKeyCode::PageUp => Some(1),
        PhysicalKeyCode::PageDown => Some(-1),
        _ => None,
    }
}

/// All arguments are layout points, the same space as a wheel event.
/// `cell_w` and `cell_h` are the drawn pitch, `f32::from(cell_px) / scale`.
/// `Pad` is the grid padding divided by scale.
pub fn cell_at(
    local_x: f32,
    local_y: f32,
    cell_w: f32,
    cell_h: f32,
    cols: u16,
    rows: u16,
    pad: Pad,
) -> Option<(u16, u16)> {
    if !(cell_w > 0.0 && cell_h > 0.0) {
        return None;
    }
    let x = local_x - pad.left;
    let y = local_y - pad.top;
    if !(x >= 0.0 && y >= 0.0) {
        return None;
    }
    let col = (x / cell_w).floor();
    let row = (y / cell_h).floor();
    if !(col < f32::from(cols) && row < f32::from(rows)) {
        return None;
    }
    Some((col as u16, row as u16))
}

fn to_modifiers(mods: Mods) -> Modifiers {
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

fn identify(code: PhysicalKeyCode) -> Option<Key> {
    use PhysicalKeyCode::*;
    Some(match code {
        KeyA => Key::Char('a'),
        KeyB => Key::Char('b'),
        KeyC => Key::Char('c'),
        KeyD => Key::Char('d'),
        KeyE => Key::Char('e'),
        KeyF => Key::Char('f'),
        KeyG => Key::Char('g'),
        KeyH => Key::Char('h'),
        KeyI => Key::Char('i'),
        KeyJ => Key::Char('j'),
        KeyK => Key::Char('k'),
        KeyL => Key::Char('l'),
        KeyM => Key::Char('m'),
        KeyN => Key::Char('n'),
        KeyO => Key::Char('o'),
        KeyP => Key::Char('p'),
        KeyQ => Key::Char('q'),
        KeyR => Key::Char('r'),
        KeyS => Key::Char('s'),
        KeyT => Key::Char('t'),
        KeyU => Key::Char('u'),
        KeyV => Key::Char('v'),
        KeyW => Key::Char('w'),
        KeyX => Key::Char('x'),
        KeyY => Key::Char('y'),
        KeyZ => Key::Char('z'),
        Digit1 => Key::Char('1'),
        Digit2 => Key::Char('2'),
        Digit3 => Key::Char('3'),
        Digit4 => Key::Char('4'),
        Digit5 => Key::Char('5'),
        Digit6 => Key::Char('6'),
        Digit7 => Key::Char('7'),
        Digit8 => Key::Char('8'),
        Digit9 => Key::Char('9'),
        Digit0 => Key::Char('0'),
        Minus => Key::Char('-'),
        Equal => Key::Char('='),
        BracketLeft => Key::Char('['),
        BracketRight => Key::Char(']'),
        Quote => Key::Char('\''),
        Comma => Key::Char(','),
        Period => Key::Char('.'),
        Slash => Key::Char('/'),
        Backquote => Key::Char('`'),
        Backslash => Key::Char('\\'),
        Semicolon => Key::Char(';'),
        Space => Key::Char(' '),
        Enter | NumpadEnter => Key::Enter,
        Tab => Key::Tab,
        Backspace => Key::Backspace,
        Escape => Key::Escape,
        ArrowUp => Key::Up,
        ArrowDown => Key::Down,
        ArrowLeft => Key::Left,
        ArrowRight => Key::Right,
        Home => Key::Home,
        End => Key::End,
        PageUp => Key::PageUp,
        PageDown => Key::PageDown,
        Insert => Key::Insert,
        Delete => Key::Delete,
        F1 => Key::F(1),
        F2 => Key::F(2),
        F3 => Key::F(3),
        F4 => Key::F(4),
        F5 => Key::F(5),
        F6 => Key::F(6),
        F7 => Key::F(7),
        F8 => Key::F(8),
        F9 => Key::F(9),
        F10 => Key::F(10),
        F11 => Key::F(11),
        F12 => Key::F(12),
        F13 => Key::F(13),
        F14 => Key::F(14),
        F15 => Key::F(15),
        F16 => Key::F(16),
        F17 => Key::F(17),
        F18 => Key::F(18),
        F19 => Key::F(19),
        F20 => Key::F(20),
        F21 => Key::F(21),
        F22 => Key::F(22),
        F23 => Key::F(23),
        F24 => Key::F(24),
        ShiftLeft => Key::ShiftLeft,
        ShiftRight => Key::ShiftRight,
        ControlLeft => Key::ControlLeft,
        ControlRight => Key::ControlRight,
        AltLeft => Key::AltLeft,
        AltRight => Key::AltRight,
        SuperLeft => Key::SuperLeft,
        SuperRight => Key::SuperRight,
        CapsLock => Key::CapsLock,
        Numpad0 => Key::Char('0'),
        Numpad1 => Key::Char('1'),
        Numpad2 => Key::Char('2'),
        Numpad3 => Key::Char('3'),
        Numpad4 => Key::Char('4'),
        Numpad5 => Key::Char('5'),
        Numpad6 => Key::Char('6'),
        Numpad7 => Key::Char('7'),
        Numpad8 => Key::Char('8'),
        Numpad9 => Key::Char('9'),
        NumpadDecimal => Key::Char('.'),
        NumpadAdd => Key::Char('+'),
        NumpadSubtract => Key::Char('-'),
        NumpadMultiply => Key::Char('*'),
        NumpadDivide => Key::Char('/'),
        Unknown => return None,
    })
}

fn cmd_chord_key(key: PhysicalKeyCode) -> bool {
    matches!(
        key,
        PhysicalKeyCode::KeyT
            | PhysicalKeyCode::KeyW
            | PhysicalKeyCode::KeyN
            | PhysicalKeyCode::KeyF
            | PhysicalKeyCode::KeyC
            | PhysicalKeyCode::KeyV
            | PhysicalKeyCode::KeyA
            | PhysicalKeyCode::Comma
            | PhysicalKeyCode::Digit1
            | PhysicalKeyCode::Digit2
            | PhysicalKeyCode::Digit3
            | PhysicalKeyCode::Digit4
            | PhysicalKeyCode::Digit5
            | PhysicalKeyCode::Digit6
            | PhysicalKeyCode::Digit7
            | PhysicalKeyCode::Digit8
            | PhysicalKeyCode::Digit9
    )
}

fn ctrl_shift_chord_key(key: PhysicalKeyCode) -> bool {
    matches!(
        key,
        PhysicalKeyCode::KeyT
            | PhysicalKeyCode::KeyW
            | PhysicalKeyCode::KeyN
            | PhysicalKeyCode::KeyF
            | PhysicalKeyCode::KeyC
            | PhysicalKeyCode::KeyV
            | PhysicalKeyCode::KeyA
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use librio::EncodeContext;

    fn legacy() -> EncodeContext {
        EncodeContext {
            alt_is_meta: true,
            ..EncodeContext::default()
        }
    }

    fn encode(event: &KeyEvent) -> Option<Vec<u8>> {
        librio::key::encode(event, &legacy())
    }

    fn shift() -> Mods {
        Mods {
            shift: true,
            ..Mods::default()
        }
    }

    fn alt() -> Mods {
        Mods {
            alt: true,
            ..Mods::default()
        }
    }

    #[test]
    fn shift_text_emits_the_platform_character() {
        let event = map_key(
            PhysicalKeyCode::KeyA,
            shift(),
            true,
            false,
            Some("A"),
            shift(),
        );
        assert_eq!(event.key, Some(Key::Char('a')));
        assert_eq!(encode(&event).as_deref(), Some(b"A".as_slice()));
    }

    #[test]
    fn consumed_alt_emits_the_platform_text() {
        for (code, unmodified, text) in [
            (PhysicalKeyCode::Digit8, "8", "["),
            (PhysicalKeyCode::KeyA, "a", "å"),
        ] {
            let event = map_key(code, alt(), true, false, Some(text), alt());
            assert_eq!(
                encode(&event).as_deref(),
                Some(text.as_bytes()),
                "{unmodified} produced {text}"
            );
        }
    }

    #[test]
    fn ctrl_c_emits_the_control_byte() {
        let mods = Mods {
            ctrl: true,
            ..Mods::default()
        };
        let event = map_key(
            PhysicalKeyCode::KeyC,
            mods,
            true,
            false,
            Some("c"),
            Mods::default(),
        );
        assert_eq!(encode(&event), Some(vec![0x03]));
    }

    #[test]
    fn super_emits_nothing() {
        let mods = Mods {
            logo: true,
            ..Mods::default()
        };
        let event = map_key(
            PhysicalKeyCode::KeyC,
            mods,
            true,
            false,
            Some("c"),
            Mods::default(),
        );
        assert!(event.mods.contains(Modifiers::SUPER));
        assert!(encode(&event).is_none());
    }

    #[test]
    fn arrows_and_plain_page_up_emit_csi() {
        let up = map_key(
            PhysicalKeyCode::ArrowUp,
            Mods::default(),
            true,
            false,
            None,
            Mods::default(),
        );
        assert_eq!(encode(&up).as_deref(), Some(b"\x1b[A".as_slice()));
        let page = map_key(
            PhysicalKeyCode::PageUp,
            Mods::default(),
            true,
            false,
            None,
            Mods::default(),
        );
        assert_eq!(encode(&page).as_deref(), Some(b"\x1b[5~".as_slice()));
        assert!(scroll_chord(PhysicalKeyCode::PageUp, Mods::default()).is_none());
    }

    #[test]
    fn shift_page_keys_are_scroll_chords() {
        assert_eq!(scroll_chord(PhysicalKeyCode::PageUp, shift()), Some(1));
        assert_eq!(scroll_chord(PhysicalKeyCode::PageDown, shift()), Some(-1));
    }

    #[test]
    fn unknown_without_text_invents_nothing() {
        let event = map_key(
            PhysicalKeyCode::Unknown,
            Mods::default(),
            true,
            false,
            None,
            Mods::default(),
        );
        assert!(event.key.is_none());
        assert!(event.text.is_none());
    }

    #[test]
    fn unknown_text_is_forwarded() {
        let one = map_key(
            PhysicalKeyCode::Unknown,
            Mods::default(),
            true,
            false,
            Some("å"),
            Mods::default(),
        );
        assert_eq!(one.key, Some(Key::Char('å')));
        assert_eq!(encode(&one).as_deref(), Some("å".as_bytes()));

        let dead = map_key(
            PhysicalKeyCode::Unknown,
            Mods::default(),
            true,
            false,
            Some("´a"),
            Mods::default(),
        );
        assert!(dead.key.is_none());
        assert_eq!(encode(&dead).as_deref(), Some("´a".as_bytes()));
    }

    #[test]
    fn numpad_keeps_a_key_and_copies_platform_text() {
        let bare = map_key(
            PhysicalKeyCode::Numpad5,
            Mods::default(),
            true,
            false,
            None,
            Mods::default(),
        );
        assert_eq!(bare.key, Some(Key::Char('5')));
        assert_eq!(encode(&bare).as_deref(), Some(b"5".as_slice()));

        let typed = map_key(
            PhysicalKeyCode::Numpad5,
            Mods::default(),
            true,
            false,
            Some("5"),
            Mods::default(),
        );
        assert_eq!(typed.key, Some(Key::Char('5')));
        assert_eq!(encode(&typed).as_deref(), Some(b"5".as_slice()));
        assert_eq!(
            identify(PhysicalKeyCode::NumpadDecimal),
            Some(Key::Char('.'))
        );
        assert_eq!(identify(PhysicalKeyCode::NumpadEnter), Some(Key::Enter));
        assert_eq!(identify(PhysicalKeyCode::NumpadAdd), Some(Key::Char('+')));
    }

    #[test]
    fn ctrl_off_macos_is_not_an_app_chord() {
        let mods = Mods {
            ctrl: true,
            cmd: !cfg!(target_os = "macos"),
            ..Mods::default()
        };
        assert!(!is_app_chord(PhysicalKeyCode::KeyC, mods));
    }

    #[test]
    fn cmd_c_is_an_app_chord_on_macos() {
        let mods = Mods {
            logo: true,
            cmd: true,
            ..Mods::default()
        };
        assert_eq!(
            is_app_chord(PhysicalKeyCode::KeyC, mods),
            cfg!(target_os = "macos")
        );
    }

    #[test]
    fn cell_at_uses_the_drawn_pitch_and_skips_padding() {
        let pad = Pad::default();
        assert_eq!(cell_at(3.7, 7.6, 7.5, 15.0, 78, 24, pad), Some((0, 0)));
        assert_eq!(cell_at(7.5, 0.0, 7.5, 15.0, 78, 24, pad), Some((1, 0)));
        assert_eq!(
            cell_at(78.0 * 7.5 - 0.1, 0.0, 7.5, 15.0, 78, 24, pad),
            Some((77, 0))
        );
        assert!(cell_at(78.0 * 7.5, 0.0, 7.5, 15.0, 78, 24, pad).is_none());
        let pad = Pad {
            left: 4.0,
            top: 2.0,
        };
        assert!(cell_at(3.0, 10.0, 8.0, 16.0, 80, 24, pad).is_none());
        assert_eq!(cell_at(4.0, 2.0, 8.0, 16.0, 80, 24, pad), Some((0, 0)));
    }
}
