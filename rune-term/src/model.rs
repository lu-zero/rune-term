use std::sync::Arc;

use librio::Engine;
use rune::TextEdit;

use crate::chrome::{Leaf, LeafId, Node, Tab, LEAF, PANE_MARK};
use crate::delegate::{self, Shared};
use crate::metrics;
use crate::surfaces::Surfaces;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Preedit {
    pub text: String,
    pub cursor: Option<(usize, usize)>,
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
    /// The search field is up. It takes focus while `query.focused` is set.
    pub search_open: bool,
    pub query: TextEdit,
    /// The pattern the last grid upload tinted. Empty when search was closed.
    pub(crate) search_drawn: String,
    /// The shell the last grid upload drew. A tab change forces the next one.
    pub(crate) grid_shown: Option<librio::SurfaceId>,
    pub(crate) hover_drawn: Option<crate::hyperlink::HoverLink>,
    pub(crate) preedit_drawn: Option<Preedit>,
    pub(crate) shared: Arc<Shared>,
    pub(crate) pending_inject: Option<Vec<u8>>,
    pub(crate) pending_link_open: Option<String>,
    pub(crate) settings_open: bool,
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
            root: Node::Leaf(Leaf {
                id: LEAF,
                surface: 0,
                mark: PANE_MARK,
                cols: 80,
                rows: 24,
            }),
            focused: LeafId(LEAF.0),
        }],
        active: 0,
        surfaces: Surfaces::new(),
        cell_points: metrics::cell_points(),
        blink_phase_on: true,
        preedit: None,
        cursor_cell: (0, 0),
        search_open: false,
        query: TextEdit::new(""),
        search_drawn: String::new(),
        grid_shown: None,
        hover_drawn: None,
        preedit_drawn: None,
        shared,
        pending_inject,
        pending_link_open: None,
        settings_open: false,
    }
}
