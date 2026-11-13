//! IME preedit and commit plumbing.

use std::time::Duration;

use librio::{KeyAction, KeyEvent, Modifiers};
use rune::{El, Ime, Tx};
use term_grid::{CellSample, CursorSample, GridSource};

use crate::model::{Model, Preedit};

pub fn mount_caret(pane: El, cursor_cell: (u16, u16), cell_px: (u16, u16), scale: f32) -> El {
    let (col, row) = cursor_cell;
    let (cw, ch) = if scale.is_finite() && scale > 0.0 {
        (f32::from(cell_px.0) / scale, f32::from(cell_px.1) / scale)
    } else {
        (1.0, 1.0)
    };
    let margin = crate::resize::CONTENT_MARGIN_PT;
    let (x, y) = if cw > 0.0 && ch > 0.0 {
        (margin + f32::from(col) * cw, margin + f32::from(row) * ch)
    } else {
        (margin, margin)
    };
    pane.child(
        rune::el()
            .key("ime-caret")
            .absolute(x.max(0.0), y.max(0.0))
            .w(cw.max(1.0))
            .h(ch.max(1.0))
            .ime_cursor(),
    )
}

pub fn on_ime(model: &mut Model, ime: &Ime, tx: &mut Tx) -> bool {
    match ime {
        Ime::Preedit(text, cursor, _clauses) => {
            if text.is_empty() {
                model.preedit = None;
            } else {
                model.preedit = Some(Preedit {
                    text: text.clone(),
                    cursor: *cursor,
                });
            }
            tx.wake_after(Duration::ZERO);
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

pub fn overlay<S: GridSource>(
    source: S,
    preedit: Option<&Preedit>,
    cursor: (u16, u16),
) -> PreeditOverlay<S> {
    PreeditOverlay {
        source,
        preedit: preedit.cloned(),
        cursor_col: usize::from(cursor.0),
        cursor_row: usize::from(cursor.1),
    }
}

pub struct PreeditOverlay<S> {
    source: S,
    preedit: Option<Preedit>,
    cursor_col: usize,
    cursor_row: usize,
}

impl<S: GridSource> GridSource for PreeditOverlay<S> {
    fn columns(&self) -> usize {
        self.source.columns()
    }

    fn lines(&self) -> usize {
        self.source.lines()
    }

    fn row_dirty(&self, line: usize) -> bool {
        if self.preedit.is_some() && line == self.cursor_row {
            return true;
        }
        self.source.row_dirty(line)
    }

    fn cell(&self, line: usize, col: usize) -> CellSample {
        if let Some(preedit) = &self.preedit {
            if line == self.cursor_row && col >= self.cursor_col {
                if let Some((ch, width, selected, spacer)) =
                    preedit_at(preedit, col - self.cursor_col)
                {
                    if spacer {
                        let mut sample = self.source.cell(line, col);
                        sample.ch = ' ';
                        sample.width = 0;
                        return sample;
                    }
                    let mut sample = self.source.cell(line, col);
                    sample.ch = ch;
                    sample.width = width;
                    sample.underline = true;
                    if selected {
                        sample.fg = [0, 0, 0, 255];
                        sample.bg = [206, 205, 195, 255];
                    }
                    return sample;
                }
            }
        }
        self.source.cell(line, col)
    }

    fn cursor(&self) -> CursorSample {
        if self.preedit.is_some() {
            let mut cursor = self.source.cursor();
            cursor.visible = false;
            cursor
        } else {
            self.source.cursor()
        }
    }

    fn reset_dirty(&mut self) {
        self.source.reset_dirty();
    }
}

/// Returns (char, display_width, selected, is_spacer_after_wide) for a grid column
/// offset from preedit start.
fn preedit_at(preedit: &Preedit, col_off: usize) -> Option<(char, u8, bool, bool)> {
    let range = preedit.cursor.unwrap_or((usize::MAX, usize::MAX));
    let mut grid_col = 0usize;
    for (byte_idx, ch) in preedit.text.char_indices() {
        let w = char_width(ch).max(1);
        let next = grid_col + w;
        let in_range = byte_idx >= range.0 && byte_idx < range.1;
        if col_off == grid_col {
            return Some((ch, w.min(2) as u8, in_range, false));
        }
        if w == 2 && col_off == grid_col + 1 {
            return Some((ch, 0, in_range, true));
        }
        grid_col = next;
        if grid_col > col_off + 1 {
            // passed
        }
    }
    None
}

fn char_width(ch: char) -> usize {
    let cp = u32::from(ch);
    // Minimal wide-char check for common IME/CJK cases (no unicode-width dep).
    if matches!(cp,
        0x1100..=0x115F | 0x2E80..=0xA4CF | 0xAC00..=0xD7A3 | 0xF900..=0xFAFF
        | 0xFE10..=0xFE19 | 0xFE30..=0xFE6F | 0xFF01..=0xFF60 | 0xFFE0..=0xFFE6
        | 0x1F300..=0x1F64F | 0x1F900..=0x1F9FF | 0x20000..=0x3FFFD
    ) {
        2
    } else {
        1
    }
}

#[cfg(all(test, not(feature = "pty")))]
mod tests {
    use super::*;
    use term_grid::{CellSample, CursorSample, GridSource};

    struct OneRow {
        cells: Vec<CellSample>,
        cols: usize,
    }

    impl GridSource for OneRow {
        fn columns(&self) -> usize {
            self.cols
        }
        fn lines(&self) -> usize {
            1
        }
        fn row_dirty(&self, _line: usize) -> bool {
            false
        }
        fn cell(&self, _line: usize, col: usize) -> CellSample {
            self.cells.get(col).cloned().unwrap_or(CellSample {
                ch: ' ',
                width: 1,
                fg: [255, 255, 255, 255],
                bg: [0, 0, 0, 255],
                underline: false,
            })
        }
        fn cursor(&self) -> CursorSample {
            CursorSample {
                col: 0,
                row: 0,
                visible: true,
                color: [1.0; 4],
                bg: [0.0; 4],
            }
        }
        fn reset_dirty(&mut self) {}
    }

    #[test]
    fn preedit_overlays_at_cursor_and_hides_cursor() {
        let source = OneRow {
            cells: vec![
                CellSample {
                    ch: 'a',
                    width: 1,
                    fg: [255, 255, 255, 255],
                    bg: [0, 0, 0, 255],
                    underline: false,
                };
                8
            ],
            cols: 8,
        };
        let preedit = Preedit {
            text: "漢字".to_string(),
            cursor: None,
        };
        let over = overlay(source, Some(&preedit), (1, 0));
        assert!(over.row_dirty(0));
        assert_eq!(over.cell(0, 1).ch, '漢');
        assert_eq!(over.cell(0, 1).width, 2);
        assert_eq!(over.cell(0, 2).width, 0); // spacer
        assert_eq!(over.cell(0, 3).ch, '字');
        assert_eq!(over.cell(0, 3).width, 2);
        assert!(over.cell(0, 1).underline);
        assert!(!over.cursor().visible);
    }

    #[test]
    fn preedit_highlights_selected_range() {
        let source = OneRow {
            cells: vec![],
            cols: 8,
        };
        // "ab" - select "a" (bytes 0..1)
        let preedit = Preedit {
            text: "ab".to_string(),
            cursor: Some((0, 1)),
        };
        let over = overlay(source, Some(&preedit), (0, 0));
        assert_eq!(over.cell(0, 0).bg, [206, 205, 195, 255]);
        assert_ne!(over.cell(0, 1).bg, [206, 205, 195, 255]);
    }
}
