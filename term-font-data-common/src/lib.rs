//! Compressed font bytes. Decompress once with `term_font::data::decompress`.

/// zstd frame holding CjkFallback.ttf.
pub const ZSTD: &[u8] = include_bytes!("../data/CjkFallback.ttf.zst");
