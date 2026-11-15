//! What a terminal multiplexer reports about its windows and panes, in cells,
//! and the tmux control-mode parser that produces it. No I/O.

pub mod layout;
pub mod tmux;

pub use layout::{Axis, Layout};
