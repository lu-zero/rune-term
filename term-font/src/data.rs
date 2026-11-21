//! Compressed font data shipped by the `term-font-data-*` crates.

use ruzstd::decoding::StreamingDecoder;
use ruzstd::io::Read as _;

/// Decode a zstd frame once, in pure Rust. The bundled frames are part of
/// this build, so a corrupt one is a build error rather than a runtime case.
pub fn decompress(zstd: &[u8]) -> Vec<u8> {
    let mut source = zstd;
    let mut decoder = StreamingDecoder::new(&mut source).expect("bundled font frame");
    let mut bytes = Vec::new();
    decoder.read_to_end(&mut bytes).expect("bundled font frame");
    bytes
}
