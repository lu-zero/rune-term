use std::sync::Arc;

use librio::Engine;

use crate::chrome::{LeafId, Node, Tab, LEAF};
use crate::delegate::{self, Shared};
use crate::metrics;
use crate::surfaces::Surfaces;

pub struct Preedit {
    pub text: String,
}

pub struct Model {
    pub engine: Engine,
    pub tabs: Vec<Tab>,
    pub active: usize,
    pub surfaces: Surfaces,
    pub cell_points: (f32, f32),
    pub blink_phase_on: bool,
    /// Filled by the IME change. The view does not read this.
    pub preedit: Option<Preedit>,
    /// Caret cell for IME placement. The view does not read this.
    pub cursor_cell: (u16, u16),
    pub(crate) shared: Arc<Shared>,
    pub(crate) pending_inject: Option<Vec<u8>>,
}

pub fn init() -> Model {
    let pending_inject = delegate::take_inject();
    let shared = Shared::new();
    delegate::publish(&shared);
    let engine = Engine::new(shared.delegate());
    Model {
        engine,
        tabs: vec![Tab {
            title: "rune-term".to_string(),
            root: Node::Leaf(LEAF),
            focused: LeafId(LEAF.0),
        }],
        active: 0,
        surfaces: Surfaces::new(),
        cell_points: metrics::cell_points(),
        blink_phase_on: true,
        preedit: None,
        cursor_cell: (0, 0),
        shared,
        pending_inject,
    }
}
