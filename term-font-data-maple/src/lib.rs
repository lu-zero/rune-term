//! Compressed font bytes. Decompress once with `term_font::data::decompress`.

/// zstd frame holding MapleMonoNF.ttf.
pub const ZSTD: &[u8] = include_bytes!("../data/MapleMonoNF.ttf.zst");
