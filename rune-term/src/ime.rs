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
            rune_term_core::ime::set_preedit(&mut model.preedit, text.clone(), *cursor);
            tx.wake_after(Duration::ZERO);
            true
        }
        Ime::Commit(text) => {
            model.preedit = None;
            if let Some(host) = model.surfaces.primary_mut() {
                let _ = rune_term_core::ime::commit_text(&host.surface, text);
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
