//! Cell-grid wgpu pass. No Rune and no librio. The caller scissors to the
//! pane, then draws the solid fill and this grid into that pass.

mod atlas;
mod cell;
#[cfg(target_os = "macos")]
mod fallback;
pub mod images;
mod renderer;
mod sixel;
mod solid;
mod sprite;

pub use atlas::{face_cell, face_cell_for, Atlas, GlyphRect};
pub use cell::{CellSample, CursorSample, GridSource};
pub use renderer::{DrawSlot, GridRenderer};
pub use solid::SolidSlot;
