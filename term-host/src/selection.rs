//! Selection rendering helpers.

use std::sync::Arc;

use term_grid::{CellSample, CursorSample, GridSource};
use term_vt::{SelectionKind, ViewportSelection};

use crate::grid_source::SurfaceGrid;
use crate::theme::Palette;

/// Wrap a grid source, tinting selected cells.
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
pub fn kind(clicks: u32) -> SelectionKind {
    match clicks {
        2 => SelectionKind::Word,
        n if n >= 3 => SelectionKind::Line,
        _ => SelectionKind::Simple,
    }
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

fn highlight(sample: &mut CellSample, palette: &Palette) {
    sample.fg = solid(palette.selection_fg);
    sample.bg = solid(palette.selection_bg);
}

fn solid(value: u32) -> [u8; 4] {
    [
        ((value >> 16) & 0xff) as u8,
        ((value >> 8) & 0xff) as u8,
        (value & 0xff) as u8,
        255,
    ]
}
