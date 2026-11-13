//! IME preedit and commit plumbing.

use std::time::Duration;

use librio::{KeyAction, KeyEvent, Modifiers};
use rune::{El, Ime, Tx};

use crate::model::{Model, Preedit};

pub fn mount_caret(pane: El, cursor_cell: (u16, u16), cell_px: (u16, u16), scale: f32) -> El {
    let (col, row) = cursor_cell;
    let (cw, ch) = if scale.is_finite() && scale > 0.0 {
        (f32::from(cell_px.0) / scale, f32::from(cell_px.1) / scale)
    } else {
        (1.0, 1.0)
    };
    let margin = crate::resize::CONTENT_MARGIN_PT;
    let (x, y) = if cw > 0.0 && ch > 0.0 {
        (margin + f32::from(col) * cw, margin + f32::from(row) * ch)
    } else {
        (margin, margin)
    };
    pane.child(
        rune::el()
            .key("ime-caret")
            .absolute(x.max(0.0), y.max(0.0))
            .w(cw.max(1.0))
            .h(ch.max(1.0))
            .ime_cursor(),
    )
}

pub fn on_ime(model: &mut Model, ime: &Ime, tx: &mut Tx) -> bool {
    match ime {
        Ime::Preedit(text, cursor, _clauses) => {
            if text.is_empty() {
                model.preedit = None;
            } else {
                model.preedit = Some(Preedit {
                    text: text.clone(),
                    cursor: *cursor,
                });
            }
            tx.wake_after(Duration::ZERO);
            true
        }
        Ime::Commit(text) => {
            model.preedit = None;
            if !text.is_empty() {
                if let Some(host) = model.surfaces.primary_mut() {
                    let event = KeyEvent {
                        action: KeyAction::Press,
                        key: None,
                        mods: Modifiers::empty(),
                        consumed_mods: Modifiers::empty(),
                        text: Some(text.clone()),
                        composing: false,
                    };
                    host.surface.key(&event);
                }
            }
            tx.wake_after(Duration::ZERO);
            true
        }
        Ime::Select(_) => false,
        Ime::Enabled | Ime::Disabled => {
            if matches!(ime, Ime::Disabled) {
                model.preedit = None;
            }
            tx.wake_after(Duration::ZERO);
            true
        }
    }
}
