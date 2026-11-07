//! The caret element is unchanged until the IME change, after multi-pane.

use rune::{El, Ime, Tx};

use crate::model::Model;

pub fn mount_caret(pane: El) -> El {
    pane
}

pub fn on_ime(_model: &mut Model, _ime: &Ime, _tx: &mut Tx) -> bool {
    false
}
