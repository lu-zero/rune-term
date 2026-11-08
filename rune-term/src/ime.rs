//! IME preedit and commit plumbing.

use librio::{KeyAction, KeyEvent, Modifiers};
use rune::{El, Ime, Tx};

use crate::model::{Model, Preedit};

pub fn mount_caret(pane: El) -> El {
    pane.child(
        rune::el()
            .key("ime-caret")
            .absolute(0.0, 0.0)
            .w(1.0)
            .h(1.0)
            .ime_cursor(),
    )
}

pub fn on_ime(model: &mut Model, ime: &Ime, tx: &mut Tx) -> bool {
    match ime {
        Ime::Preedit(text, cursor) => {
            if text.is_empty() {
                model.preedit = None;
            } else {
                model.preedit = Some(Preedit {
                    text: text.clone(),
                    cursor: *cursor,
                });
            }
            tx.live_only();
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
            tx.live_only();
            true
        }
        Ime::Enabled | Ime::Disabled => {
            if matches!(ime, Ime::Disabled) {
                model.preedit = None;
            }
            tx.live_only();
            true
        }
    }
}
