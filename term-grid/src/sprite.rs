//! Box-drawing lookup. The later pass replaces this file.

use crate::atlas::GlyphRect;

pub fn sprite(_ch: char) -> Option<GlyphRect> {
    None
}
