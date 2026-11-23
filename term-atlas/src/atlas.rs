//! One 2048² gray atlas, plus a color atlas for system emoji.
//! The face is the profile's family, Maple Mono NF by default. ASCII is rasterized up
//! front. Every other covered codepoint is rasterized on demand into the
//! same cell. A codepoint the face does not cover is drawn from the compact
//! bundled fallback, then by the macOS cascade. The faces are OFL; see
//! `LICENSE-OFL.txt` in the `term-font-data-*` crates.

use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};

use swash::scale::image::Image;
use swash::scale::{Render, ScaleContext, Source};
use swash::{FontRef, NormalizedCoord};

use crate::face::Face;

pub const ATLAS_SIZE: u32 = 2048;

/// Colour glyphs are sparse, so their atlas is a quarter of the gray one.
pub const COLOR_ATLAS_SIZE: u32 = 1024;

/// Cap on cached misses, so a stream of novel codepoints cannot grow the
/// lookup tables without bound.
const MAX_MISSES: usize = 4096;

/// Rio's default font size, in points.
const DEFAULT_FACE_PX: f32 = 14.0;

/// Cell pitch of the bundled face at 14 points.
pub fn face_cell() -> (f32, f32) {
    face_cell_for(&Face::bundled(), DEFAULT_FACE_PX)
}

/// Cell pitch of `face` at `font_size` points, in points. Width is the
/// ASCII advance. Height is Rio's line box: ascent, descent, and twice the
/// line gap. Icon glyphs in the Nerd Font file are wider than that advance
/// and do not set the cell.
pub fn face_cell_for(face: &Face, font_size: f32) -> (f32, f32) {
    let font_size = if font_size.is_finite() && font_size > 0.0 {
        font_size
    } else {
        DEFAULT_FACE_PX
    };
    let Some(font) = face.font() else {
        let scale = font_size / DEFAULT_FACE_PX;
        return (
            (8.0 * scale).ceil().max(1.0),
            (16.0 * scale).ceil().max(1.0),
        );
    };
    let coords = regular_coords(&font);
    let metrics = font.metrics(&coords).scale(font_size);
    let advance = font
        .glyph_metrics(&coords)
        .scale(font_size)
        .advance_width(font.charmap().map('0'));
    let width = if advance > 0.0 {
        advance
    } else {
        metrics.max_width
    };
    let body = metrics.ascent.abs() + metrics.descent.abs();
    let height = (body + metrics.leading * 2.0).max(body);
    (width.ceil().max(1.0), height.ceil().max(1.0))
}

fn regular_coords(font: &FontRef<'_>) -> Vec<NormalizedCoord> {
    let mut out = Vec::new();
    regular_coords_into(font, &mut out);
    out
}

fn regular_coords_into(font: &FontRef<'_>, out: &mut Vec<NormalizedCoord>) {
    out.clear();
    out.extend(font.variations().normalized_coords([("wght", 400.0)]));
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

/// Byte order of the premultiplied color pixels a caller reads from the atlas.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PixelFormat {
    /// Bytes `R, G, B, A` — what wgpu textures take.
    #[default]
    Rgba,
    /// Bytes `B, G, R, A` — an ARGB8888 `u32` in little-endian memory, as
    /// softbuffer's frame buffer stores it.
    Argb,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct GlyphKey {
    code: u32,
    cells: u8,
}

/// Metrics a fallback face is rasterized with, relative to the active cell.
struct FallbackScale {
    ppem: f32,
    glyph_advance: f32,
    coords: Vec<NormalizedCoord>,
}

/// Past every char, so the underline's slot never collides with a glyph's.
const UNDERLINE: u32 = u32::MAX;

pub struct Atlas {
    pixels: Vec<u8>,
    ascii: [Option<GlyphRect>; 95],
    extra: HashMap<GlyphKey, Option<GlyphRect>>,
    /// Grapheme clusters that span more than one codepoint, keyed by a hash
    /// of the text and the cell count. The text is kept to rule out a hash
    /// collision returning another cluster's glyph.
    clusters: HashMap<u64, (Box<str>, Option<GlyphRect>)>,
    /// Glyphs warm-up leaves to `ensure`: color images and CoreText draws.
    warm_skipped: HashSet<GlyphKey>,
    /// Scratch buffers reused by cluster rasterization, so a miss does not
    /// allocate a fresh slot bitmap and coordinate list each time.
    cluster_color: Vec<u8>,
    cluster_gray: Vec<u8>,
    cluster_coords: Vec<NormalizedCoord>,
    /// Whether fallback outlines are autohinted; the profile face always is.
    hint_fallback: bool,
    /// Cached misses in `extra` and `clusters`.
    misses: usize,
    /// The next free slot in each atlas, and where a reset starts again.
    color_cursor: u32,
    color_cols: u32,
    reset_cursor: u32,
    reset_color_cursor: u32,
    /// Set when an atlas ran out of room; the caller rebuilds on the next frame.
    needs_reset: bool,
    cell: (u32, u32),
    /// Cell columns in the atlas. Dynamic glyphs pack after the ASCII rows.
    cols: u32,
    cursor: u32,
    /// Set when `extra` gains a gray bitmap. The renderer uploads once, then clears it.
    dirty: bool,
    /// Premultiplied color pixels in `format`. Empty until the first color glyph.
    color: Vec<u8>,
    color_dirty: bool,
    format: PixelFormat,
    context: ScaleContext,
    coords: Vec<NormalizedCoord>,
    ppem: f32,
    baseline: f32,
    pen_x: f32,
    advance_px: f32,
    ascent: f32,
    body: f32,
    upem: f32,
    /// The underline stroke's top, from the cell's top, and its thickness, in pixels.
    underline: (f32, f32),
    has_font: bool,
    face: Face,
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
        Self::with_face(Face::bundled(), cell_w, cell_h)
    }

    /// [`Self::with_cell`] in `face`, with the default RGBA color bytes.
    pub fn with_face(face: Face, cell_w: u32, cell_h: u32) -> Self {
        Self::with_face_format(face, cell_w, cell_h, PixelFormat::default())
    }

    /// Like [`Self::with_face`], with the byte order the caller's frame buffer wants.
    pub fn with_face_format(face: Face, cell_w: u32, cell_h: u32, format: PixelFormat) -> Self {
        let cell_w = cell_w.max(1);
        let cell_h = cell_h.max(1);
        let mut pixels = vec![0u8; (ATLAS_SIZE * ATLAS_SIZE) as usize];
        let mut ascii = [None; 95];
        let cols = (ATLAS_SIZE / cell_w).max(1);
        let Some(font) = face.font() else {
            return Self::empty(pixels, ascii, (cell_w, cell_h), cols, format);
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
        let underline = (baseline - scaled.underline_offset, scaled.stroke_size);
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
        let _ = scaler;
        let ascii_rows = 95u32.div_ceil(cols);
        Self {
            pixels,
            ascii,
            extra: HashMap::new(),
            clusters: HashMap::new(),
            warm_skipped: HashSet::new(),
            cluster_color: Vec::new(),
            cluster_gray: Vec::new(),
            cluster_coords: Vec::new(),
            misses: 0,
            hint_fallback: true,
            color_cursor: 0,
            color_cols: (COLOR_ATLAS_SIZE / cell_w).max(1),
            reset_cursor: ascii_rows.max(2) * cols,
            reset_color_cursor: 0,
            needs_reset: false,
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
            underline,
            has_font: true,
            format,
            face,
        }
    }

    fn empty(
        pixels: Vec<u8>,
        ascii: [Option<GlyphRect>; 95],
        cell: (u32, u32),
        cols: u32,
        format: PixelFormat,
    ) -> Self {
        Self {
            pixels,
            ascii,
            extra: HashMap::new(),
            clusters: HashMap::new(),
            warm_skipped: HashSet::new(),
            cluster_color: Vec::new(),
            cluster_gray: Vec::new(),
            cluster_coords: Vec::new(),
            misses: 0,
            hint_fallback: true,
            color_cursor: 0,
            color_cols: (COLOR_ATLAS_SIZE / cell.0).max(1),
            reset_cursor: 0,
            reset_color_cursor: 0,
            needs_reset: false,
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
            underline: (cell.1 as f32 - 2.0, 1.0),
            has_font: false,
            format,
            face: Face::bundled(),
        }
    }

    pub fn face(&self) -> &Face {
        &self.face
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

    /// Whether fallback glyphs are autohinted. The profile face is unaffected.
    pub fn set_hint_fallback(&mut self, enabled: bool) {
        self.hint_fallback = enabled;
    }

    /// Rasterize the outlines of `keys` that are not cached yet, in parallel.
    /// Slot allocation and painting stay on the calling thread, so rects and
    /// atlas contents remain deterministic. Color and CoreText glyphs are
    /// left to [`Self::ensure`].
    pub fn warm_glyphs(&mut self, keys: impl IntoIterator<Item = (char, u8)>) {
        if !self.has_font || self.ppem <= 0.0 {
            return;
        }
        let ppem = self.ppem;
        let mut work: Vec<(GlyphKey, char, fontique::Blob<u8>, u32, u16)> = Vec::new();
        for (ch, cells) in keys {
            let cells = if cells >= 2 { 2 } else { 1 };
            let code = u32::from(ch);
            if (cells == 1 && (0x20..0x7f).contains(&code)) || crate::sprite::covers(ch) {
                continue;
            }
            let key = GlyphKey { code, cells };
            if self.extra.contains_key(&key) || self.warm_skipped.contains(&key) {
                continue;
            }
            let face_gid = self
                .face
                .font()
                .map(|font| font.charmap().map(ch))
                .unwrap_or(0);
            let (blob, index, gid) = if face_gid != 0 {
                (self.face.data().clone(), self.face.index(), face_gid)
            } else if let Some((blob, index, gid)) =
                crate::fallback_bytes().iter().find_map(|blob| {
                    let font = FontRef::from_index(blob.data(), 0)?;
                    let gid = font.charmap().map(ch);
                    (gid != 0).then_some((blob.clone(), 0, gid))
                })
            {
                (blob, index, gid)
            } else {
                let Some(found) = term_font::discover(self.face.family(), ch) else {
                    continue;
                };
                let Some(font) = FontRef::from_index(found.data.data(), found.index as usize)
                else {
                    continue;
                };
                let gid = font.charmap().map(ch);
                if gid == 0 {
                    self.warm_skipped.insert(key);
                    continue;
                }
                if term_font::color::rasterize(
                    found.data.data(),
                    found.index,
                    u32::from(gid),
                    ppem,
                    &[],
                )
                .is_some()
                    || term_font::color::rasterize_bitmap(found.data.data(), found.index, gid, ppem)
                        .is_some()
                {
                    self.warm_skipped.insert(key);
                    continue;
                }
                (found.data.clone(), found.index, gid)
            };
            work.push((key, ch, blob, index, gid));
        }
        work.sort_by_key(|(key, _, _, _, _)| (key.code, key.cells));
        work.dedup_by_key(|(key, _, _, _, _)| *key);
        if work.is_empty() {
            return;
        }
        let threads = std::thread::available_parallelism()
            .map_or(1, |n| n.get())
            .min(16)
            .min(work.len());
        if threads < 2 {
            return;
        }
        let hint_fallback = self.hint_fallback;
        let chunk_size = work.len().div_ceil(threads);
        let mut rendered: Vec<(usize, GlyphKey, char, Image, f32)> = Vec::new();
        std::thread::scope(|scope| {
            let mut handles = Vec::new();
            for (part, chunk) in work.chunks(chunk_size).enumerate() {
                handles.push(scope.spawn(move || {
                    let mut context = ScaleContext::new();
                    let mut out = Vec::new();
                    for (ix, (key, ch, blob, index, gid)) in chunk.iter().enumerate() {
                        let Some(font) = FontRef::from_index(blob.data(), *index as usize) else {
                            continue;
                        };
                        let coords = regular_coords(&font);
                        let upem = f32::from(font.metrics(&coords).units_per_em.max(1)).max(1.0);
                        let advance = font.glyph_metrics(&coords).advance_width(*gid) * ppem / upem;
                        let mut scaler = context
                            .builder(font)
                            .size(ppem)
                            .hint(hint_fallback)
                            .normalized_coords(coords.iter().copied())
                            .build();
                        if let Some(image) =
                            Render::new(&[Source::Outline]).render(&mut scaler, *gid)
                        {
                            out.push((part * chunk_size + ix, *key, *ch, image, advance));
                        }
                    }
                    out
                }));
            }
            for handle in handles {
                if let Ok(mut part) = handle.join() {
                    rendered.append(&mut part);
                }
            }
        });
        rendered.sort_by_key(|item| item.0);
        for (_, key, ch, image, advance) in rendered {
            if let Some(rect) = self.stamp_outline(ch, key.cells, image, advance) {
                if rect.atlas == 0 {
                    self.dirty = true;
                }
                self.extra.insert(key, Some(rect));
            } else if self.misses < MAX_MISSES {
                self.misses += 1;
                self.extra.insert(key, None);
            }
        }
    }

    /// Rasterize `ch` into a slot `cells` wide (1 or 2). ASCII is already
    /// placed. Misses are cached up to a bound, so a stream of novel
    /// codepoints cannot grow the table without limit.
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
        if let Some(rect) = crate::sprite::raster(self, ch, cells) {
            self.extra.insert(key, Some(rect));
            return Some(rect);
        }
        let rect = self.rasterize(ch, cells);
        if rect.is_some_and(|rect| rect.atlas == 0) {
            self.dirty = true;
        }
        if rect.is_some() || self.misses < MAX_MISSES {
            if rect.is_none() {
                self.misses += 1;
            }
            self.extra.insert(key, rect);
        }
        rect
    }

    /// Rasterize a grapheme cluster (a flag, ZWJ emoji, a modifier) into a
    /// slot `cells` wide. A single codepoint takes the ordinary path. The
    /// text is cached alongside its hash so a collision cannot reuse another
    /// cluster's glyph.
    pub fn ensure_str(&mut self, text: &str, cells: u8) -> Option<GlyphRect> {
        let cells = if cells >= 2 { 2 } else { 1 };
        let mut chars = text.chars();
        let first = chars.next()?;
        if chars.next().is_none() {
            return self.ensure(first, cells);
        }
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        text.hash(&mut hasher);
        cells.hash(&mut hasher);
        let key = hasher.finish();
        if let Some((known, hit)) = self.clusters.get(&key) {
            if known.as_ref() == text {
                return *hit;
            }
        }
        let rect = self.rasterize_cluster(text, cells);
        if rect.is_some() || self.misses < MAX_MISSES {
            if rect.is_none() {
                self.misses += 1;
            }
            self.clusters.insert(key, (text.into(), rect));
        }
        rect
    }

    /// Shape a cluster with rustybuzz, then composite every glyph it produces
    /// into one slot. A keycap can shape to a digit plus an enclosing ring;
    /// drawing only the first glyph loses the ring.
    fn rasterize_cluster(&mut self, text: &str, cells: u8) -> Option<GlyphRect> {
        let (blob, index) = crate::face::emoji_font()?;
        self.rasterize_cluster_with(text, cells, blob.data(), *index)
    }

    fn rasterize_cluster_with(
        &mut self,
        text: &str,
        cells: u8,
        bytes: &[u8],
        index: u32,
    ) -> Option<GlyphRect> {
        if !self.has_font || self.ppem <= 0.0 {
            return None;
        }
        let face = rustybuzz::Face::from_slice(bytes, index)?;
        let mut buffer = rustybuzz::UnicodeBuffer::new();
        buffer.push_str(text);
        let shaped = rustybuzz::shape(&face, &[], buffer);
        let font = swash::FontRef::from_index(bytes, index as usize)?;
        let mut coords = std::mem::take(&mut self.cluster_coords);
        regular_coords_into(&font, &mut coords);
        let upem = f32::from(font.metrics(&coords).units_per_em.max(1));
        let scale = self.ppem / upem;
        let slot_w = u32::from(cells.max(1)) * self.cell.0;
        let slot_h = self.cell.1;
        let mut color = std::mem::take(&mut self.cluster_color);
        color.clear();
        color.resize((slot_w * slot_h * 4) as usize, 0);
        let mut gray = std::mem::take(&mut self.cluster_gray);
        gray.clear();
        gray.resize((slot_w * slot_h) as usize, 0);
        let mut has_color = false;
        let mut has_ink = false;
        let mut pen = 0.0f32;
        for (info, pos) in shaped.glyph_infos().iter().zip(shaped.glyph_positions()) {
            let advance = pos.x_advance as f32 * scale;
            let x = pen + pos.x_offset as f32 * scale;
            let y = pos.y_offset as f32 * scale;
            pen += advance;
            let Ok(gid) = u16::try_from(info.glyph_id) else {
                continue;
            };
            if gid == 0 {
                continue;
            }
            if let Some((image, place)) =
                term_font::color::rasterize(bytes, index, u32::from(gid), self.ppem, &[])
            {
                blit_rgba(
                    &mut color,
                    (slot_w, slot_h),
                    &image.rgba,
                    (image.width, image.height),
                    (x + place.left as f32, self.baseline - place.top as f32 - y),
                );
                has_color = true;
                has_ink = true;
                continue;
            }
            if let Some((image, place)) =
                term_font::color::rasterize_bitmap(bytes, index, gid, self.ppem)
            {
                blit_rgba(
                    &mut color,
                    (slot_w, slot_h),
                    &image.rgba,
                    (image.width, image.height),
                    (x + place.left as f32, self.baseline - place.top as f32 - y),
                );
                has_color = true;
                has_ink = true;
                continue;
            }
            let mut scaler = self
                .context
                .builder(font)
                .size(self.ppem)
                .normalized_coords(coords.iter().copied())
                .build();
            let Some(image) = Render::new(&[Source::Outline]).render(&mut scaler, gid) else {
                continue;
            };
            blit_gray(
                &mut gray,
                (slot_w, slot_h),
                &image.data,
                (image.placement.width, image.placement.height),
                (
                    x + image.placement.left as f32,
                    self.baseline - image.placement.top as f32 - y,
                ),
            );
            has_ink = true;
        }
        let result = if !has_ink {
            None
        } else if has_color {
            self.alloc_color(cells).map(|origin| {
                if self.color.is_empty() {
                    self.color = vec![0u8; (COLOR_ATLAS_SIZE * COLOR_ATLAS_SIZE * 4) as usize];
                }
                for row in 0..slot_h {
                    for col in 0..slot_w {
                        let i = ((row * slot_w + col) * 4) as usize;
                        let sample = [color[i], color[i + 1], color[i + 2], color[i + 3]];
                        let sample = self.to_format(sample);
                        stamp_color(&mut self.color, origin, col, row, sample);
                    }
                }
                self.color_dirty = true;
                GlyphRect {
                    x: origin.0,
                    y: origin.1,
                    w: slot_w,
                    h: slot_h,
                    atlas: 1,
                }
            })
        } else {
            self.alloc(cells).map(|origin| {
                for row in 0..slot_h {
                    for col in 0..slot_w {
                        stamp(
                            &mut self.pixels,
                            origin,
                            col,
                            row,
                            gray[(row * slot_w + col) as usize],
                        );
                    }
                }
                self.dirty = true;
                GlyphRect {
                    x: origin.0,
                    y: origin.1,
                    w: slot_w,
                    h: slot_h,
                    atlas: 0,
                }
            })
        };
        self.cluster_color = color;
        self.cluster_gray = gray;
        self.cluster_coords = coords;
        result
    }

    /// A stroke `cells` wide (1 or 2) where the face puts its underline, drawn under a cell.
    pub fn underline(&mut self, cells: u8) -> Option<GlyphRect> {
        let cells = if cells >= 2 { 2 } else { 1 };
        let key = GlyphKey {
            code: UNDERLINE,
            cells,
        };
        if let Some(hit) = self.extra.get(&key) {
            return *hit;
        }
        let rect = self.alloc(cells).map(|at| {
            let (w, h) = (self.cell.0 * u32::from(cells), self.cell.1);
            let thick = (self.underline.1.round() as u32).clamp(1, h);
            let top = (self.underline.0.round().max(0.0) as u32).min(h - thick);
            let mut bitmap = vec![0u8; (w * h) as usize];
            bitmap[(top * w) as usize..((top + thick) * w) as usize].fill(255);
            self.insert(&bitmap, w, h, at)
        });
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
        let face = self.face.clone();
        let font = face.font()?;
        let gid = font.charmap().map(ch);
        if gid == 0 {
            return self.place_fallback(ch, cells);
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
        let _ = scaler;
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

    /// A codepoint the face lacks: try the compact bundled subset, then a
    /// system font found by `term-font`. On macOS CoreText is the last resort.
    fn place_fallback(&mut self, ch: char, cells: u8) -> Option<GlyphRect> {
        for blob in crate::fallback_bytes() {
            let Some(font) = swash::FontRef::from_index(blob.data(), 0) else {
                continue;
            };
            let gid = font.charmap().map(ch);
            if gid == 0 {
                continue;
            }
            let coords = regular_coords(&font);
            let upem = f32::from(font.metrics(&coords).units_per_em.max(1));
            // ppem is pixels per em at the chosen size; upem only converts
            // the fallback face's advance units into pixels.
            let ppem = self.ppem;
            let advance = font.glyph_metrics(&coords).advance_width(gid) * ppem / upem;
            return self.place_face_glyph(
                font,
                gid,
                ch,
                cells,
                FallbackScale {
                    ppem,
                    glyph_advance: advance,
                    coords,
                },
            );
        }
        if let Some(rect) = self.place_system_font(ch, cells) {
            return Some(rect);
        }
        #[cfg(all(target_os = "macos", feature = "bundled-font"))]
        {
            self.place_system(ch, cells)
        }
        #[cfg(not(all(target_os = "macos", feature = "bundled-font")))]
        {
            None
        }
    }

    /// A system font covering `ch`, rasterized with swash or its color image.
    fn place_system_font(&mut self, ch: char, cells: u8) -> Option<GlyphRect> {
        let found = term_font::discover(self.face.family(), ch)?;
        let bytes: &[u8] = found.data.as_ref();
        let font = swash::FontRef::from_index(bytes, found.index as usize)?;
        let gid = font.charmap().map(ch);
        if gid == 0 {
            return None;
        }
        let coords = regular_coords(&font);
        let upem = f32::from(font.metrics(&coords).units_per_em.max(1));
        // ppem is pixels per em at the chosen size; upem only converts
        // the fallback face's advance units into pixels.
        let ppem = self.ppem;
        let advance = font.glyph_metrics(&coords).advance_width(gid) * ppem / upem;
        if let Some((image, _)) =
            term_font::color::rasterize(bytes, found.index, u32::from(gid), ppem, &[])
        {
            return self.place_color_image(cells, image);
        }
        if let Some((image, _)) = term_font::color::rasterize_bitmap(bytes, found.index, gid, ppem)
        {
            return self.place_color_image(cells, image);
        }
        self.place_face_glyph(
            font,
            gid,
            ch,
            cells,
            FallbackScale {
                ppem,
                glyph_advance: advance,
                coords,
            },
        )
    }

    /// Rasterize `gid` from a fallback face into a cell.
    fn place_face_glyph(
        &mut self,
        font: FontRef<'_>,
        gid: u16,
        ch: char,
        cells: u8,
        scale: FallbackScale,
    ) -> Option<GlyphRect> {
        if !self.has_font || self.ppem <= 0.0 {
            return None;
        }
        let mut scaler = self
            .context
            .builder(font)
            .size(scale.ppem)
            .hint(self.hint_fallback)
            .normalized_coords(scale.coords.iter().copied())
            .build();
        let image = Render::new(&[Source::Outline]).render(&mut scaler, gid);
        let _ = scaler;
        let image = image?;
        self.stamp_outline(ch, cells, image, scale.glyph_advance)
    }

    /// Allocate a slot and paint an already rasterized outline into it.
    fn stamp_outline(
        &mut self,
        ch: char,
        cells: u8,
        image: Image,
        glyph_advance: f32,
    ) -> Option<GlyphRect> {
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
        let mode = fit_mode(
            ch,
            glyph_advance,
            slot_w as f32,
            &image,
            em_w,
            self.ascent,
            self.body,
        );
        let pixels = &mut self.pixels;
        match mode {
            Fit::Em => paint_em(
                pixels,
                origin,
                slot_w,
                slot_h,
                &image,
                em_w,
                self.body,
                self.ascent,
            ),
            Fit::Uniform => paint_uniform(pixels, origin, slot_w, slot_h, &image.data, gw, gh),
            Fit::Baseline => {
                let left = self.pen_x + image.placement.left as f32;
                let top = self.baseline - image.placement.top as f32;
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

    #[cfg(all(target_os = "macos", feature = "bundled-font"))]
    fn place_system(&mut self, ch: char, cells: u8) -> Option<GlyphRect> {
        let drawn = crate::fallback::draw(
            crate::face::bundled_data(),
            ch,
            self.ppem.max(self.cell.1 as f32),
        )?;
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
        Some(GlyphRect {
            x: origin.0,
            y: origin.1,
            w: slot_w,
            h: slot_h,
            atlas: 0,
        })
    }

    #[cfg(all(target_os = "macos", feature = "bundled-font"))]
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

    #[cfg(all(target_os = "macos", feature = "bundled-font"))]
    fn paint_color(
        &mut self,
        origin: (u32, u32),
        slot_w: u32,
        slot_h: u32,
        drawn: &crate::fallback::Drawn,
    ) {
        self.paint_color_image(
            origin,
            slot_w,
            slot_h,
            drawn.width,
            drawn.height,
            &drawn.bytes,
        );
    }

    fn place_color_image(
        &mut self,
        cells: u8,
        image: term_font::color::RgbaImage,
    ) -> Option<GlyphRect> {
        let origin = self.alloc_color(cells)?;
        let slot_w = u32::from(cells) * self.cell.0;
        let slot_h = self.cell.1;
        self.paint_color_image(
            origin,
            slot_w,
            slot_h,
            image.width,
            image.height,
            &image.rgba,
        );
        self.color_dirty = true;
        Some(GlyphRect {
            x: origin.0,
            y: origin.1,
            w: slot_w,
            h: slot_h,
            atlas: 1,
        })
    }

    /// Scale premultiplied RGBA into a slot in the color atlas.
    fn paint_color_image(
        &mut self,
        origin: (u32, u32),
        slot_w: u32,
        slot_h: u32,
        width: u32,
        height: u32,
        rgba: &[u8],
    ) {
        if self.color.is_empty() {
            self.color = vec![0u8; (COLOR_ATLAS_SIZE * COLOR_ATLAS_SIZE * 4) as usize];
        }
        if width == 0 || height == 0 || slot_w == 0 || slot_h == 0 {
            return;
        }
        let scale = (slot_w as f32 / width as f32).min(slot_h as f32 / height as f32);
        if scale <= 0.0 {
            return;
        }
        let dw = width as f32 * scale;
        let dh = height as f32 * scale;
        let ox = (slot_w as f32 - dw) * 0.5;
        let oy = (slot_h as f32 - dh) * 0.5;
        for y in 0..slot_h {
            for x in 0..slot_w {
                let sx = (x as f32 + 0.5 - ox) / scale - 0.5;
                let sy = (y as f32 + 0.5 - oy) / scale - 0.5;
                let sample = bilinear_rgba(rgba, width, height, sx, sy);
                let sample = self.to_format(sample);
                stamp_color(&mut self.color, origin, x, y, sample);
            }
        }
    }

    /// Reorder premultiplied RGBA into the caller's chosen byte order.
    fn to_format(&self, sample: [u8; 4]) -> [u8; 4] {
        match self.format {
            PixelFormat::Rgba => sample,
            PixelFormat::Argb => [sample[2], sample[1], sample[0], sample[3]],
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

    fn alloc_impl(&mut self, cells: u8, color: bool) -> Option<(u32, u32)> {
        let need = u32::from(cells.max(1));
        let (size, cols, mut cursor) = if color {
            (COLOR_ATLAS_SIZE, self.color_cols, self.color_cursor)
        } else {
            (ATLAS_SIZE, self.cols, self.cursor)
        };
        let cols = cols.max(1);
        let (cell_w, cell_h) = self.cell;
        for _ in 0..cols + 2 {
            let col = cursor % cols;
            let row = cursor / cols;
            if col + need > cols {
                cursor = row.saturating_add(1).saturating_mul(cols);
                continue;
            }
            let origin_x = col * cell_w;
            let origin_y = row * cell_h;
            if origin_y + cell_h > size || origin_x + need * cell_w > size {
                return None;
            }
            cursor += need;
            if color {
                self.color_cursor = cursor;
            } else {
                self.cursor = cursor;
            }
            return Some((origin_x, origin_y));
        }
        None
    }

    pub(crate) fn alloc(&mut self, cells: u8) -> Option<(u32, u32)> {
        if let Some(at) = self.alloc_impl(cells, false) {
            return Some(at);
        }
        self.needs_reset = true;
        None
    }

    fn alloc_color(&mut self, cells: u8) -> Option<(u32, u32)> {
        if let Some(at) = self.alloc_impl(cells, true) {
            return Some(at);
        }
        self.needs_reset = true;
        None
    }

    /// Whether the caller should rebuild the caches at a frame boundary.
    pub fn needs_reset(&self) -> bool {
        self.needs_reset
    }

    /// Drop every cached glyph and start packing both atlases again. The
    /// caller forces a full re-upload on the same frame.
    pub fn reclaim(&mut self) {
        self.extra.clear();
        self.clusters.clear();
        self.misses = 0;
        self.cursor = self.reset_cursor;
        self.color_cursor = self.reset_color_cursor;
        self.needs_reset = false;
        self.dirty = true;
        self.color_dirty = true;
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

#[allow(clippy::too_many_arguments)]
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
        1 => x.is_multiple_of(2) && y.is_multiple_of(2),
        2 => (x + y).is_multiple_of(2),
        _ => !(x % 2 == 1 && y % 2 == 1),
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

#[allow(clippy::too_many_arguments)]
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

fn blit_rgba(dst: &mut [u8], size: (u32, u32), src: &[u8], src_size: (u32, u32), at: (f32, f32)) {
    let (dw, dh) = size;
    let (sw, sh) = src_size;
    let (x, y) = at;
    if sw == 0 || sh == 0 {
        return;
    }
    for row in 0..sh {
        for col in 0..sw {
            let px = (x + col as f32).round() as i32;
            let py = (y + row as f32).round() as i32;
            if px < 0 || py < 0 || px >= dw as i32 || py >= dh as i32 {
                continue;
            }
            let si = ((row * sw + col) * 4) as usize;
            if si + 4 > src.len() {
                continue;
            }
            let di = ((py as u32 * dw + px as u32) * 4) as usize;
            let a = u16::from(src[si + 3]);
            let inv = 255 - a;
            for c in 0..3 {
                let v = u16::from(src[si + c]) + ((u16::from(dst[di + c]) * inv + 127) / 255);
                dst[di + c] = v.min(255) as u8;
            }
            let out = a + ((u16::from(dst[di + 3]) * inv + 127) / 255);
            dst[di + 3] = out.min(255) as u8;
        }
    }
}

fn blit_gray(dst: &mut [u8], size: (u32, u32), src: &[u8], src_size: (u32, u32), at: (f32, f32)) {
    let (dw, dh) = size;
    let (sw, sh) = src_size;
    let (x, y) = at;
    if sw == 0 || sh == 0 {
        return;
    }
    for row in 0..sh {
        for col in 0..sw {
            let px = (x + col as f32).round() as i32;
            let py = (y + row as f32).round() as i32;
            if px < 0 || py < 0 || px >= dw as i32 || py >= dh as i32 {
                continue;
            }
            let si = (row * sw + col) as usize;
            if si >= src.len() {
                continue;
            }
            let di = (py as u32 * dw + px as u32) as usize;
            let v = u16::from(src[si]);
            let inv = 255 - v;
            let out = v + ((u16::from(dst[di]) * inv + 127) / 255);
            dst[di] = out.min(255) as u8;
        }
    }
}

fn stamp_color(pixels: &mut [u8], origin: (u32, u32), x: u32, y: u32, sample: [u8; 4]) {
    if sample[3] == 0 {
        return;
    }
    let dst = (((origin.1 + y) * COLOR_ATLAS_SIZE + origin.0 + x) * 4) as usize;
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

    #[test]
    fn warmed_glyphs_have_the_same_ink_as_sequential_ones() {
        let keys = [('中', 2), ('日', 2), ('語', 2), ('ф', 1)];
        let mut warmed = Atlas::with_cell(18, 34);
        warmed.warm_glyphs(keys);
        let mut sequential = Atlas::with_cell(18, 34);
        for (ch, cells) in keys {
            let _ = sequential.ensure(ch, cells);
        }
        for (ch, cells) in keys {
            let a = warmed.ensure(ch, cells).expect("warmed rect");
            let b = sequential.ensure(ch, cells).expect("sequential rect");
            assert_eq!(ink_count(&warmed, a), ink_count(&sequential, b), "{ch}");
        }
    }

    #[test]
    fn a_wide_glyph_at_the_last_column_wraps_without_reclaiming() {
        let mut atlas = Atlas::with_cell(18, 38);
        for color in [false, true] {
            let cols = (ATLAS_SIZE / 18).max(1);
            let cursor = if color {
                &mut atlas.color_cursor
            } else {
                &mut atlas.cursor
            };
            for _ in 0..cols + 2 {
                if *cursor % cols == cols - 1 {
                    break;
                }
                *cursor += 1;
            }
            assert_eq!(*cursor % cols, cols - 1);
            let at = atlas.alloc_impl(2, color).expect("wrapped slot");
            assert_eq!(at.0, 0, "the wrap starts a new row");
            assert!(!atlas.needs_reset, "a wide glyph must not empty the caches");
        }
    }

    #[cfg(feature = "bundled-font")]
    #[test]
    fn a_colliding_cluster_hash_does_not_return_the_wrong_glyph() {
        let mut atlas = Atlas::with_cell(18, 34);
        let text = "\u{1F1FA}\u{1F1E6}";
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        text.hash(&mut hasher);
        2u8.hash(&mut hasher);
        let bogus = GlyphRect {
            x: 1,
            y: 1,
            w: 1,
            h: 1,
            atlas: 0,
        };
        atlas
            .clusters
            .insert(hasher.finish(), ("other".into(), Some(bogus)));
        assert_ne!(atlas.ensure_str(text, 2), Some(bogus));
    }

    #[cfg(feature = "bundled-font")]
    #[test]
    fn reclaim_clears_caches_and_restarts_packing() {
        let mut atlas = Atlas::with_cell(18, 34);
        let before = atlas.ensure('λ', 1).expect("lambda");
        atlas.reclaim();
        assert!(atlas.take_dirty());
        assert!(atlas.take_color_dirty());
        assert!(!atlas.needs_reset());
        assert_eq!(atlas.ensure('λ', 1), Some(before));
    }

    #[cfg(feature = "bundled-font")]
    #[test]
    fn color_pixels_use_the_chosen_byte_order() {
        let rgba = Atlas::new();
        assert_eq!(rgba.to_format([1, 2, 3, 4]), [1, 2, 3, 4]);
        let argb = Atlas::with_face_format(Face::bundled(), 8, 16, PixelFormat::Argb);
        assert_eq!(argb.to_format([1, 2, 3, 4]), [3, 2, 1, 4]);
    }

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

    #[cfg(feature = "bundled-font")]
    #[test]
    fn the_underline_sits_under_the_baseline_within_the_cell() {
        let mut atlas = Atlas::with_cell(9, 17);
        let h = atlas.glyph('H').expect("H");
        let line = atlas.underline(1).expect("underline");
        let (_, h_bottom) = ink_span(&atlas, h);
        let (top, bottom) = ink_span(&atlas, line);
        assert!(
            top > h_bottom && bottom < line.h,
            "H ends {h_bottom}, line {top}..={bottom}"
        );
        assert_eq!(atlas.underline(2).expect("wide").w, 18);
    }

    #[cfg(feature = "bundled-font")]
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
        assert!(w >= 8.0 && h >= 16.0, "bundled cell {w}x{h}");
    }

    #[cfg(any(
        feature = "fallback-common",
        feature = "fallback-ja",
        feature = "fallback-zh-full"
    ))]
    #[test]
    fn a_codepoint_the_face_lacks_comes_from_the_compact_fallback() {
        let mut atlas = Atlas::with_cell(10, 20);
        assert_eq!(atlas.face().font().unwrap().charmap().map('中'), 0);
        let rect = atlas.ensure('中', 2).expect("CJK glyph");
        assert_eq!(rect.w, 20);
        assert!(ink_count(&atlas, rect) > 20, "fallback has ink");
    }

    #[cfg(all(feature = "cascadia", feature = "fallback-common"))]
    #[test]
    fn fallback_glyph_scales_to_a_face_with_another_upem() {
        let mut atlas = Atlas::with_face(Face::bundled_cascadia(), 8, 16);
        let rect = atlas.ensure('中', 2).expect("CJK glyph");
        let columns = (0..rect.w)
            .filter(|x| (0..rect.h).any(|y| px(&atlas, rect, *x, y) > 16))
            .count();
        assert!(
            columns >= (rect.w / 2) as usize,
            "glyph too narrow: {columns} of {}",
            rect.w
        );
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

    #[cfg(feature = "bundled-font")]
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

    #[cfg(feature = "bundled-font")]
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

    #[cfg(feature = "emoji-fallback")]
    #[test]
    fn a_two_glyph_cluster_keeps_both_glyphs() {
        let mut atlas = Atlas::with_cell(18, 34);
        let (blob, index) = crate::face::bundled_emoji();
        let man = atlas
            .rasterize_cluster_with("\u{1F468}", 4, blob.data(), index)
            .expect("man");
        let both = atlas
            .rasterize_cluster_with("\u{1F468}\u{200D}\u{1F40D}", 4, blob.data(), index)
            .expect("man and snake");
        assert_eq!(man.atlas, 0);
        assert_eq!(both.atlas, 0);
        assert!(
            ink_count(&atlas, both) > ink_count(&atlas, man) + 20,
            "the second glyph is missing"
        );
    }

    #[cfg(all(target_os = "macos", feature = "bundled-font"))]
    #[test]
    fn a_flag_cluster_shapes_into_one_color_glyph() {
        let mut atlas = Atlas::with_cell(18, 34);
        let flag = atlas
            .ensure_str("\u{1F1FA}\u{1F1E6}", 2)
            .expect("Ukrainian flag");
        assert_eq!(flag.atlas, 1);
        let pixels = atlas.color_pixels().expect("color atlas");
        let mut alpha = 0u8;
        for y in 0..flag.h {
            for x in 0..flag.w {
                let i = (((flag.y + y) * COLOR_ATLAS_SIZE + flag.x + x) * 4) as usize;
                alpha = alpha.max(pixels[i + 3]);
            }
        }
        assert!(alpha > 16, "flag alpha {alpha}");
        assert_eq!(atlas.ensure_str("\u{1F1FA}\u{1F1E6}", 2), Some(flag));
    }

    #[cfg(all(target_os = "macos", feature = "bundled-font"))]
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
                let i = (((snake.y + y) * COLOR_ATLAS_SIZE + snake.x + x) * 4) as usize;
                alpha = alpha.max(pixels[i + 3]);
            }
        }
        assert!(alpha > 16, "emoji alpha {alpha}");
        assert!(atlas.take_color_dirty());
        assert!(!atlas.take_dirty());
        assert_eq!(atlas.ensure('🐍', 2), Some(snake));
        assert!(!atlas.take_color_dirty());
    }

    #[cfg(feature = "bundled-font")]
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
