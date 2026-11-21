//! Search match collection and tinting.

use std::sync::Arc;

use librio::{RenderState, Surface};
use term_grid::{CellSample, CursorSample, GridSource};

use crate::theme::Palette;

const MAX_MATCHES: usize = 4096;

/// One visible run. `end` is the last column of the match.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hit {
    pub row: usize,
    pub start: usize,
    pub end: usize,
}

pub fn collect(surface: &Surface, render: &RenderState, pattern: &str) -> Vec<Hit> {
    if pattern.is_empty() {
        return Vec::new();
    }
    let Some(matches) = surface.search(pattern, MAX_MATCHES) else {
        return Vec::new();
    };
    viewport_hits(
        &matches,
        surface.history_size(),
        render.display_offset(),
        render.lines(),
        render.columns(),
    )
}

/// Ring line 0 is the top of the scrollback. Viewport row 0 is ring line
/// `history - offset`.
pub fn viewport_hits(
    matches: &[(u32, u16, u32, u16)],
    history: usize,
    offset: usize,
    rows: usize,
    cols: usize,
) -> Vec<Hit> {
    if rows == 0 || cols == 0 {
        return Vec::new();
    }
    let origin = history.saturating_sub(offset);
    let last = cols - 1;
    let mut hits = Vec::new();
    for &(start_line, start_col, end_line, end_col) in matches {
        let start_line = start_line as usize;
        let end_line = end_line as usize;
        if end_line < start_line {
            continue;
        }
        for ring in start_line..=end_line {
            if ring < origin || ring >= origin + rows {
                continue;
            }
            let (start, end) = if start_line == end_line {
                (start_col as usize, end_col as usize)
            } else if ring == start_line {
                (start_col as usize, last)
            } else if ring == end_line {
                (0, end_col as usize)
            } else {
                (0, last)
            };
            let start = start.min(last);
            let end = end.min(last);
            if start > end {
                continue;
            }
            hits.push(Hit {
                row: ring - origin,
                start,
                end,
            });
        }
    }
    hits
}

pub fn tint<'a, S>(source: S, hits: &'a [Hit], palette: Arc<Palette>) -> Tint<'a, S> {
    Tint {
        source,
        hits,
        palette,
    }
}

pub struct Tint<'a, S> {
    source: S,
    hits: &'a [Hit],
    palette: Arc<Palette>,
}

impl<S: GridSource> GridSource for Tint<'_, S> {
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
        if self
            .hits
            .iter()
            .any(|hit| hit.row == line && col >= hit.start && col <= hit.end)
        {
            mark(&mut sample, &self.palette);
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

fn mark(sample: &mut CellSample, palette: &Palette) {
    sample.fg = solid(palette.bg);
    sample.bg = solid(palette.muted);
}

fn solid(value: u32) -> [u8; 4] {
    [
        ((value >> 16) & 0xff) as u8,
        ((value >> 8) & 0xff) as u8,
        (value & 0xff) as u8,
        255,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_match_above_the_viewport_is_dropped() {
        let matches = [(0, 0, 0, 3), (12, 1, 12, 4)];
        let hits = viewport_hits(&matches, 10, 0, 5, 10);
        assert_eq!(
            hits,
            vec![Hit {
                row: 2,
                start: 1,
                end: 4
            }]
        );
    }

    #[test]
    fn an_invalid_span_is_skipped() {
        let hits = viewport_hits(&[(4, 3, 2, 1)], 0, 0, 10, 10);
        assert!(hits.is_empty());
    }
}
