//! One window, one surface, and the seams later changes fill in.
//! The design lives in `docs/todo/rune-term.md`.

mod app;
mod chrome;
mod chrome_view;
pub mod cli;
mod delegate;
mod grid_pass;
pub use rune_term_core::grid_source;
mod hyperlink;
pub use rune_term_core::images;
mod ime;
mod ipc;
pub mod metrics;
mod model;
mod pane_input;
mod resize;
mod scrollback;
mod search;
mod selection;
pub mod settings;
pub mod settings_ui;
mod split;
pub use rune_term_core::surfaces;
mod tabs;
pub mod theme;

pub use app::{app, run, run_with_rune_args};
pub use chrome::{slot, tab};
pub use delegate::{harness, queue_inject, Harness};
pub use metrics::cell_points;
pub use settings::ConfigFile;
