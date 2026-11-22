//! zstd-compressed subset of Noto Emoji, used when the host has no emoji face.
//!
//! The subset keeps the regional indicators, ZWJ, variation selectors, skin
//! tone modifiers, keycaps and common emoji, with GSUB retained so flags and
//! ZWJ sequences still shape into one glyph. `LICENSE-OFL.txt` is the notice.

/// zstd frame; decode once with `term_font::data::decompress`.
pub const ZSTD: &[u8] = include_bytes!("../data/EmojiFallback.ttf.zst");
