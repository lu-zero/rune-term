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
        Ime::Preedit(text, cursor) => {
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
                let idx = col - self.cursor_col;
                if let Some(ch) = preedit.text.chars().nth(idx) {
                    let mut sample = self.source.cell(line, col);
                    sample.ch = ch;
                    sample.width = 1;
                    sample.underline = true;
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

#[cfg(all(test, not(feature = "pty")))]
mod tests {
    use super::*;
    use term_grid::{CellSample, CursorSample, GridSource};

    struct OneRow {
        cells: Vec<CellSample>,
    }

    impl GridSource for OneRow {
        fn columns(&self) -> usize {
            self.cells.len()
        }
        fn lines(&self) -> usize {
            1
        }
        fn row_dirty(&self, _line: usize) -> bool {
            false
        }
        fn cell(&self, _line: usize, col: usize) -> CellSample {
            self.cells[col]
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
        };
        let preedit = Preedit {
            text: "漢字".to_string(),
            cursor: None,
        };
        let over = overlay(source, Some(&preedit), (1, 0));
        assert!(over.row_dirty(0));
        assert_eq!(over.cell(0, 1).ch, '漢');
        assert_eq!(over.cell(0, 2).ch, '字');
        assert!(over.cell(0, 1).underline);
        assert!(!over.cursor().visible);
    }
}
