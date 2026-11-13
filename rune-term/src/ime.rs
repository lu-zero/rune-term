//! IME preedit and commit plumbing.

use std::time::Duration;

use rune::{El, Ime, Tx};

use crate::model::Model;

pub fn mount_caret(pane: El, cursor_cell: (u16, u16), cell_px: (u16, u16), scale: f32) -> El {
    let rect = rune_term_core::ime::caret_rect(
        cursor_cell,
        cell_px,
        scale,
        crate::resize::CONTENT_MARGIN_PT,
    );
    pane.child(
        rune::el()
            .key("ime-caret")
            .absolute(rect.x, rect.y)
            .w(rect.w)
            .h(rect.h)
            .ime_cursor(),
    )
}

pub fn on_ime(model: &mut Model, ime: &Ime, tx: &mut Tx) -> bool {
    match ime {
        Ime::Preedit(text, cursor, _clauses) => {
            model.terminal.set_preedit(text.clone(), *cursor);
            tx.wake_after(Duration::ZERO);
            true
        }
        Ime::Commit(text) => {
            let _ = model.terminal.commit_preedit_text(text);
            tx.wake_after(Duration::ZERO);
            true
        }
        Ime::Select(_) => false,
        Ime::Enabled | Ime::Disabled => {
            if matches!(ime, Ime::Disabled) {
                model.terminal.clear_preedit();
            }
            tx.wake_after(Duration::ZERO);
            true
        }
    }
}
