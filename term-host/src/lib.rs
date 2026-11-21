//! Reusable terminal host state: surfaces, grid sync, scrollback, selection,
//! search and input delivery. No Rune frontend and no rendering backend.

pub mod font;
pub mod grid_source;
pub mod hyperlink;
pub mod images;
pub mod ime;
pub mod input;
pub mod render;
pub mod resize;
pub mod scrollback;
pub mod search;
pub mod selection;
pub mod surfaces;
pub mod terminal;
pub mod terminal_input;
pub mod theme;

pub use terminal::{State as TerminalState, Terminal};
