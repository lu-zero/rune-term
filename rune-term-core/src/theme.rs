//! Rune-side colour conversion, over the generic palette in `term-host`.

pub use term_host::theme::*;

use rune::color::srgb_decode;
use rune::Color;

/// A palette colour as a linear Rune colour for the UI.
pub fn linear(value: u32) -> Color {
    let [r, g, b, _] = term_host::theme::channels(value);
    Color {
        r: srgb_decode(r),
        g: srgb_decode(g),
        b: srgb_decode(b),
        a: 1.0,
    }
}
