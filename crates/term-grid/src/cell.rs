//! GPU cell layouts. Straight sRGB plus alpha. The shader premultiplies.

use bytemuck::{Pod, Zeroable};

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Pod, Zeroable)]
pub struct CellBg {
    pub rgba: [u8; 4],
}

impl CellBg {
    pub const TRANSPARENT: Self = Self { rgba: [0, 0, 0, 0] };
}

/// Bearings are `Sint32x2`, the grid position is `Uint32x2`, and the atlas
/// id is `Uint32`. Those are formats Rune's Metal backend draws.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Pod, Zeroable)]
pub struct CellText {
    pub glyph_pos: [u32; 2],
    pub glyph_size: [u32; 2],
    pub bearings: [i32; 2],
    pub grid_pos: [u32; 2],
    pub color: [u8; 4],
    pub atlas: u32,
    pub bools: u32,
    pub page: u32,
}

impl CellText {
    pub const ATLAS_GRAYSCALE: u32 = 0;
    pub const ATLAS_COLOR: u32 = 1;
}

/// 160 bytes. The scalar block starts at offset 144.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct GridUniforms {
    pub projection: [f32; 16],
    pub grid_padding: [f32; 4],
    pub cursor_color: [f32; 4],
    pub cursor_bg_color: [f32; 4],
    pub cell_size: [f32; 2],
    pub grid_size: [u32; 2],
    pub cursor_pos: [u32; 2],
    pub _pad_cursor: [u32; 2],
    pub min_contrast: f32,
    pub flags: u32,
    pub padding_extend: u32,
    pub input_colorspace: u32,
}

impl GridUniforms {
    pub const FLAG_DISPLAY_P3: u32 = 1 << 0;
    pub const FLAG_LINEAR_BLENDING: u32 = 1 << 1;
}

const _: () = {
    assert!(std::mem::size_of::<CellBg>() == 4);
    assert!(std::mem::size_of::<CellText>() == 48);
    assert!(std::mem::size_of::<GridUniforms>() == 160);
    assert!(std::mem::size_of::<GridUniforms>() % 16 == 0);
    assert!(CellText::ATLAS_GRAYSCALE == 0);
    assert!(CellText::ATLAS_COLOR == 1);
    assert!(GridUniforms::FLAG_DISPLAY_P3 == 1);
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CellSample {
    pub ch: char,
    pub width: u8,
    pub fg: [u8; 4],
    pub bg: [u8; 4],
    pub underline: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CursorSample {
    pub col: u32,
    pub row: u32,
    pub visible: bool,
    pub color: [f32; 4],
    pub bg: [f32; 4],
}

pub trait GridSource {
    fn columns(&self) -> usize;
    fn lines(&self) -> usize;
    fn row_dirty(&self, line: usize) -> bool;
    fn cell(&self, line: usize, col: usize) -> CellSample;
    fn cursor(&self) -> CursorSample;
    fn reset_dirty(&mut self);
}
