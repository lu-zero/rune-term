//! Local selection.

#[allow(unused_imports)]
pub use rune_term_core::selection::{kind, tint, Tint};

use rune::Tx;

use crate::model::Model;

/// Copy the selection onto Rune's clipboard. An empty selection writes nothing.
pub fn copy(model: &Model, tx: &mut Tx) {
    let Some(text) = model
        .terminal
        .surfaces
        .primary()
        .and_then(|host| host.surface.selection_text())
        .filter(|text| !text.is_empty())
    else {
        return;
    };
    tx.copy(text);
}

/// A user paste. Bracketed paste is `Surface::paste`'s concern.
pub fn on_paste(model: &mut Model, text: &str, tx: &mut Tx) {
    if text.is_empty() {
        return;
    }
    if let Some(host) = model.terminal.surfaces.primary_mut() {
        host.surface.paste(text);
    }
    tx.live_only();
}

#[cfg(all(test, not(feature = "pty")))]
mod tests {
    use super::*;
    use crate::delegate;
    use crate::grid_source::SurfaceGrid;
    use crate::model;
    use librio::{SelectionKind, Side};
    use term_grid::GridSource;

    fn host() -> crate::model::Model {
        let mut model = model::init();
        delegate::setup(&mut model, std::sync::Arc::new(|| {}));
        model
    }

    #[test]
    fn a_selected_cell_takes_the_foreground_as_its_highlight() {
        let mut model = host();
        {
            let host = model.terminal.surfaces.primary_mut().unwrap();
            host.surface.inject_output(b"HELLO WORLD");
            host.surface
                .selection_begin(0, 0, SelectionKind::Simple, Side::Left);
            host.surface.selection_update(0, 4, Side::Right);
            host.render.update();
        }
        let palette = model.palette.clone();
        let host = model.terminal.surfaces.primary_mut().unwrap();
        let tinted = tint(SurfaceGrid {
            render: &mut host.render,
            palette,
        });
        let selected = tinted.cell(0, 0);
        let outside = tinted.cell(0, 6);
        assert_eq!(selected.ch, 'H');
        assert_eq!(selected.bg[3], 255);
        assert_ne!(selected.bg, [0, 0, 0, 255]);
        assert_eq!(outside.ch, 'W');
        assert_eq!(outside.bg[3], 0);
    }
}
