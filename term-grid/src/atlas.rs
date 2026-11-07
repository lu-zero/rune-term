//! One 2048² gray atlas, plus a color atlas for system emoji.
//! The face is Cascadia Code NF, the file Rio ships. ASCII is rasterized up
//! front. Every other covered codepoint is rasterized on demand into the
//! same cell. A codepoint the face does not cover is drawn by the macOS
//! cascade. The face is OFL; see `CascadiaCode-LICENSE.txt`.

use std::collections::HashMap;

use swash::scale::image::Image;
use swash::scale::{Render, ScaleContext, Source};
use swash::{FontRef, NormalizedCoord};

pub const ATLAS_SIZE: u32 = 2048;

/// Same bytes Sugarloaf embeds. Vendored so a clone builds without the Rio tree.
const FONT: &[u8] = include_bytes!("../CascadiaCodeNF.ttf");

/// Rio's default font size, in points.
const FACE_PX: f32 = 14.0;

/// Cell pitch of the regular face at [`FACE_PX`], in points. Width is the
/// ASCII advance. Height is Rio's line box: ascent, descent, and twice the
/// line gap. Icon glyphs in the Nerd Font file are wider than that advance
/// and do not set the cell.
pub fn face_cell() -> (f32, f32) {
    static CELL: std::sync::OnceLock<(f32, f32)> = std::sync::OnceLock::new();
    *CELL.get_or_init(|| {
        let Some(font) = FontRef::from_index(FONT, 0) else {
            return (8.0, 16.0);
        };
        let coords = regular_coords(&font);
        let metrics = font.metrics(&coords).scale(FACE_PX);
        let advance = font
            .glyph_metrics(&coords)
            .scale(FACE_PX)
            .advance_width(font.charmap().map('0'));
        let width = if advance > 0.0 {
            advance
        } else {
            metrics.max_width
        };
        let body = metrics.ascent.abs() + metrics.descent.abs();
        let height = (body + metrics.leading * 2.0).max(body);
        (width.ceil().max(1.0), height.ceil().max(1.0))
    })
}

fn regular_coords(font: &FontRef<'_>) -> Vec<NormalizedCoord> {
    font.variations()
        .normalized_coords([("wght", 400.0)])
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GlyphRect {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
    /// `0` is the grayscale atlas. `1` is the color atlas.
    pub atlas: u32,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct GlyphKey {
    code: u32,
    cells: u8,
}

pub struct Atlas {
    pixels: Vec<u8>,
    ascii: [Option<GlyphRect>; 95],
    extra: HashMap<GlyphKey, Option<GlyphRect>>,
    cell: (u32, u32),
    /// Cell columns in the atlas. Dynamic glyphs pack after the ASCII rows.
    cols: u32,
    cursor: u32,
    /// Set when `extra` gains a gray bitmap. The renderer uploads once, then clears it.
    dirty: bool,
    /// Premultiplied RGBA. Empty until the first color glyph.
    color: Vec<u8>,
    color_dirty: bool,
    context: ScaleContext,
    coords: Vec<NormalizedCoord>,
    ppem: f32,
    baseline: f32,
    pen_x: f32,
    advance_px: f32,
    ascent: f32,
    body: f32,
    upem: f32,
    has_font: bool,
}

impl Atlas {
    /// Rasterize at 8×16. The renderer rebuilds this for the live cell.
    pub fn new() -> Self {
        Self::with_cell(8, 16)
    }

    /// Fit the monospace advance to `cell_w` and the em box to `cell_h`.
    /// The cell pitch is fixed by the winsize math, so the face scales
    /// down until it sits inside that pitch.
    pub fn with_cell(cell_w: u32, cell_h: u32) -> Self {
        let cell_w = cell_w.max(1);
        let cell_h = cell_h.max(1);
        let mut pixels = vec![0u8; (ATLAS_SIZE * ATLAS_SIZE) as usize];
        let mut ascii = [None; 95];
        let cols = (ATLAS_SIZE / cell_w).max(1);
        let Some(font) = FontRef::from_index(FONT, 0) else {
            return Self::empty(pixels, ascii, (cell_w, cell_h), cols);
        };
        let coords = regular_coords(&font);
        let metrics = font.metrics(&coords);
        let advances = font.glyph_metrics(&coords);
        let zero = font.charmap().map('0');
        let advance_units = {
            let adv = advances.advance_width(zero);
            if adv > 0.0 {
                adv
            } else {
                metrics.average_width.max(1.0)
            }
        };
        let upem = f32::from(metrics.units_per_em.max(1));
        let mut ppem = (cell_w as f32 - 0.5).max(1.0) * upem / advance_units;
        let mut scaled = metrics.scale(ppem);
        // swash reports both distances as positive.
        let mut body = scaled.ascent.abs() + scaled.descent.abs();
        if body > cell_h as f32 && body > 0.0 {
            ppem *= cell_h as f32 / body;
            scaled = metrics.scale(ppem);
            body = scaled.ascent.abs() + scaled.descent.abs();
        }
        let advance_px = advance_units * ppem / upem;
        let baseline = (cell_h as f32 - body) * 0.5 + scaled.ascent.abs();
        let pen_x = (cell_w as f32 - advance_px) * 0.5;

        let mut context = ScaleContext::new();
        let mut scaler = context
            .builder(font)
            .size(ppem)
            .hint(true)
            .normalized_coords(coords.iter().copied())
            .build();
        let stored_coords = coords;
        let raster = Render::new(&[Source::Outline]);
        for code in 0x20u32..=0x7e {
            let index = code - 0x20;
            let image = raster.render(&mut scaler, font.charmap().map(code));
            let Some(image) = image else { continue };
            let (gw, gh) = (image.placement.width, image.placement.height);
            if gw == 0 || gh == 0 || image.data.len() < (gw * gh) as usize {
                continue;
            }
            if image.data.iter().take((gw * gh) as usize).all(|p| *p == 0) {
                continue;
            }
            let col = index % cols;
            let row = index / cols;
            let origin_x = col * cell_w;
            let origin_y = row * cell_h;
            if origin_x + cell_w > ATLAS_SIZE || origin_y + cell_h > ATLAS_SIZE {
                continue;
            }
            blit(
                &mut pixels,
                origin_x,
                origin_y,
                cell_w,
                cell_h,
                &image.data,
                gw,
                gh,
                pen_x + image.placement.left as f32,
                baseline - image.placement.top as f32,
            );
            ascii[index as usize] = Some(GlyphRect {
                x: origin_x,
                y: origin_y,
                w: cell_w,
                h: cell_h,
                atlas: 0,
            });
        }
        drop(scaler);
        let ascii_rows = (95 + cols - 1) / cols;
        Self {
            pixels,
            ascii,
            extra: HashMap::new(),
            cell: (cell_w, cell_h),
            cols,
            // ASCII occupies the first rows. Dynamic glyphs start below them.
            cursor: ascii_rows.max(2) * cols,
            dirty: false,
            color: Vec::new(),
            color_dirty: false,
            context,
            coords: stored_coords,
            ppem,
            baseline,
            pen_x,
            advance_px,
            ascent: scaled.ascent.abs(),
            body,
            upem,
            has_font: true,
        }
    }

    fn empty(pixels: Vec<u8>, ascii: [Option<GlyphRect>; 95], cell: (u32, u32), cols: u32) -> Self {
        Self {
            pixels,
            ascii,
            extra: HashMap::new(),
            cell,
            cols,
            cursor: 0,
            dirty: false,
            color: Vec::new(),
            color_dirty: false,
            context: ScaleContext::new(),
            coords: Vec::new(),
            ppem: 0.0,
            baseline: 0.0,
            pen_x: 0.0,
            advance_px: 0.0,
            ascent: 0.0,
            body: 0.0,
            upem: 1.0,
            has_font: false,
        }
    }

    pub fn cell(&self) -> (u32, u32) {
        self.cell
    }

    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    pub fn glyph(&self, ch: char) -> Option<GlyphRect> {
        let code = u32::from(ch);
        if (0x20..0x7f).contains(&code) {
            self.ascii[(code - 0x20) as usize]
        } else {
            None
        }
    }

    /// Rasterize `ch` into a slot `cells` wide (1 or 2). ASCII is already
    /// placed. A miss is cached so a later frame does not rasterize it again.
    pub fn ensure(&mut self, ch: char, cells: u8) -> Option<GlyphRect> {
        let cells = if cells >= 2 { 2 } else { 1 };
        let code = u32::from(ch);
        if cells == 1 && (0x20..0x7f).contains(&code) {
            return self.ascii[(code - 0x20) as usize];
        }
        let key = GlyphKey { code, cells };
        if let Some(hit) = self.extra.get(&key) {
            return *hit;
        }
        let rect = self.rasterize(ch, cells);
        if rect.is_some_and(|rect| rect.atlas == 0) {
            self.dirty = true;
        }
        self.extra.insert(key, rect);
        rect
    }

    /// True once since the last call, when a new gray bitmap was inserted.
    pub fn take_dirty(&mut self) -> bool {
        std::mem::take(&mut self.dirty)
    }

    /// True once since the last call, when a color bitmap was inserted.
    pub fn take_color_dirty(&mut self) -> bool {
        std::mem::take(&mut self.color_dirty)
    }

    pub fn color_pixels(&self) -> Option<&[u8]> {
        if self.color.is_empty() {
            None
        } else {
            Some(&self.color)
        }
    }

    fn rasterize(&mut self, ch: char, cells: u8) -> Option<GlyphRect> {
        // The face's block outlines sit inside the em, so a scaled blit leaves
        // a gap at the cell edge. Fill these to the slot instead.
        if is_block_element(ch) {
            return self.fill_block(ch, cells);
        }
        if !self.has_font || self.ppem <= 0.0 {
            return None;
        }
        let font = FontRef::from_index(FONT, 0)?;
        let gid = font.charmap().map(ch);
        if gid == 0 {
            return self.place_system(ch, cells);
        }
        let adv_units = font.glyph_metrics(&self.coords).advance_width(gid);
        let glyph_advance = adv_units * self.ppem / self.upem;
        let tiled = tiles(ch);
        let coords = self.coords.clone();
        let mut scaler = self
            .context
            .builder(font)
            .size(self.ppem)
            .hint(!tiled)
            .normalized_coords(coords)
            .build();
        let image = Render::new(&[Source::Outline]).render(&mut scaler, gid);
        drop(scaler);
        let image = image?;
        let (gw, gh) = (image.placement.width, image.placement.height);
        if gw == 0 || gh == 0 || image.data.len() < (gw * gh) as usize {
            return None;
        }
        if image.data.iter().take((gw * gh) as usize).all(|p| *p == 0) {
            return None;
        }
        let origin = self.alloc(cells)?;
        let slot_w = u32::from(cells) * self.cell.0;
        let slot_h = self.cell.1;
        let em_w = self.advance_px * f32::from(cells);
        let body = self.body;
        let ascent = self.ascent;
        let pen_x = self.pen_x;
        let baseline = self.baseline;
        let mode = fit_mode(ch, glyph_advance, slot_w as f32, &image, em_w, ascent, body);
        let pixels = &mut self.pixels;
        match mode {
            Fit::Em => paint_em(pixels, origin, slot_w, slot_h, &image, em_w, body, ascent),
            Fit::Uniform => paint_uniform(pixels, origin, slot_w, slot_h, &image.data, gw, gh),
            Fit::Baseline => {
                let left = pen_x + image.placement.left as f32;
                let top = baseline - image.placement.top as f32;
                let right = left + gw as f32;
                let bottom = top + gh as f32;
                let overflow_x = (-left).max(0.0) + (right - slot_w as f32).max(0.0);
                let overflow_y = (-top).max(0.0) + (bottom - slot_h as f32).max(0.0);
                if overflow_x > 1.0 || overflow_y > 1.0 {
                    paint_uniform(pixels, origin, slot_w, slot_h, &image.data, gw, gh);
                } else {
                    blit(
                        pixels,
                        origin.0,
                        origin.1,
                        slot_w,
                        slot_h,
                        &image.data,
                        gw,
                        gh,
                        left,
                        top,
                    );
                }
            }
        }
        Some(GlyphRect {
            x: origin.0,
            y: origin.1,
            w: slot_w,
            h: slot_h,
            atlas: 0,
        })
    }

    fn place_system(&mut self, ch: char, cells: u8) -> Option<GlyphRect> {
        #[cfg(target_os = "macos")]
        {
            let drawn = crate::fallback::draw(FONT, ch, self.ppem.max(self.cell.1 as f32))?;
            let origin = self.alloc(cells)?;
            let slot_w = u32::from(cells) * self.cell.0;
            let slot_h = self.cell.1;
            if drawn.color {
                self.paint_color(origin, slot_w, slot_h, &drawn);
                self.color_dirty = true;
                return Some(GlyphRect {
                    x: origin.0,
                    y: origin.1,
                    w: slot_w,
                    h: slot_h,
                    atlas: 1,
                });
            }
            self.paint_fallback_gray(origin, slot_w, slot_h, &drawn);
            return Some(GlyphRect {
                x: origin.0,
                y: origin.1,
                w: slot_w,
                h: slot_h,
                atlas: 0,
            });
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = (ch, cells);
            None
        }
    }

    #[cfg(target_os = "macos")]
    fn paint_fallback_gray(
        &mut self,
        origin: (u32, u32),
        slot_w: u32,
        slot_h: u32,
        drawn: &crate::fallback::Drawn,
    ) {
        let left = self.pen_x + drawn.left as f32;
        let top = self.baseline - drawn.top as f32;
        let right = left + drawn.width as f32;
        let bottom = top + drawn.height as f32;
        let overflow_x = (-left).max(0.0) + (right - slot_w as f32).max(0.0);
        let overflow_y = (-top).max(0.0) + (bottom - slot_h as f32).max(0.0);
        if overflow_x > 1.0 || overflow_y > 1.0 {
            paint_uniform(
                &mut self.pixels,
                origin,
                slot_w,
                slot_h,
                &drawn.bytes,
                drawn.width,
                drawn.height,
            );
        } else {
            blit(
                &mut self.pixels,
                origin.0,
                origin.1,
                slot_w,
                slot_h,
                &drawn.bytes,
                drawn.width,
                drawn.height,
                left,
                top,
            );
        }
    }

    #[cfg(target_os = "macos")]
    fn paint_color(
        &mut self,
        origin: (u32, u32),
        slot_w: u32,
        slot_h: u32,
        drawn: &crate::fallback::Drawn,
    ) {
        if self.color.is_empty() {
            self.color = vec![0u8; (ATLAS_SIZE * ATLAS_SIZE * 4) as usize];
        }
        if drawn.width == 0 || drawn.height == 0 || slot_w == 0 || slot_h == 0 {
            return;
        }
        let scale = (slot_w as f32 / drawn.width as f32).min(slot_h as f32 / drawn.height as f32);
        if scale <= 0.0 {
            return;
        }
        let dw = drawn.width as f32 * scale;
        let dh = drawn.height as f32 * scale;
        let ox = (slot_w as f32 - dw) * 0.5;
        let oy = (slot_h as f32 - dh) * 0.5;
        for y in 0..slot_h {
            for x in 0..slot_w {
                let sx = (x as f32 + 0.5 - ox) / scale - 0.5;
                let sy = (y as f32 + 0.5 - oy) / scale - 0.5;
                let sample = bilinear_rgba(&drawn.bytes, drawn.width, drawn.height, sx, sy);
                stamp_color(&mut self.color, origin, x, y, sample);
            }
        }
    }

    fn fill_block(&mut self, ch: char, cells: u8) -> Option<GlyphRect> {
        let origin = self.alloc(cells)?;
        let slot_w = u32::from(cells) * self.cell.0;
        let slot_h = self.cell.1;
        for y in 0..slot_h {
            for x in 0..slot_w {
                if block_covers(ch, x, y, slot_w, slot_h) {
                    stamp(&mut self.pixels, origin, x, y, 255);
                }
            }
        }
        Some(GlyphRect {
            x: origin.0,
            y: origin.1,
            w: slot_w,
            h: slot_h,
            atlas: 0,
        })
    }

    fn alloc(&mut self, cells: u8) -> Option<(u32, u32)> {
        let need = u32::from(cells.max(1));
        let cols = self.cols.max(1);
        let (cell_w, cell_h) = self.cell;
        for _ in 0..cols + 2 {
            let col = self.cursor % cols;
            let row = self.cursor / cols;
            if col + need > cols {
                self.cursor = row.saturating_add(1).saturating_mul(cols);
                continue;
            }
            let origin_x = col * cell_w;
            let origin_y = row * cell_h;
            if origin_y + cell_h > ATLAS_SIZE || origin_x + need * cell_w > ATLAS_SIZE {
                return None;
            }
            self.cursor += need;
            return Some((origin_x, origin_y));
        }
        None
    }

    /// Insert a bitmap. The box-drawing pass uses this. ASCII occupies the
    /// first rows, so callers start below `cell.1 * 2`.
    pub fn insert(&mut self, bitmap: &[u8], width: u32, height: u32, at: (u32, u32)) -> GlyphRect {
        self.dirty = true;
        for row in 0..height {
            for col in 0..width {
                let src = (row * width + col) as usize;
                let dst = ((at.1 + row) * ATLAS_SIZE + at.0 + col) as usize;
                if src < bitmap.len() && dst < self.pixels.len() {
                    self.pixels[dst] = bitmap[src];
                }
            }
        }
        GlyphRect {
            x: at.0,
            y: at.1,
            w: width,
            h: height,
            atlas: 0,
        }
    }
}

impl Default for Atlas {
    fn default() -> Self {
        Self::new()
    }
}

/// Light glyphs on a dark cell. A gamma under 1 keeps the stem from
/// washing out once the coverage is used as text alpha.
fn cover(byte: u8) -> u8 {
    if byte == 0 {
        return 0;
    }
    let t = f32::from(byte) / 255.0;
    (t.powf(0.75) * 255.0).round() as u8
}

fn blit(
    pixels: &mut [u8],
    origin_x: u32,
    origin_y: u32,
    cell_w: u32,
    cell_h: u32,
    src: &[u8],
    src_w: u32,
    src_h: u32,
    left: f32,
    top: f32,
) {
    let left = left.round() as i32;
    let top = top.round() as i32;
    for row in 0..src_h as i32 {
        for col in 0..src_w as i32 {
            let dx = left + col;
            let dy = top + row;
            if dx < 0 || dy < 0 || dx >= cell_w as i32 || dy >= cell_h as i32 {
                continue;
            }
            let sample = src[(row as u32 * src_w + col as u32) as usize];
            let sample = cover(sample);
            if sample == 0 {
                continue;
            }
            let dst = ((origin_y + dy as u32) * ATLAS_SIZE + origin_x + dx as u32) as usize;
            pixels[dst] = pixels[dst].max(sample);
        }
    }
}

enum Fit {
    /// Map the monospace em box onto the slot so rules and blocks meet.
    Em,
    /// Scale the ink into the slot and center it.
    Uniform,
    /// Place on the ASCII baseline.
    Baseline,
}

/// Outlines that are meant to meet the next cell. Block elements are filled
/// separately. Hinting is off so the scaled edge stays on the cell boundary.
fn tiles(ch: char) -> bool {
    let code = u32::from(ch);
    (0x2500..=0x257F).contains(&code) || (0x1FB00..=0x1FBFF).contains(&code) || is_pua(code)
}

fn is_block_element(ch: char) -> bool {
    (0x2580..=0x259F).contains(&u32::from(ch))
}

/// Block Elements, filled to the slot. Halves split on `n/2` so the two
/// sides meet. Eighths use `n * k / 8` so neighboring weights meet too.
fn block_covers(ch: char, x: u32, y: u32, w: u32, h: u32) -> bool {
    let left = x < w / 2;
    let top = y < h / 2;
    let from_left = |k: u32| x < w * k / 8;
    let from_bottom = |k: u32| y >= h * (8 - k) / 8;
    let shade = |level: u8| match level {
        1 => (x % 2) == 0 && (y % 2) == 0,
        2 => (x + y) % 2 == 0,
        _ => !((x % 2) == 1 && (y % 2) == 1),
    };
    match ch {
        '\u{2580}' => top,
        '\u{2581}' => from_bottom(1),
        '\u{2582}' => from_bottom(2),
        '\u{2583}' => from_bottom(3),
        '\u{2584}' => from_bottom(4),
        '\u{2585}' => from_bottom(5),
        '\u{2586}' => from_bottom(6),
        '\u{2587}' => from_bottom(7),
        '\u{2588}' => true,
        '\u{2589}' => from_left(7),
        '\u{258A}' => from_left(6),
        '\u{258B}' => from_left(5),
        '\u{258C}' => from_left(4),
        '\u{258D}' => from_left(3),
        '\u{258E}' => from_left(2),
        '\u{258F}' => from_left(1),
        '\u{2590}' => !left,
        '\u{2591}' => shade(1),
        '\u{2592}' => shade(2),
        '\u{2593}' => shade(3),
        '\u{2594}' => y < h / 8,
        '\u{2595}' => x >= w * 7 / 8,
        '\u{2596}' => !top && left,
        '\u{2597}' => !top && !left,
        '\u{2598}' => top && left,
        '\u{2599}' => !(top && !left),
        '\u{259A}' => (top && left) || (!top && !left),
        '\u{259B}' => !(!top && !left),
        '\u{259C}' => !(!top && left),
        '\u{259D}' => top && !left,
        '\u{259E}' => (top && !left) || (!top && left),
        '\u{259F}' => !(top && left),
        _ => false,
    }
}

fn is_pua(code: u32) -> bool {
    (0xE000..=0xF8FF).contains(&code)
        || (0xF0000..=0xFFFFD).contains(&code)
        || (0x100000..=0x10FFFD).contains(&code)
}

fn fit_mode(
    ch: char,
    glyph_advance: f32,
    slot_w: f32,
    image: &Image,
    em_w: f32,
    ascent: f32,
    body: f32,
) -> Fit {
    let code = u32::from(ch);
    if (0x2500..=0x257F).contains(&code) || (0x1FB00..=0x1FBFF).contains(&code) {
        return Fit::Em;
    }
    if is_pua(code) {
        if glyph_advance > slot_w * 1.35 {
            return Fit::Uniform;
        }
        if fills_em(image, em_w, ascent, body) {
            return Fit::Em;
        }
        return Fit::Uniform;
    }
    Fit::Baseline
}

fn fills_em(image: &Image, em_w: f32, ascent: f32, body: f32) -> bool {
    if em_w <= 0.0 || body <= 0.0 {
        return false;
    }
    let left = image.placement.left as f32;
    let right = left + image.placement.width as f32;
    let top = ascent - image.placement.top as f32;
    let bottom = top + image.placement.height as f32;
    let pad_x = em_w * 0.12;
    let pad_y = body * 0.12;
    left <= pad_x && right >= em_w - pad_x && top <= pad_y && bottom >= body - pad_y
}

fn paint_em(
    pixels: &mut [u8],
    origin: (u32, u32),
    slot_w: u32,
    slot_h: u32,
    image: &Image,
    em_w: f32,
    body: f32,
    ascent: f32,
) {
    if em_w <= 0.0 || body <= 0.0 || slot_w == 0 || slot_h == 0 {
        return;
    }
    let sw = image.placement.width;
    let sh = image.placement.height;
    let left = image.placement.left as f32;
    let top = ascent - image.placement.top as f32;
    for y in 0..slot_h {
        for x in 0..slot_w {
            let em_x = (x as f32 + 0.5) / slot_w as f32 * em_w;
            let em_y = (y as f32 + 0.5) / slot_h as f32 * body;
            let sample = bilinear(&image.data, sw, sh, em_x - left, em_y - top);
            stamp(pixels, origin, x, y, cover(sample));
        }
    }
}

fn paint_uniform(
    pixels: &mut [u8],
    origin: (u32, u32),
    slot_w: u32,
    slot_h: u32,
    src: &[u8],
    sw: u32,
    sh: u32,
) {
    if sw == 0 || sh == 0 || slot_w == 0 || slot_h == 0 {
        return;
    }
    let scale = (slot_w as f32 / sw as f32).min(slot_h as f32 / sh as f32);
    if scale <= 0.0 {
        return;
    }
    let dw = sw as f32 * scale;
    let dh = sh as f32 * scale;
    let ox = (slot_w as f32 - dw) * 0.5;
    let oy = (slot_h as f32 - dh) * 0.5;
    for y in 0..slot_h {
        for x in 0..slot_w {
            let sx = (x as f32 + 0.5 - ox) / scale - 0.5;
            let sy = (y as f32 + 0.5 - oy) / scale - 0.5;
            stamp(pixels, origin, x, y, cover(bilinear(src, sw, sh, sx, sy)));
        }
    }
}

fn bilinear(src: &[u8], w: u32, h: u32, x: f32, y: f32) -> u8 {
    if w == 0 || h == 0 || x < -1.0 || y < -1.0 || x > w as f32 || y > h as f32 {
        return 0;
    }
    let x0 = x.floor() as i32;
    let y0 = y.floor() as i32;
    let fx = x - x0 as f32;
    let fy = y - y0 as f32;
    let sample = |ix: i32, iy: i32| -> f32 {
        if ix < 0 || iy < 0 || ix >= w as i32 || iy >= h as i32 {
            0.0
        } else {
            f32::from(src[(iy as u32 * w + ix as u32) as usize])
        }
    };
    let v = sample(x0, y0) * (1.0 - fx) * (1.0 - fy)
        + sample(x0 + 1, y0) * fx * (1.0 - fy)
        + sample(x0, y0 + 1) * (1.0 - fx) * fy
        + sample(x0 + 1, y0 + 1) * fx * fy;
    v.round().clamp(0.0, 255.0) as u8
}

#[cfg(target_os = "macos")]
fn bilinear_rgba(src: &[u8], w: u32, h: u32, x: f32, y: f32) -> [u8; 4] {
    if w == 0 || h == 0 || x < -1.0 || y < -1.0 || x > w as f32 || y > h as f32 {
        return [0, 0, 0, 0];
    }
    let x0 = x.floor() as i32;
    let y0 = y.floor() as i32;
    let fx = x - x0 as f32;
    let fy = y - y0 as f32;
    let sample = |ix: i32, iy: i32| -> [f32; 4] {
        if ix < 0 || iy < 0 || ix >= w as i32 || iy >= h as i32 {
            [0.0; 4]
        } else {
            let i = ((iy as u32 * w + ix as u32) * 4) as usize;
            [
                f32::from(src[i]),
                f32::from(src[i + 1]),
                f32::from(src[i + 2]),
                f32::from(src[i + 3]),
            ]
        }
    };
    let mix = |a: f32, b: f32, t: f32| a * (1.0 - t) + b * t;
    let p00 = sample(x0, y0);
    let p10 = sample(x0 + 1, y0);
    let p01 = sample(x0, y0 + 1);
    let p11 = sample(x0 + 1, y0 + 1);
    let mut out = [0u8; 4];
    for channel in 0..4 {
        let v = mix(
            mix(p00[channel], p10[channel], fx),
            mix(p01[channel], p11[channel], fx),
            fy,
        );
        out[channel] = v.round().clamp(0.0, 255.0) as u8;
    }
    out
}

#[cfg(target_os = "macos")]
fn stamp_color(pixels: &mut [u8], origin: (u32, u32), x: u32, y: u32, sample: [u8; 4]) {
    if sample[3] == 0 {
        return;
    }
    let dst = (((origin.1 + y) * ATLAS_SIZE + origin.0 + x) * 4) as usize;
    if dst + 4 <= pixels.len() {
        pixels[dst..dst + 4].copy_from_slice(&sample);
    }
}

fn stamp(pixels: &mut [u8], origin: (u32, u32), x: u32, y: u32, sample: u8) {
    if sample == 0 {
        return;
    }
    let dst = ((origin.1 + y) * ATLAS_SIZE + origin.0 + x) as usize;
    if dst < pixels.len() {
        pixels[dst] = pixels[dst].max(sample);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ink_span(atlas: &Atlas, rect: GlyphRect) -> (u32, u32) {
        let mut top = rect.h;
        let mut bottom = 0u32;
        for row in 0..rect.h {
            let hit = (0..rect.w).any(|col| {
                let i = ((rect.y + row) * ATLAS_SIZE + rect.x + col) as usize;
                atlas.pixels()[i] > 16
            });
            if hit {
                top = top.min(row);
                bottom = bottom.max(row);
            }
        }
        (top, bottom)
    }

    #[test]
    fn letters_have_ink() {
        let atlas = Atlas::new();
        let h = atlas.glyph('H').expect("H");
        let i = atlas.glyph('i').expect("i");
        let g = atlas.glyph('g').expect("g");
        assert!(h.w >= 8 && i.h >= 16);
        assert!(atlas.glyph(' ').is_none());
        let partial = atlas
            .pixels()
            .iter()
            .filter(|p| **p > 8 && **p < 247)
            .count();
        let covered = atlas.pixels().iter().filter(|p| **p > 128).count();
        assert!(partial > 20, "antialiased coverage {partial}");
        assert!(covered > 100, "ink {covered}");
        let (h_top, h_bot) = ink_span(&atlas, h);
        let (_, g_bot) = ink_span(&atlas, g);
        assert!(h_top > 0, "H starts at {h_top}");
        assert!(g_bot > h_bot, "descender {g_bot} vs cap {h_bot}");
        assert!(g_bot + 1 < g.h, "g clipped at {g_bot} of {}", g.h);
        let (w, h) = face_cell();
        assert!(w >= 8.0 && h >= 16.0, "cascadia cell {w}x{h}");
    }

    fn px(atlas: &Atlas, rect: GlyphRect, x: u32, y: u32) -> u8 {
        atlas.pixels()[((rect.y + y) * ATLAS_SIZE + rect.x + x) as usize]
    }

    fn ink_count(atlas: &Atlas, rect: GlyphRect) -> usize {
        let mut n = 0;
        for y in 0..rect.h {
            for x in 0..rect.w {
                if px(atlas, rect, x, y) > 16 {
                    n += 1;
                }
            }
        }
        n
    }

    #[test]
    fn ascii_ensure_reuses_the_cell_and_does_not_upload() {
        let mut atlas = Atlas::new();
        let h = atlas.glyph('H');
        assert_eq!(atlas.ensure('H', 1), h);
        assert!(!atlas.take_dirty());
    }

    #[test]
    fn block_rules_and_icons_cover_the_cell() {
        let mut atlas = Atlas::with_cell(9, 17);
        let (cw, ch) = atlas.cell();
        let block = atlas.ensure('\u{2588}', 1).expect("U+2588");
        assert_eq!((block.w, block.h), (cw, ch));
        for (x, y) in [
            (0, 0),
            (cw - 1, 0),
            (0, ch - 1),
            (cw - 1, ch - 1),
            (cw / 2, ch / 2),
        ] {
            let v = px(&atlas, block, x, y);
            assert!(v > 200, "full block {x},{y} = {v}");
        }
        let upper = atlas.ensure('\u{2580}', 1).expect("U+2580");
        let lower = atlas.ensure('\u{2584}', 1).expect("U+2584");
        assert!(px(&atlas, upper, cw / 2, 0) > 200, "upper half top");
        assert!(px(&atlas, upper, cw / 2, ch - 1) < 16, "upper half bottom");
        assert!(px(&atlas, lower, cw / 2, ch - 1) > 200, "lower half bottom");
        assert!(px(&atlas, lower, cw / 2, 0) < 16, "lower half top");
        let rule = atlas.ensure('\u{2500}', 1).expect("U+2500");
        let mut best = (0u32, 0u8);
        for y in 0..rule.h {
            let left = px(&atlas, rule, 0, y);
            let right = px(&atlas, rule, rule.w - 1, y);
            let peak = left.max(right);
            if peak > best.1 {
                best = (y, peak);
            }
        }
        assert!(best.1 > 128, "horizontal rule edges {}", best.1);
        assert!(px(&atlas, rule, 0, 0) < 32 && px(&atlas, rule, 0, rule.h - 1) < 32);
        let bar = atlas.ensure('\u{2502}', 1).expect("U+2502");
        let mut bar_best = 0u8;
        for x in 0..bar.w {
            bar_best = bar_best.max(px(&atlas, bar, x, 0).max(px(&atlas, bar, x, bar.h - 1)));
        }
        assert!(bar_best > 128, "vertical rule ends {bar_best}");
        assert!(px(&atlas, bar, 0, bar.h / 2) < 32 && px(&atlas, bar, bar.w - 1, bar.h / 2) < 32);
        let icon = atlas.ensure('\u{E0B0}', 1).expect("U+E0B0");
        assert!(ink_count(&atlas, icon) > 8, "powerline ink");
        let reaches = (0..icon.h).any(|y| px(&atlas, icon, 0, y) > 128)
            || (0..icon.w).any(|x| px(&atlas, icon, x, 0) > 128);
        assert!(reaches, "powerline reaches a cell edge");
        let mark = atlas.ensure('\u{F09B}', 1).expect("U+F09B");
        assert!(ink_count(&atlas, mark) > 8, "nerd icon ink");
        let wide = atlas.ensure('\u{2588}', 2).expect("wide block");
        assert_eq!(wide.w, cw * 2);
        assert!(px(&atlas, wide, 0, 0) > 200 && px(&atlas, wide, wide.w - 1, ch - 1) > 200);
        assert!(atlas.take_dirty());
        assert_eq!(atlas.ensure('\u{2588}', 1), Some(block));
        assert!(!atlas.take_dirty());
    }

    #[test]
    fn a_missing_codepoint_is_cached_without_a_bitmap() {
        let mut atlas = Atlas::new();
        assert!(atlas.ensure('\u{0378}', 1).is_none());
        assert!(!atlas.take_dirty());
        assert!(atlas.ensure('\u{0378}', 1).is_none());
    }

    #[test]
    fn the_prompt_mark_sits_in_the_cell() {
        let mut atlas = Atlas::with_cell(18, 34);
        let mark = atlas.ensure('\u{276F}', 1).expect("chevron");
        assert_eq!(mark.atlas, 0);
        assert!(ink_count(&atlas, mark) > 16, "chevron ink");
        assert!(atlas.take_dirty());
        assert_eq!(atlas.ensure('\u{276F}', 1), Some(mark));
        assert!(!atlas.take_dirty());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn emoji_comes_from_the_system_cascade() {
        let mut atlas = Atlas::with_cell(18, 34);
        let snake = atlas.ensure('🐍', 2).expect("snake");
        assert_eq!(snake.atlas, 1);
        assert_eq!((snake.w, snake.h), (36, 34));
        let pixels = atlas.color_pixels().expect("color atlas");
        let mut alpha = 0u8;
        for y in 0..snake.h {
            for x in 0..snake.w {
                let i = (((snake.y + y) * ATLAS_SIZE + snake.x + x) * 4) as usize;
                alpha = alpha.max(pixels[i + 3]);
            }
        }
        assert!(alpha > 16, "emoji alpha {alpha}");
        assert!(atlas.take_color_dirty());
        assert!(!atlas.take_dirty());
        assert_eq!(atlas.ensure('🐍', 2), Some(snake));
        assert!(!atlas.take_color_dirty());
    }

    #[test]
    fn latin_beyond_ascii_stays_on_the_baseline() {
        let mut atlas = Atlas::new();
        let h = atlas.glyph('H').expect("H");
        let aring = atlas.ensure('å', 1).expect("å");
        let (_, h_bot) = ink_span(&atlas, h);
        let (_, a_bot) = ink_span(&atlas, aring);
        assert!(
            (a_bot as i32 - h_bot as i32).abs() <= 2,
            "å {a_bot} vs H {h_bot}"
        );
        assert!(a_bot + 1 < aring.h, "å stretched to {}", aring.h);
    }
}
