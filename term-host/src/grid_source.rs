//! `RenderState` as a [`GridSource`](term_grid::GridSource). Straight sRGB.

use std::sync::Arc;

use term_grid::{CellSample, CursorSample, GridSource};
use term_vt::{AnsiColor, NamedColor, Square, Style, StyleFlags, TermColors};

use crate::theme::Palette;

pub struct SurfaceGrid<'a> {
    pub render: &'a mut term_vt::RenderState,
    pub palette: Arc<Palette>,
}

impl GridSource for SurfaceGrid<'_> {
    fn columns(&self) -> usize {
        self.render.columns()
    }

    fn lines(&self) -> usize {
        self.render.lines()
    }

    fn row_dirty(&self, line: usize) -> bool {
        self.render.row_dirty(line)
    }

    fn cell(&self, line: usize, col: usize) -> CellSample {
        let Some(sq) = self.render.square(line, col).copied() else {
            return blank();
        };
        let style = self.render.style_at(line, col, &sq);
        let colors = self.render.term_colors();
        let palette = self.palette.as_ref();
        let mut sample = match (sq.raw() >> 30) & 0b11 {
            1 => CellSample {
                ch: ' ',
                cluster: None,
                width: 0,
                fg: [0, 0, 0, 0],
                bg: opaque(indexed(colors, sq.bg_palette_index() as usize, palette)),
                underline: false,
            },
            2 | 3 => {
                let (r, g, b) = sq.bg_rgb();
                CellSample {
                    ch: ' ',
                    cluster: None,
                    width: 0,
                    fg: [0, 0, 0, 0],
                    bg: [r, g, b, 255],
                    underline: false,
                }
            }
            _ => codepoint(line, col, sq, &style, self.render, colors, palette),
        };
        if style.flags.contains(StyleFlags::DIM) {
            sample.fg = dim(sample.fg);
            if sample.bg[3] != 0 {
                sample.bg = dim(sample.bg);
            }
        }
        sample
    }

    fn cursor(&self) -> CursorSample {
        let (row, col) = self.render.cursor();
        let colors = self.render.term_colors();
        let palette = self.palette.as_ref();
        CursorSample {
            col: col as u32,
            row: row as u32,
            visible: self.render.cursor_visible(),
            color: crate::theme::channels(palette.cursor_fg),
            bg: named(colors, NamedColor::Cursor, palette),
        }
    }

    fn reset_dirty(&mut self) {
        self.render.reset_dirty();
    }
}

fn blank() -> CellSample {
    CellSample {
        ch: ' ',
        cluster: None,
        width: 0,
        fg: [0, 0, 0, 0],
        bg: [0, 0, 0, 0],
        underline: false,
    }
}

fn codepoint(
    line: usize,
    col: usize,
    sq: Square,
    style: &Style,
    render: &term_vt::RenderState,
    colors: &TermColors,
    palette: &Palette,
) -> CellSample {
    let mut fg = opaque(resolve(colors, &style.fg, palette));
    let mut bg = opaque(resolve(colors, &style.bg, palette));
    let inverse = style.flags.contains(StyleFlags::INVERSE);
    let default_bg = matches!(style.bg, AnsiColor::Named(NamedColor::Background));
    if inverse {
        std::mem::swap(&mut fg, &mut bg);
        fg[3] = 255;
        bg[3] = 255;
    } else if default_bg {
        bg = [0, 0, 0, 0];
    }
    let hidden = style.flags.contains(StyleFlags::HIDDEN);
    let width = if hidden { 0 } else { width_of(sq) };
    let cluster = if width == 0 {
        None
    } else {
        render
            .cell_cluster_text(line, col)
            .map(String::into_boxed_str)
    };
    CellSample {
        ch: if width == 0 { ' ' } else { sq.c() },
        cluster,
        width,
        fg,
        bg,
        underline: style.flags.intersects(StyleFlags::ALL_UNDERLINES),
    }
}

/// Narrow is 1, wide is 2. Spacers and the kitty placeholder draw nothing.
fn width_of(sq: Square) -> u8 {
    if sq.c() == '\u{10EEEE}' {
        return 0;
    }
    match (sq.raw() >> 21) & 0b11 {
        0 => 1,
        1 => 2,
        _ => 0,
    }
}

fn resolve(colors: &TermColors, color: &AnsiColor, palette: &Palette) -> [f32; 4] {
    match color {
        AnsiColor::Spec(rgb) => [
            f32::from(rgb.r) / 255.0,
            f32::from(rgb.g) / 255.0,
            f32::from(rgb.b) / 255.0,
            1.0,
        ],
        AnsiColor::Indexed(index) => indexed(colors, *index as usize, palette),
        AnsiColor::Named(name) => named(colors, *name, palette),
    }
}

fn named(colors: &TermColors, name: NamedColor, palette: &Palette) -> [f32; 4] {
    if let Some(color) = colors[name] {
        return color;
    }
    let index = name as u16;
    if index < 16 {
        return crate::theme::from_bytes(palette.ansi16[index as usize]);
    }
    match name {
        NamedColor::Foreground => crate::theme::channels(palette.fg),
        NamedColor::Cursor => crate::theme::channels(palette.cursor_bg),
        NamedColor::LightForeground => crate::theme::channels(palette.fg_bright),
        NamedColor::Background => crate::theme::channels(palette.bg),
        NamedColor::DimForeground | NamedColor::DimWhite => crate::theme::channels(palette.muted),
        NamedColor::DimBlack => crate::theme::from_bytes(palette.ansi16[0]),
        NamedColor::DimRed => crate::theme::from_bytes(palette.ansi16[1]),
        NamedColor::DimGreen => crate::theme::from_bytes(palette.ansi16[2]),
        NamedColor::DimYellow => crate::theme::from_bytes(palette.ansi16[3]),
        NamedColor::DimBlue => crate::theme::from_bytes(palette.ansi16[4]),
        NamedColor::DimMagenta => crate::theme::from_bytes(palette.ansi16[5]),
        NamedColor::DimCyan => crate::theme::from_bytes(palette.ansi16[6]),
        _ => crate::theme::channels(palette.fg),
    }
}

fn indexed(colors: &TermColors, index: usize, palette: &Palette) -> [f32; 4] {
    if index < 269 {
        if let Some(color) = colors[index] {
            return color;
        }
    }
    if let Ok(key) = u8::try_from(index) {
        if let Some(rgb) = palette.indexed.get(&key) {
            return crate::theme::from_bytes(*rgb);
        }
        if key >= 16 {
            return crate::theme::from_bytes(crate::theme::xterm_index(key));
        }
    }
    crate::theme::from_bytes(palette.ansi16[index.min(15)])
}

fn opaque(color: [f32; 4]) -> [u8; 4] {
    [
        (color[0].clamp(0.0, 1.0) * 255.0).round() as u8,
        (color[1].clamp(0.0, 1.0) * 255.0).round() as u8,
        (color[2].clamp(0.0, 1.0) * 255.0).round() as u8,
        255,
    ]
}

fn dim(px: [u8; 4]) -> [u8; 4] {
    [
        (f32::from(px[0]) * 0.66).round() as u8,
        (f32::from(px[1]) * 0.66).round() as u8,
        (f32::from(px[2]) * 0.66).round() as u8,
        px[3],
    ]
}
