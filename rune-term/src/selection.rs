//! Local selection.

#[allow(unused_imports)]
pub use rune_term_core::selection::{kind, tint, Tint};

use rune::Tx;

use crate::model::Model;

/// Copy the selection onto Rune's clipboard. An empty selection writes nothing.
pub fn copy(model: &Model, tx: &mut Tx) {
    let Some(text) = model
        .terminal
        .primary()
        .and_then(|host| host.surface.selection_text())
        .filter(|text| !text.is_empty())
    else {
        return;
    };
    tx.copy(text);
}

/// A user paste. `Surface::paste` encodes it; a paste that could run as
/// Enter or close a bracket waits for confirmation first.
pub fn on_paste(model: &mut Model, text: &str, tx: &mut Tx) {
    if text.is_empty() {
        return;
    }
    let Some(host) = model.terminal.primary_mut() else {
        return;
    };
    if host.surface.paste_is_safe(text) {
        host.surface.paste(text);
        tx.live_only();
    } else {
        let mark = host.mark;
        model.pending_paste = Some((mark, text.to_owned()));
    }
}

/// Send the paste the user confirmed, to the pane that received it.
pub fn confirm_paste(model: &mut Model, tx: &mut Tx) {
    if let Some((mark, text)) = model.pending_paste.take() {
        if let Some(host) = model.terminal.by_mark_mut(mark) {
            host.surface.paste(text);
        }
    }
    tx.live_only();
}

#[cfg(all(test, not(feature = "pty")))]
mod tests {
    use super::*;
    use crate::delegate;
    use crate::grid_source::SurfaceGrid;
    use crate::model;
    use rune::{Env, Runtime};
    use term_grid::GridSource;
    use term_vt::{SelectionKind, Side};

    fn host() -> crate::model::Model {
        let mut model = model::init();
        delegate::setup(&mut model, std::sync::Arc::new(|| {}));
        model
    }

    #[test]
    fn an_unsafe_paste_waits_for_confirmation_then_sends() {
        let mut rt = Runtime::new(
            host(),
            crate::chrome::view,
            (800.0, 500.0),
            Env::new(true, 1.0),
        );
        rt.frame(0.0);
        rt.update(|model, tx| {
            crate::selection::on_paste(model, "echo hi\n", tx);
        });
        assert!(rt.state.shared.take_output().is_empty());
        assert!(rt.state.pending_paste.is_some());
        rt.update(|model, tx| {
            crate::selection::confirm_paste(model, tx);
        });
        let out = rt.state.shared.take_output();
        assert_eq!(out, b"echo hi\r");
    }

    #[test]
    fn a_safe_paste_goes_straight_to_the_shell() {
        let mut rt = Runtime::new(
            host(),
            crate::chrome::view,
            (800.0, 500.0),
            Env::new(true, 1.0),
        );
        rt.frame(0.0);
        rt.update(|model, tx| {
            crate::selection::on_paste(model, "echo hi", tx);
        });
        assert_eq!(rt.state.shared.take_output(), b"echo hi");
        assert!(rt.state.pending_paste.is_none());
    }

    #[test]
    fn a_selected_cell_takes_the_foreground_as_its_highlight() {
        let mut model = host();
        {
            let host = model.terminal.primary_mut().unwrap();
            host.surface.inject_output(b"HELLO WORLD");
            host.surface
                .selection_begin(0, 0, SelectionKind::Simple, Side::Left);
            host.surface.selection_update(0, 4, Side::Right);
            host.render.update();
        }
        let palette = model.palette.clone();
        let host = model.terminal.primary_mut().unwrap();
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
