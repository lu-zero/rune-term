//! `RenderState` as a [`GridSource`](term_grid::GridSource). Straight sRGB.

use librio::{AnsiColor, NamedColor, Square, Style, StyleFlags, TermColors};
use term_grid::{CellSample, CursorSample, GridSource};

pub struct SurfaceGrid<'a> {
    pub render: &'a mut librio::RenderState,
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
        let mut sample = match (sq.raw() >> 30) & 0b11 {
            1 => CellSample {
                ch: ' ',
                width: 0,
                fg: [0, 0, 0, 0],
                bg: opaque(indexed(colors, sq.bg_palette_index() as usize)),
                underline: false,
            },
            2 | 3 => {
                let (r, g, b) = sq.bg_rgb();
                CellSample {
                    ch: ' ',
                    width: 0,
                    fg: [0, 0, 0, 0],
                    bg: [r, g, b, 255],
                    underline: false,
                }
            }
            _ => codepoint(sq, &style, colors),
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
        let glyph = if row < self.render.lines() && col < self.render.columns() {
            let under = self.cell(row, col);
            if under.bg[3] == 0 {
                named(colors, NamedColor::Background)
            } else {
                under.bg.map(|channel| channel as f32 / 255.0)
            }
        } else {
            named(colors, NamedColor::Background)
        };
        CursorSample {
            col: col as u32,
            row: row as u32,
            visible: self.render.cursor_visible(),
            color: glyph,
            bg: named(colors, NamedColor::Cursor),
        }
    }

    fn reset_dirty(&mut self) {
        self.render.reset_dirty();
    }
}

fn blank() -> CellSample {
    CellSample {
        ch: ' ',
        width: 0,
        fg: [0, 0, 0, 0],
        bg: [0, 0, 0, 0],
        underline: false,
    }
}

fn codepoint(sq: Square, style: &Style, colors: &TermColors) -> CellSample {
    let mut fg = opaque(resolve(colors, &style.fg));
    let mut bg = opaque(resolve(colors, &style.bg));
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
    CellSample {
        ch: if width == 0 { ' ' } else { sq.c() },
        width,
        fg,
        bg,
        underline: style.flags.contains(StyleFlags::UNDERLINE),
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

fn resolve(colors: &TermColors, color: &AnsiColor) -> [f32; 4] {
    match color {
        AnsiColor::Spec(rgb) => [
            f32::from(rgb.r) / 255.0,
            f32::from(rgb.g) / 255.0,
            f32::from(rgb.b) / 255.0,
            1.0,
        ],
        AnsiColor::Indexed(index) => indexed(colors, *index as usize),
        AnsiColor::Named(name) => named(colors, *name),
    }
}

fn named(colors: &TermColors, name: NamedColor) -> [f32; 4] {
    if let Some(color) = colors[name] {
        return color;
    }
    let index = name as u16;
    if index < 16 {
        return from_bytes(crate::theme::ANSI16[index as usize]);
    }
    match name {
        NamedColor::Foreground | NamedColor::Cursor => crate::theme::channels(crate::theme::FG),
        NamedColor::LightForeground => crate::theme::channels(crate::theme::FG_BRIGHT),
        NamedColor::Background => crate::theme::channels(crate::theme::BG),
        NamedColor::DimForeground | NamedColor::DimWhite => {
            crate::theme::channels(crate::theme::MUTED)
        }
        NamedColor::DimBlack => from_bytes(crate::theme::ANSI16[0]),
        NamedColor::DimRed => from_bytes(crate::theme::ANSI16[1]),
        NamedColor::DimGreen => from_bytes(crate::theme::ANSI16[2]),
        NamedColor::DimYellow => from_bytes(crate::theme::ANSI16[3]),
        NamedColor::DimBlue => from_bytes(crate::theme::ANSI16[4]),
        NamedColor::DimMagenta => from_bytes(crate::theme::ANSI16[5]),
        NamedColor::DimCyan => from_bytes(crate::theme::ANSI16[6]),
        _ => crate::theme::channels(crate::theme::FG),
    }
}

fn from_bytes(rgb: [u8; 3]) -> [f32; 4] {
    [
        f32::from(rgb[0]) / 255.0,
        f32::from(rgb[1]) / 255.0,
        f32::from(rgb[2]) / 255.0,
        1.0,
    ]
}

fn indexed(colors: &TermColors, index: usize) -> [f32; 4] {
    if index < 269 {
        if let Some(color) = colors[index] {
            return color;
        }
    }
    if (16..232).contains(&index) {
        let v = index - 16;
        let channel = |n: usize| {
            if n == 0 {
                0.0
            } else {
                f32::from(55 + 40 * n as u8) / 255.0
            }
        };
        return [channel(v / 36), channel((v / 6) % 6), channel(v % 6), 1.0];
    }
    if (232..256).contains(&index) {
        let v = f32::from(8 + 10 * (index - 232) as u8) / 255.0;
        return [v, v, v, 1.0];
    }
    from_bytes(crate::theme::ANSI16[index.min(15)])
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
