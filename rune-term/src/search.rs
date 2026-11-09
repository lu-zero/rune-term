//! A field over the pane. It takes focus, so the pane stops seeing keys.
//! `Surface::search` reports ring lines. The tint keeps the ones on screen.

use std::sync::Arc;

use librio::{RenderState, Surface};
use rune::{col, text_input, El, Key, Mods, TextEdit, TextStyle};
use term_grid::{CellSample, CursorSample, GridSource};

use crate::model::Model;
use crate::scrollback;
use crate::theme::{self, Palette};

const MAX_MATCHES: usize = 4096;

/// One visible run. `end` is the last column of the match.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hit {
    pub row: usize,
    pub start: usize,
    pub end: usize,
}

pub fn shown(model: &Model) -> String {
    if model.search_open && model.query.focused {
        model.query.text.clone()
    } else {
        String::new()
    }
}

pub fn open(model: &mut Model) {
    model.search_open = true;
    model.query.focused = true;
}

/// Cmd+F on macOS. Ctrl+Shift+F everywhere else.
pub fn is_chord(key: &Key, mods: Mods) -> bool {
    let Key::Command(c) = key else {
        return false;
    };
    if !c.eq_ignore_ascii_case(&'f') {
        return false;
    }
    if cfg!(target_os = "macos") {
        mods.logo && !mods.ctrl && !mods.alt && !mods.shift
    } else {
        mods.ctrl && mods.shift && !mods.alt && !mods.logo
    }
}

/// The field is out of flow, so opening it does not resize the grid.
pub fn mount(model: &Model, pane: El) -> El {
    let mut stack = col().w_full().grow(1.0).min_h(0.0).min_w(0.0).child(pane);
    if model.search_open && model.query.focused {
        stack = stack.child(field(&model.query, model.palette.clone()));
    }
    stack
}

fn field(query: &TextEdit, palette: Arc<Palette>) -> El {
    text_input(query, TextStyle::new(13.0, 400.0))
        .key("search")
        .single_line()
        .h(28.0)
        .w(220.0)
        .pin(None, Some(8.0), Some(8.0 + scrollback::WIDTH), None)
        .fill(theme::linear(palette.tab_bg))
        .color(theme::linear(palette.fg))
        .on_edit(|model: &mut Model| &mut model.query)
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
mod geometry {
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

#[cfg(all(test, not(feature = "pty")))]
mod tests {
    use super::*;
    use crate::delegate;
    use crate::grid_source::SurfaceGrid;
    use crate::model;
    use crate::pane_input;
    use rune::{Env, KeyEvent, Mods, PhysicalInput, PhysicalKeyCode, Runtime, Tx};

    fn host() -> Model {
        let mut model = model::init();
        delegate::setup(&mut model, std::sync::Arc::new(|| {}));
        model
    }

    fn chord_mods() -> Mods {
        if cfg!(target_os = "macos") {
            Mods {
                logo: true,
                cmd: true,
                ..Mods::default()
            }
        } else {
            Mods {
                ctrl: true,
                shift: true,
                cmd: true,
                ..Mods::default()
            }
        }
    }

    #[test]
    fn the_search_chord_opens_the_field_and_writes_nothing() {
        let mut model = host();
        let event = PhysicalInput::Key {
            key: PhysicalKeyCode::KeyF,
            mods: chord_mods(),
            pressed: true,
            repeat: false,
            text: None,
            consumed_mods: Mods::default(),
        };
        assert!(pane_input::on_physical(
            &mut model,
            &event,
            &mut Tx::default()
        ));
        assert!(model.search_open);
        assert!(model.query.focused);
        assert!(model.shared.take_output().is_empty());
    }

    #[test]
    fn the_line_on_screen_is_tinted_and_the_one_above_is_not() {
        let mut model = host();
        {
            let host = model.surfaces.primary_mut().unwrap();
            let mut bytes = b"OLDNEEDLE\r\n".to_vec();
            for _ in 0..30 {
                bytes.extend_from_slice(b"filler\r\n");
            }
            bytes.extend_from_slice(b"NEWNEEDLE\r\n");
            host.surface.inject_output(&bytes);
            host.render.update();
            assert!(host.surface.history_size() > 0);
            assert_eq!(host.render.display_offset(), 0);
            let raw = host.surface.search("NEEDLE", 10).expect("pattern");
            assert!(raw.len() >= 2, "the history line is a match too: {raw:?}");
            for row in 0..host.render.lines() {
                assert!(
                    !host.render.text_row(row).contains("OLDNEEDLE"),
                    "row {row}: {:?}",
                    host.render.text_row(row)
                );
            }
        }
        let palette = model.palette.clone();
        let host = model.surfaces.primary_mut().unwrap();
        let hits = collect(&host.surface, &host.render, "NEEDLE");
        assert_eq!(hits.len(), 1, "{hits:?}");
        let row = hits[0].row;
        assert!(
            host.render.text_row(row).contains("NEWNEEDLE"),
            "{:?}",
            host.render.text_row(row)
        );
        let source = SurfaceGrid {
            render: &mut host.render,
            palette: palette.clone(),
        };
        let tinted = tint(source, &hits, palette.clone());
        let marked = tinted.cell(row, hits[0].start);
        assert_eq!(marked.bg, solid(palette.muted));
        assert_eq!(marked.ch, 'N');
        let other = if row == 0 { 1 } else { 0 };
        assert_ne!(tinted.cell(other, hits[0].start).bg, marked.bg);
    }

    #[test]
    fn a_physical_key_misses_the_shell_while_the_field_is_focused() {
        let mut rt = Runtime::new(
            host(),
            crate::chrome::view,
            (800.0, 500.0),
            Env::new(true, 1.0),
        );
        rt.frame(0.0);
        let (x, y) = rt.find("pane").expect("pane");
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
        rt.frame(0.0);
        rt.physical_input(letter(PhysicalKeyCode::KeyA, "a"));
        rt.frame(0.0);
        assert_eq!(rt.state.shared.take_output(), b"a");

        rt.physical_input(PhysicalInput::Key {
            key: PhysicalKeyCode::KeyF,
            mods: chord_mods(),
            pressed: true,
            repeat: false,
            text: None,
            consumed_mods: Mods::default(),
        });
        rt.frame(0.0);
        let field = rt
            .elements()
            .into_iter()
            .find(|el| el.key.as_deref() == Some("search"))
            .expect("search field");
        assert!(field.focused, "rect {:?}", field.rect);

        rt.physical_input(letter(PhysicalKeyCode::KeyB, "b"));
        rt.frame(0.0);
        assert!(
            rt.state.shared.take_output().is_empty(),
            "the pane still took the key"
        );
        rt.key(KeyEvent {
            key: Key::Text("b".into()),
            mods: Mods::default(),
        });
        rt.frame(0.0);
        assert_eq!(rt.state.query.text, "b");
        assert!(rt.state.shared.take_output().is_empty());
    }

    fn letter(key: PhysicalKeyCode, text: &str) -> PhysicalInput {
        PhysicalInput::Key {
            key,
            mods: Mods::default(),
            pressed: true,
            repeat: false,
            text: Some(text.to_owned()),
            consumed_mods: Mods::default(),
        }
    }
}
