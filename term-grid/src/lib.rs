//! Cell-grid wgpu pass. No Rune and no librio. The caller scissors to the
//! pane, then draws the solid fill and this grid into that pass.

mod cell;
pub mod images;
mod renderer;
mod sixel;
mod solid;

pub use cell::{CellSample, CursorSample, GridSource};
pub use renderer::{DrawSlot, GridRenderer};
pub use solid::SolidSlot;
#[cfg(feature = "cascadia")]
pub use term_atlas::CASCADIA_FAMILY;
pub use term_atlas::{
    face_cell, face_cell_for, fallback_bytes, monospace_families, Atlas, Face, GlyphRect,
    BUNDLED_FAMILY, MAPLE_FAMILY,
};
