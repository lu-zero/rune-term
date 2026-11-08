//! One window, one surface, and the seams later changes fill in.
//! The design lives in `docs/todo/rune-term.md`.

mod app;
mod chrome;
mod chrome_view;
mod delegate;
mod grid_pass;
mod grid_source;
mod hyperlink;
mod images;
mod ime;
mod metrics;
mod model;
mod pane_input;
mod resize;
mod scrollback;
mod search;
mod selection;
mod split;
mod surfaces;
mod tabs;
mod theme;

pub use app::{app, run};
pub use chrome::{slot, tab};
pub use delegate::{harness, queue_inject, Harness};
pub use metrics::cell_points;
