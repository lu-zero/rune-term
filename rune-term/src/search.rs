//! Search UI and integration over the terminal pane.

use std::sync::Arc;

use rune::{col, text_input, El, Key, Mods, TextEdit, TextStyle};
#[allow(unused_imports)]
pub use rune_term_core::search::{collect, tint, viewport_hits, Hit, Tint};

use crate::model::Model;
use crate::scrollback;
use crate::theme::{self, Palette};

pub fn shown(model: &Model) -> String {
    if model.search.open && model.search.query.focused {
        model.search.query.text.clone()
    } else {
        String::new()
    }
}

pub fn open(model: &mut Model) {
    model.search.open = true;
    model.search.query.focused = true;
}

pub fn close(model: &mut Model) {
    model.search.open = false;
    model.search.query.focused = false;
}

pub fn toggle(model: &mut Model) {
    if model.search.open {
        close(model);
    } else {
        open(model);
    }
}

/// Cmd+F on macOS. Ctrl+Shift+F everywhere else.
pub fn is_chord(key: &Key, mods: Mods) -> bool {
    let Key::Command(c) = key else {
        return false;
    };
    if !c.eq_ignore_ascii_case(&'f') {
        return false;
    }
    if cfg!(target_os = "macos") {
        mods.logo && !mods.ctrl && !mods.alt && !mods.shift
    } else {
        mods.ctrl && mods.shift && !mods.alt && !mods.logo
    }
}

/// The field is out of flow, so opening it does not resize the grid.
pub fn mount(model: &Model, pane: El) -> El {
    let mut stack = col().w_full().grow(1.0).min_h(0.0).min_w(0.0).child(pane);
    if model.search.open && model.search.query.focused {
        stack = stack.child(field(&model.search.query, model.palette.clone()));
    }
    stack
}

fn field(query: &TextEdit, palette: Arc<Palette>) -> El {
    text_input(query, TextStyle::new(13.0, 400.0))
        .key("search")
        .single_line()
        .h(28.0)
        .w(220.0)
        .pin(None, Some(8.0), Some(8.0 + scrollback::WIDTH), None)
        .fill(theme::linear(palette.tab_bg))
        .color(theme::linear(palette.fg))
        .on_edit(|model: &mut Model| &mut model.search.query)
}

#[cfg(all(test, not(feature = "pty")))]
mod tests {
    use super::*;
    use crate::delegate;
    use crate::grid_source::SurfaceGrid;
    use crate::model;
    use crate::pane_input;
    use rune::{Env, Key, KeyEvent, Mods, PhysicalInput, PhysicalKeyCode, Runtime, Tx};
    use term_grid::GridSource;

    fn host() -> Model {
        let mut model = model::init();
        delegate::setup(&mut model, std::sync::Arc::new(|| {}));
        model
    }

    fn chord_mods() -> Mods {
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
                cmd: true,
                ..Mods::default()
            }
        }
    }

    #[test]
    fn the_search_chord_opens_the_field_and_writes_nothing() {
        let mut model = host();
        let event = PhysicalInput::Key {
            key: PhysicalKeyCode::KeyF,
            mods: chord_mods(),
            pressed: true,
            repeat: false,
            text: None,
            consumed_mods: Mods::default(),
        };
        assert!(pane_input::on_physical(
            &mut model,
            &event,
            &mut Tx::default()
        ));
        assert!(model.search.open);
        assert!(model.search.query.focused);
        assert!(model.shared.take_output().is_empty());
    }

    #[test]
    fn the_line_on_screen_is_tinted_and_the_one_above_is_not() {
        let mut model = host();
        {
            let host = model.terminal.primary_mut().unwrap();
            let mut bytes = b"OLDNEEDLE\r\n".to_vec();
            for _ in 0..30 {
                bytes.extend_from_slice(b"filler\r\n");
            }
            bytes.extend_from_slice(b"NEWNEEDLE\r\n");
            host.surface.inject_output(&bytes);
            host.render.update();
            assert!(host.surface.history_size() > 0);
            assert_eq!(host.render.display_offset(), 0);
            let raw = host.surface.search("NEEDLE", 10).expect("pattern");
            assert!(raw.len() >= 2, "the history line is a match too: {raw:?}");
            for row in 0..host.render.lines() {
                assert!(
                    !host.render.text_row(row).contains("OLDNEEDLE"),
                    "row {row}: {:?}",
                    host.render.text_row(row)
                );
            }
        }
        let palette = model.palette.clone();
        let host = model.terminal.primary_mut().unwrap();
        let hits = collect(&host.surface, &host.render, "NEEDLE");
        assert_eq!(hits.len(), 1, "{hits:?}");
        let row = hits[0].row;
        assert!(
            host.render.text_row(row).contains("NEWNEEDLE"),
            "{:?}",
            host.render.text_row(row)
        );
        let source = SurfaceGrid {
            render: &mut host.render,
            palette: palette.clone(),
        };
        let tinted = tint(source, &hits, palette.clone());
        let marked = tinted.cell(row, hits[0].start);
        assert_eq!(
            marked.bg,
            [
                ((palette.muted >> 16) & 0xff) as u8,
                ((palette.muted >> 8) & 0xff) as u8,
                (palette.muted & 0xff) as u8,
                255,
            ]
        );
        assert_eq!(marked.ch, 'N');
        let other = if row == 0 { 1 } else { 0 };
        assert_ne!(tinted.cell(other, hits[0].start).bg, marked.bg);
    }

    #[test]
    fn a_physical_key_misses_the_shell_while_the_field_is_focused() {
        let mut rt = Runtime::new(
            host(),
            crate::chrome::view,
            (800.0, 500.0),
            Env::new(true, 1.0),
        );
        rt.frame(0.0);
        let (x, y) = rt.find("pane").expect("pane");
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
        rt.frame(0.0);
        rt.physical_input(letter(PhysicalKeyCode::KeyA, "a"));
        rt.frame(0.0);
        assert_eq!(rt.state.shared.take_output(), b"a");

        rt.physical_input(PhysicalInput::Key {
            key: PhysicalKeyCode::KeyF,
            mods: chord_mods(),
            pressed: true,
            repeat: false,
            text: None,
            consumed_mods: Mods::default(),
        });
        rt.frame(0.0);
        let field = rt
            .elements()
            .into_iter()
            .find(|el| el.key.as_deref() == Some("search"))
            .expect("search field");
        assert!(field.focused, "rect {:?}", field.rect);

        rt.physical_input(letter(PhysicalKeyCode::KeyB, "b"));
        rt.frame(0.0);
        assert!(
            rt.state.shared.take_output().is_empty(),
            "the pane still took the key"
        );
        rt.key(KeyEvent {
            key: Key::Text("b".into()),
            mods: Mods::default(),
        });
        rt.frame(0.0);
        assert_eq!(rt.state.search.query.text, "b");
        assert!(rt.state.shared.take_output().is_empty());
    }

    fn letter(key: PhysicalKeyCode, text: &str) -> PhysicalInput {
        PhysicalInput::Key {
            key,
            mods: Mods::default(),
            pressed: true,
            repeat: false,
            text: Some(text.to_owned()),
            consumed_mods: Mods::default(),
        }
    }
}
