//! Identity until selection tints cells and owns paste.

use rune::Tx;

use crate::model::Model;

pub fn tint<T>(source: T) -> T {
    source
}

/// Does not call `Surface::paste`. Rune's `on_paste` wrapper still consumes
/// a plain-text paste once this hook is attached.
pub fn on_paste(_model: &mut Model, _text: &str, _tx: &mut Tx) {}
