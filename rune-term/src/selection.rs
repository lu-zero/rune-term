//! Local selection. The tint swaps a selected cell's colors. Paste writes
//! the clipboard into the shell. OSC 52 stays in the delegate.

use std::sync::Arc;

use librio::{SelectionKind, ViewportSelection};
use rune::Tx;
use term_grid::{CellSample, CursorSample, GridSource};

use crate::grid_source::SurfaceGrid;
use crate::model::Model;
use crate::theme::Palette;

pub fn tint(source: SurfaceGrid<'_>) -> Tint<'_> {
    let selection = source.render.selection();
    let palette = source.palette.clone();
    Tint {
        source,
        selection,
        palette,
    }
}

pub struct Tint<'a> {
    source: SurfaceGrid<'a>,
    selection: Option<ViewportSelection>,
    palette: Arc<Palette>,
}

impl GridSource for Tint<'_> {
    fn columns(&self) -> usize {
        self.source.columns()
    }

    fn lines(&self) -> usize {
        self.source.lines()
    }

    fn row_dirty(&self, line: usize) -> bool {
        self.source.row_dirty(line)
    }

    fn cell(&self, line: usize, col: usize) -> CellSample {
        let mut sample = self.source.cell(line, col);
        if covers(self.selection, line, col) {
            highlight(&mut sample, &self.palette);
        }
        sample
    }

    fn cursor(&self) -> CursorSample {
        self.source.cursor()
    }

    fn reset_dirty(&mut self) {
        self.source.reset_dirty();
    }
}

/// One click is a stream of cells, two is a word, three is a line.
pub(crate) fn kind(clicks: u32) -> SelectionKind {
    match clicks {
        2 => SelectionKind::Word,
        n if n >= 3 => SelectionKind::Line,
        _ => SelectionKind::Simple,
    }
}

/// Copy the selection onto Rune's clipboard. An empty selection writes nothing.
pub fn copy(model: &Model, tx: &mut Tx) {
    let Some(text) = model
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
    if let Some(host) = model.surfaces.primary_mut() {
        host.surface.paste(text);
    }
    tx.live_only();
}

fn covers(selection: Option<ViewportSelection>, line: usize, col: usize) -> bool {
    let Some(selection) = selection else {
        return false;
    };
    let (Ok(line), Ok(col)) = (u16::try_from(line), u16::try_from(col)) else {
        return false;
    };
    let (r0, c0, r1, c1) = ordered(selection);
    if line < r0 || line > r1 {
        return false;
    }
    if selection.is_block {
        let (left, right) = if c0 <= c1 { (c0, c1) } else { (c1, c0) };
        return col >= left && col <= right;
    }
    if line == r0 && col < c0 {
        return false;
    }
    if line == r1 && col > c1 {
        return false;
    }
    true
}

fn ordered(selection: ViewportSelection) -> (u16, u16, u16, u16) {
    let start = (selection.start_line, selection.start_col);
    let end = (selection.end_line, selection.end_col);
    if start <= end {
        (start.0, start.1, end.0, end.1)
    } else {
        (end.0, end.1, start.0, start.1)
    }
}

/// A transparent cell takes the foreground as its highlight. An opaque cell swaps.
fn highlight(sample: &mut CellSample, palette: &Palette) {
    if sample.bg[3] == 0 {
        sample.fg = solid(palette.bg);
        sample.bg = solid(palette.fg);
        return;
    }
    std::mem::swap(&mut sample.fg, &mut sample.bg);
    sample.fg[3] = 255;
    sample.bg[3] = 255;
}

fn solid(value: u32) -> [u8; 4] {
    [
        ((value >> 16) & 0xff) as u8,
        ((value >> 8) & 0xff) as u8,
        (value & 0xff) as u8,
        255,
    ]
}

#[cfg(all(test, not(feature = "pty")))]
mod tests {
    use super::*;
    use crate::delegate;
    use crate::grid_source::SurfaceGrid;
    use crate::model;
    use librio::{SelectionKind, Side};

    fn host() -> crate::model::Model {
        let mut model = model::init();
        delegate::setup(&mut model, std::sync::Arc::new(|| {}));
        model
    }

    #[test]
    fn a_selected_cell_takes_the_foreground_as_its_highlight() {
        let mut model = host();
        {
            let host = model.surfaces.primary_mut().unwrap();
            host.surface.inject_output(b"HELLO WORLD");
            host.surface
                .selection_begin(0, 0, SelectionKind::Simple, Side::Left);
            host.surface.selection_update(0, 4, Side::Right);
            host.render.update();
        }
        let palette = model.palette.clone();
        let host = model.surfaces.primary_mut().unwrap();
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
