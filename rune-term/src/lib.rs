//! One window, one surface, and the seams later changes fill in.
//! The design lives in `docs/todo/rune-term.md`.

mod app;
mod chrome;
mod delegate;
mod grid_pass;
mod grid_source;
mod images;
mod ime;
mod metrics;
mod model;
mod pane_input;
mod resize;
mod search;
mod selection;
mod split;
mod surfaces;
mod theme;

pub use app::{app, run};
pub use chrome::{slot, tab};
pub use delegate::{harness, queue_inject, Harness};
pub use metrics::cell_points;
