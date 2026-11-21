//! CPU glyph atlas: font faces, cell sprites, fallback and rasterized pixels.
//! No rendering backend — the caller uploads or blits [`Atlas::pixels`] and
//! [`Atlas::color_pixels`] however it draws.

mod atlas;
mod face;
#[cfg(all(target_os = "macos", feature = "bundled-font"))]
mod fallback;
mod sprite;

pub use atlas::{face_cell, face_cell_for, Atlas, GlyphRect, PixelFormat, ATLAS_SIZE};
#[cfg(feature = "cascadia")]
pub use face::CASCADIA_FAMILY;
pub use face::{fallback_bytes, monospace_families, Face, BUNDLED_FAMILY, MAPLE_FAMILY};
