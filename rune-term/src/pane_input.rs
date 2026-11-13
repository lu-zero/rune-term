//! The focused pane maps physical keys, the wheel, and the pointer onto the shell.
//! App chords return false, except copy, paste, search, and tabs, which this shell handles.
//! Terminal delivery primitives live in `rune_term_core::terminal_input`.

use std::time::Duration;

use rune::{Drag, Key, KeyEvent, PhysicalInput, PointerEvent, Tx, WheelEvent};
use rune_term_core::input as core_input;
use rune_term_core::terminal_input as core_terminal;

use crate::model::Model;
use crate::surfaces::HostSurface;

#[cfg(all(test, not(feature = "pty")))]
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
            core_input::code(*key),
            core_input::mods_of(*mods),
            *pressed,
            *repeat,
            text.as_deref(),
            core_input::mods_of(*consumed_mods),
            tx,
        ),
        PhysicalInput::FocusLost => {
            if let Some(host) = target_host(model, mark) {
                core_terminal::focus_lost(host);
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
    let Some(host) = model.terminal.by_mark_mut(mark) else {
        return false;
    };
    let (lines, consumed) = core_terminal::wheel(
        host,
        event.x,
        event.y,
        event.dx,
        event.dy,
        event.mods.shift,
        scale,
        core_input::modifiers(event.mods),
    );
    if lines != 0 {
        crate::scrollback::note_activity(host, tx);
    }
    tx.live_only();
    consumed
}

pub fn on_drag_mark(model: &mut Model, mark: u32, drag: &mut Drag, scale: f32, tx: &mut Tx) {
    let Some(button) = core_input::button_code(tx.button()) else {
        return;
    };
    let Some(host) = model.terminal.by_mark_mut(mark) else {
        return;
    };
    let action = core_terminal::gesture(
        host,
        drag.phase,
        drag.x,
        drag.y,
        drag.clicks,
        button,
        core_input::modifiers(tx.mods()),
        scale,
        true,
    );
    match action {
        core_terminal::LinkAction::Opened => {
            tx.wake_after(Duration::ZERO);
        }
        core_terminal::LinkAction::Confirm(uri) => {
            model.pending_link_open = Some(uri);
            tx.wake_after(Duration::ZERO);
        }
        core_terminal::LinkAction::None => {
            tx.live_only();
        }
    }
}

/// Bare motion is button 3. A drag already reported the held button.
pub fn on_pointer_move_mark(
    model: &mut Model,
    mark: u32,
    point: PointerEvent,
    scale: f32,
    tx: &mut Tx,
) {
    let Some(host) = model.terminal.by_mark_mut(mark) else {
        return;
    };
    let outcome = core_terminal::pointer_move(
        host,
        point.x,
        point.y,
        scale,
        core_input::modifiers(tx.mods()),
    );
    if outcome.changed_hover {
        tx.wake_after(Duration::ZERO);
    }
}

pub fn on_secondary_mark(
    model: &mut Model,
    mark: u32,
    point: PointerEvent,
    scale: f32,
    tx: &mut Tx,
) {
    let Some(host) = model.terminal.by_mark_mut(mark) else {
        return;
    };
    let mods = core_input::modifiers(tx.mods());
    if core_terminal::secondary_press(host, point.x, point.y, scale, mods) {
        tx.live_only();
        return;
    }

    let has_selection = host.surface.selection_text().is_some_and(|t| !t.is_empty());
    let mut menu = rune::PopupMenu::new();
    if has_selection {
        menu = menu.item(rune::PopupItem::new("copy", "Copy"));
    }
    menu = menu
        .item(rune::PopupItem::new("paste", "Paste"))
        .separator()
        .item(rune::PopupItem::new("search", "Search"))
        .item(rune::PopupItem::new("new-tab", "New Tab"));
    let default_name = model.config.default_profile_name().to_owned();
    let mut names: Vec<String> = model.config.profile_names().map(str::to_owned).collect();
    names.sort();
    if names.len() > 1 || names.first().is_some_and(|n| n != &default_name) {
        menu = menu.separator();
        for name in names {
            let label = if name == default_name {
                format!("New Tab: {name} (default)")
            } else {
                format!("New Tab: {name}")
            };
            menu = menu.item(rune::PopupItem::new(
                format!("new-tab-profile:{name}"),
                label,
            ));
        }
    }
    tx.popup_menu(
        menu,
        point.window(),
        move |model: &mut Model, picked, tx| {
            if let Some(id) = picked {
                context_action(model, mark, id, tx);
            }
        },
    );
}

fn context_action(model: &mut Model, mark: u32, id: &str, tx: &mut Tx) {
    match id {
        "copy" => {
            let Some(text) = model
                .terminal
                .by_mark_mut(mark)
                .and_then(|host| host.surface.selection_text())
                .filter(|text| !text.is_empty())
            else {
                return;
            };
            tx.copy(text);
        }
        "paste" => tx.request_paste(),
        "search" => crate::search::toggle(model),
        "new-tab" => {
            crate::tabs::open(model);
        }
        _ if id.starts_with("new-tab-profile:") => {
            let name = &id["new-tab-profile:".len()..];
            crate::tabs::open_with_profile(model, name);
        }
        _ => {}
    }
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
        .map(core_input::code)
        .unwrap_or_else(|| core_input::logical_code(&event.key));
    let mark = model
        .terminal
        .primary()
        .map(|host| host.mark)
        .unwrap_or(crate::chrome::PANE_MARK);
    deliver(
        model,
        mark,
        code,
        core_input::mods_of(event.mods),
        true,
        tx.key_repeat(),
        text.as_deref(),
        core_input::mods_of(core_input::consumed_logical(
            event.mods,
            text.as_deref(),
            tx.key_base(),
        )),
        tx,
    )
}

fn target_host(model: &mut Model, mark: u32) -> Option<&mut HostSurface> {
    if model.terminal.by_mark(mark).is_some() {
        model.terminal.by_mark_mut(mark)
    } else {
        model.terminal.primary_mut()
    }
}

#[allow(clippy::too_many_arguments)]
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
        return handle_app_chord(model, code, mods, pressed, tx);
    }
    if let Some(sign) = term_input::scroll_chord(code, mods) {
        if pressed {
            if let Some(host) = target_host(model, mark) {
                core_terminal::scroll_by_lines(host, sign.saturating_mul(i32::from(host.rows)));
            }
            tx.live_only();
        }
        return true;
    }
    let mapped = term_input::map_key(code, mods, pressed, repeat, text, consumed);
    let Some(host) = target_host(model, mark) else {
        return false;
    };
    let wrote = core_terminal::deliver_mapped_key(host, &mapped);
    if wrote {
        tx.wake_after(Duration::ZERO);
    }
    wrote
}

fn handle_app_chord(
    model: &mut Model,
    code: term_input::PhysicalKeyCode,
    mods: term_input::Mods,
    pressed: bool,
    tx: &mut Tx,
) -> bool {
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
        crate::settings_ui::open(model);
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
    if let Some(digit) = core_input::tab_digit(code) {
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
    false
}

#[cfg(all(test, not(feature = "pty")))]
mod tests {
    use super::*;
    use crate::delegate;
    use crate::model;
    use rune::{Env, Phase, PhysicalKeyCode as Code, Runtime, Shell};

    fn host() -> Model {
        let mut model = model::init();
        delegate::setup(&mut model, std::sync::Arc::new(|| {}));
        model
    }

    fn press(
        model: &mut Model,
        key: Code,
        mods: rune::Mods,
        text: Option<&str>,
        consumed: rune::Mods,
    ) -> bool {
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
            rune::Mods::default(),
            Some("a"),
            rune::Mods::default()
        ));
        assert_eq!(model.shared.take_output(), b"a");
    }

    #[test]
    fn consumed_alt_reaches_the_shell_as_text() {
        let mut model = host();
        let alt = rune::Mods {
            alt: true,
            ..rune::Mods::default()
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
            rune::Mods::default(),
            Some("5"),
            rune::Mods::default()
        ));
        assert_eq!(model.shared.take_output(), b"5");
    }

    #[test]
    fn ctrl_c_reaches_the_shell() {
        let mut model = host();
        let ctrl = rune::Mods {
            ctrl: true,
            cmd: !cfg!(target_os = "macos"),
            ..rune::Mods::default()
        };
        assert!(press(
            &mut model,
            Code::KeyC,
            ctrl,
            Some("c"),
            rune::Mods::default()
        ));
        assert_eq!(model.shared.take_output(), [0x03]);
    }

    #[test]
    fn the_settings_chord_writes_nothing_to_the_shell() {
        let mut model = host();
        assert!(press(
            &mut model,
            Code::Comma,
            copy_mods(),
            None,
            rune::Mods::default()
        ));
        assert!(model.shared.take_output().is_empty());
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
        let first = model.terminal.primary().unwrap().surface.id();
        assert!(press(
            &mut model,
            Code::KeyT,
            copy_mods(),
            None,
            rune::Mods::default()
        ));
        assert!(model.shared.take_output().is_empty());
        assert_eq!(model.tabs.len(), 2);
        assert_eq!(model.terminal.len(), 2);
        assert_eq!(model.active, 1);
        let second = model.terminal.primary().unwrap().surface.id();
        assert_ne!(first, second);
        assert_eq!(leaf_surface(&model, 1), second);
        assert!(press(
            &mut model,
            Code::KeyA,
            rune::Mods::default(),
            Some("a"),
            rune::Mods::default()
        ));
        assert_eq!(model.shared.take_output(), b"a");
    }

    #[test]
    fn logo_and_cmd_open_a_tab_only_on_macos() {
        let mut model = host();
        let cmd = rune::Mods {
            logo: true,
            cmd: true,
            ..rune::Mods::default()
        };
        let took = press(&mut model, Code::KeyT, cmd, None, rune::Mods::default());
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
        let first = model.terminal.primary().unwrap().surface.id();
        assert!(press(
            &mut model,
            Code::KeyT,
            copy_mods(),
            None,
            rune::Mods::default()
        ));
        let cmd = rune::Mods {
            logo: true,
            cmd: true,
            ..rune::Mods::default()
        };
        let took = press(&mut model, Code::Digit1, cmd, None, rune::Mods::default());
        assert!(model.shared.take_output().is_empty());
        if cfg!(target_os = "macos") {
            assert!(took);
            assert_eq!(model.active, 0);
            assert_eq!(model.terminal.primary().unwrap().surface.id(), first);
            assert!(press(
                &mut model,
                Code::Digit9,
                cmd,
                None,
                rune::Mods::default()
            ));
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
        let first = model.terminal.primary().unwrap().surface.id();
        assert!(press(
            &mut model,
            Code::KeyT,
            copy_mods(),
            None,
            rune::Mods::default()
        ));
        let second = model.terminal.primary().unwrap().surface.id();
        let ctrl = rune::Mods {
            ctrl: true,
            ..rune::Mods::default()
        };
        assert!(press(
            &mut model,
            Code::Tab,
            ctrl,
            None,
            rune::Mods::default()
        ));
        assert_eq!(model.active, 0);
        assert_eq!(model.terminal.primary().unwrap().surface.id(), first);
        let back = rune::Mods {
            ctrl: true,
            shift: true,
            ..rune::Mods::default()
        };
        assert!(press(
            &mut model,
            Code::Tab,
            back,
            None,
            rune::Mods::default()
        ));
        assert_eq!(model.active, 1);
        assert_eq!(model.terminal.primary().unwrap().surface.id(), second);
        assert!(model.shared.take_output().is_empty());
    }

    #[test]
    fn command_w_closes_the_selected_tab() {
        let mut model = host();
        let first = model.terminal.primary().unwrap().surface.id();
        assert!(press(
            &mut model,
            Code::KeyT,
            copy_mods(),
            None,
            rune::Mods::default()
        ));
        assert!(press(
            &mut model,
            Code::KeyW,
            copy_mods(),
            None,
            rune::Mods::default()
        ));
        assert!(model.shared.take_output().is_empty());
        assert_eq!(model.tabs.len(), 1);
        assert_eq!(model.terminal.len(), 1);
        assert_eq!(model.active, 0);
        assert_eq!(model.terminal.primary().unwrap().surface.id(), first);
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
                consumed_mods: rune::Mods::default(),
            };
            assert!(on_physical(model, &event, tx));
        });
        assert!(rt.state.tabs.is_empty());
        assert_eq!(rt.state.terminal.len(), 0);
        assert_eq!(rt.shell_request, Some(Shell::Close));
    }

    #[test]
    fn shift_page_up_scrolls_and_writes_nothing() {
        let mut model = host();
        {
            let host = model.terminal.primary_mut().unwrap();
            host.rows = 24;
            let mut bytes = Vec::new();
            for _ in 0..40 {
                bytes.extend_from_slice(b"line\r\n");
            }
            host.surface.inject_output(&bytes);
        }
        let shift = rune::Mods {
            shift: true,
            ..rune::Mods::default()
        };
        assert!(press(
            &mut model,
            Code::PageUp,
            shift,
            None,
            rune::Mods::default()
        ));
        assert!(model.shared.take_output().is_empty());
        let host = model.terminal.primary_mut().unwrap();
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
                mods: rune::Mods::default(),
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
                mods: rune::Mods {
                    alt: true,
                    ..rune::Mods::default()
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
                mods: rune::Mods {
                    logo: true,
                    cmd: true,
                    ..rune::Mods::default()
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
        let host = model.terminal.primary_mut().unwrap();
        host.cell_px = (10, 20);
        host.cols = 80;
        host.rows = 24;
    }

    fn history(model: &mut Model) {
        let host = model.terminal.primary_mut().unwrap();
        let mut bytes = Vec::new();
        for _ in 0..40 {
            bytes.extend_from_slice(b"line\r\n");
        }
        host.surface.inject_output(&bytes);
    }

    fn wheel(model: &mut Model, dx: f32, dy: f32, mods: rune::Mods) -> bool {
        on_wheel_mark(
            model,
            crate::chrome::PANE_MARK,
            WheelEvent::new(dx, dy).mods(mods),
            1.0,
            &mut Tx::default(),
        )
    }

    fn offset(model: &mut Model) -> usize {
        let host = model.terminal.primary_mut().unwrap();
        host.render.update();
        host.render.display_offset()
    }

    #[test]
    fn a_fraction_of_a_row_stays_in_the_remainder() {
        let mut model = host();
        sized(&mut model);
        history(&mut model);
        assert!(wheel(&mut model, 0.0, 2.0, rune::Mods::default()));
        let remainder = model.terminal.primary().unwrap().wheel_remainder;
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
            assert!(wheel(&mut model, 0.0, 6.0, rune::Mods::default()));
        }
        let remainder = model.terminal.primary().unwrap().wheel_remainder;
        assert!((remainder - 0.5).abs() < 1e-4, "{remainder}");
        assert_eq!(offset(&mut model), 1);
    }

    #[test]
    fn a_positive_wheel_moves_toward_history() {
        let mut model = host();
        sized(&mut model);
        history(&mut model);
        assert!(wheel(&mut model, 0.0, 20.0, rune::Mods::default()));
        assert!(offset(&mut model) > 0);
    }

    #[test]
    fn a_shifted_horizontal_notch_scrolls_off_macos() {
        let mut model = host();
        sized(&mut model);
        history(&mut model);
        let shift = rune::Mods {
            shift: true,
            ..rune::Mods::default()
        };
        assert!(wheel(&mut model, 40.0, 0.0, shift));
        let moved = offset(&mut model);
        if cfg!(target_os = "macos") {
            assert_eq!(moved, 0);
            assert_eq!(model.terminal.primary().unwrap().wheel_remainder, 0.0);
        } else {
            assert_eq!(moved, 2);
        }
    }

    #[test]
    fn focus_loss_drops_the_wheel_fraction() {
        let mut model = host();
        sized(&mut model);
        assert!(wheel(&mut model, 0.0, 2.0, rune::Mods::default()));
        assert!(!on_physical(
            &mut model,
            &PhysicalInput::FocusLost,
            &mut Tx::default()
        ));
        assert_eq!(model.terminal.primary().unwrap().wheel_remainder, 0.0);
        assert!(model.terminal.primary().unwrap().pointer_button.is_none());
    }

    #[test]
    fn hit_testing_uses_the_drawn_pitch() {
        let mut model = host();
        {
            let host = model.terminal.primary_mut().unwrap();
            host.cell_px = (15, 15);
            host.cols = 78;
            host.rows = 78;
        }
        let host = model.terminal.primary().unwrap();
        let scale = 2.0;
        for i in 0..78 {
            let center = (i as f32 + 0.5) * 7.5;
            assert_eq!(
                core_input::cell_under(host, center, 0.0, scale),
                Some((i, 0))
            );
            assert_eq!(
                core_input::cell_under(host, 0.0, center, scale),
                Some((0, i))
            );
        }
        assert_eq!(core_input::cell_under(host, 588.5, 0.0, scale), None);
        assert_eq!(core_input::cell_under(host, 0.0, 588.5, scale), None);
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
            .terminal
            .primary()
            .map(|host| host.mark)
            .unwrap_or(crate::chrome::PANE_MARK);
        let mut tx = Tx::default();
        if let Some(host) = model.terminal.by_mark_mut(mark) {
            let action = rune_term_core::terminal_input::gesture(
                host, phase, x, y, clicks, 0, mods, 1.0, true,
            );
            match action {
                rune_term_core::terminal_input::LinkAction::Opened => {
                    tx.wake_after(Duration::ZERO);
                }
                rune_term_core::terminal_input::LinkAction::Confirm(uri) => {
                    model.pending_link_open = Some(uri);
                    tx.wake_after(Duration::ZERO);
                }
                rune_term_core::terminal_input::LinkAction::None => {
                    tx.live_only();
                }
            }
        }
    }

    fn select_hello(model: &mut Model) {
        sized(model);
        model
            .terminal
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
            .terminal
            .primary()
            .unwrap()
            .surface
            .selection_text()
            .unwrap_or_default();
        assert!(text.contains("HELLO"), "{text:?}");
        assert!(!text.contains("WORLD"), "{text:?}");
        assert!(!model.terminal.primary().unwrap().selecting);
    }

    #[test]
    fn two_clicks_select_the_word() {
        let mut model = host();
        sized(&mut model);
        model
            .terminal
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
            .terminal
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
            .terminal
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
            .terminal
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
            .terminal
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
            .terminal
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
            .terminal
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
            .terminal
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
            .terminal
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
            .terminal
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
            let host = model.terminal.primary_mut().unwrap();
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
            .terminal
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
            let host = model.terminal.primary_mut().unwrap();
            host.surface
                .inject_output(b"\x1b]8;;https://example.com\x07link\x1b]8;;\x07");
            host.render.update();
        }
        let mark = crate::chrome::PANE_MARK;
        let mut point = PointerEvent::default();
        point.x = 1.0;
        point.y = 1.0;
        on_pointer_move_mark(&mut model, mark, point, 1.0, &mut Tx::default());
        let probe = model.terminal.primary().and_then(|h| h.hover_probe.clone());
        assert!(probe.is_some());
        let mut point = PointerEvent::default();
        point.x = 2.0;
        point.y = 2.0;
        on_pointer_move_mark(&mut model, mark, point, 1.0, &mut Tx::default());
        assert_eq!(
            model.terminal.primary().and_then(|h| h.hover_probe.clone()),
            probe
        );
    }

    fn copy_mods() -> rune::Mods {
        if cfg!(target_os = "macos") {
            rune::Mods {
                logo: true,
                cmd: true,
                ..rune::Mods::default()
            }
        } else {
            rune::Mods {
                ctrl: true,
                shift: true,
                ..rune::Mods::default()
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
            rune::Mods::default()
        ));
        assert!(model.shared.take_output().is_empty());
        let text = model
            .terminal
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
            consumed_mods: rune::Mods::default(),
        });
        rt.frame(0.0);
        rt.frame(0.0);
        assert_eq!(rt.state.shared.take_output(), b"pasted");
    }

    #[test]
    fn focus_loss_clears_pointer_hover_and_selection_state() {
        let mut model = host();
        select_hello(&mut model);
        {
            let host = model.terminal.primary_mut().unwrap();
            host.wheel_remainder = 0.3;
            host.pointer_button = Some(0);
            host.selecting = true;
            host.hover_link = Some(crate::hyperlink::HoverLink {
                line: 0,
                start: 0,
                end: 1,
                uri: "https://example.com".to_string(),
            });
            host.hover_probe = Some((0, 0, None));
        }
        assert!(!on_physical(
            &mut model,
            &PhysicalInput::FocusLost,
            &mut Tx::default()
        ));
        let host = model.terminal.primary().unwrap();
        assert_eq!(host.wheel_remainder, 0.0);
        assert!(host.pointer_button.is_none());
        assert!(!host.selecting);
        assert!(host.hover_link.is_none());
        assert!(host.hover_probe.is_none());
    }

    #[test]
    fn mod_click_on_risky_link_requests_confirmation() {
        let mut model = host();
        sized(&mut model);
        {
            let host = model.terminal.primary_mut().unwrap();
            host.surface
                .inject_output(b"\x1b]8;;file:///tmp/x\x07link\x1b]8;;\x07");
            host.render.update();
        }
        let (_capture, _opened) = crate::hyperlink::capture_opens();
        let mods = if cfg!(target_os = "macos") {
            librio::Modifiers::SUPER
        } else {
            librio::Modifiers::CTRL
        };
        gesture_at(&mut model, Phase::Down, 1.0, 1.0, 1, mods);
        assert_eq!(model.pending_link_open.as_deref(), Some("file:///tmp/x"));
        assert!(crate::hyperlink::take_captured_opens().is_empty());
        assert!(model.shared.take_output().is_empty());
        assert!(model
            .terminal
            .primary()
            .unwrap()
            .surface
            .selection_text()
            .is_none());
    }

    #[test]
    fn secondary_click_forwards_to_mouse_reporting_program() {
        let mut model = host();
        sized(&mut model);
        model
            .terminal
            .primary_mut()
            .unwrap()
            .surface
            .inject_output(b"HELLO\x1b[?1000h\x1b[?1006h");
        let mut point = PointerEvent::default();
        point.x = 1.0;
        point.y = 1.0;
        on_secondary_mark(
            &mut model,
            crate::chrome::PANE_MARK,
            point,
            1.0,
            &mut Tx::default(),
        );
        let out = model.shared.take_output();
        assert!(
            out.windows(3).any(|w| w == b"\x1b[<"),
            "expected mouse report, got {out:?}"
        );
    }

    #[test]
    fn a_program_captured_drag_reports_press_and_release_without_selection() {
        let mut model = host();
        sized(&mut model);
        model
            .terminal
            .primary_mut()
            .unwrap()
            .surface
            .inject_output(b"HELLO WORLD\x1b[?1000h\x1b[?1006h");
        let mods = librio::Modifiers::empty();
        gesture_at(&mut model, Phase::Down, 1.0, 1.0, 1, mods);
        assert_eq!(model.terminal.primary().unwrap().pointer_button, Some(0));
        assert!(!model.terminal.primary().unwrap().selecting);
        gesture_at(&mut model, Phase::Move, 21.0, 1.0, 1, mods);
        gesture_at(&mut model, Phase::Up, 21.0, 1.0, 1, mods);
        let out = model.shared.take_output();
        let reports = out.windows(3).filter(|w| *w == b"\x1b[<").count();
        assert!(
            reports >= 2,
            "expected press/release mouse reports, got {reports} in {out:?}"
        );
        assert!(model
            .terminal
            .primary()
            .unwrap()
            .surface
            .selection_text()
            .is_none());
        assert!(model.terminal.primary().unwrap().pointer_button.is_none());
    }

    #[test]
    fn local_selection_update_outside_grid_keeps_last_in_bounds_extent() {
        let mut model = host();
        sized(&mut model);
        model
            .terminal
            .primary_mut()
            .unwrap()
            .surface
            .inject_output(b"HELLO WORLD");
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
            Phase::Move,
            47.0,
            1.0,
            1,
            librio::Modifiers::empty(),
        );
        gesture_at(
            &mut model,
            Phase::Move,
            5000.0,
            5000.0,
            1,
            librio::Modifiers::empty(),
        );
        gesture_at(
            &mut model,
            Phase::Up,
            5000.0,
            5000.0,
            1,
            librio::Modifiers::empty(),
        );
        let text = model
            .terminal
            .primary()
            .unwrap()
            .surface
            .selection_text()
            .unwrap_or_default();
        assert!(text.contains("HELLO"), "{text:?}");
        assert!(!text.contains("WORLD"), "{text:?}");
    }
}
