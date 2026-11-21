//! Rune adapter over the generic terminal host: the app-facing event and
//! colour types live here; the reusable terminal state lives in `term-host`.

pub mod input;
pub mod theme;

pub use term_host::*;
