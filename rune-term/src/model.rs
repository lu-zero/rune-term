use std::sync::Arc;

use librio::Engine;

use crate::chrome::{Leaf, LeafId, Node, Tab, LEAF, PANE_MARK};
use crate::delegate::{self, Shared};
use crate::metrics;
use crate::search::SearchUi;
use crate::settings::ConfigFile;
use crate::theme::Palette;

pub struct Model {
    pub engine: Engine,
    pub tabs: Vec<Tab>,
    pub active: usize,
    pub terminal: rune_term_core::terminal::Terminal,
    pub cell_points: (f32, f32),
    pub scale: f32,
    pub config: ConfigFile,
    pub font_size: f32,
    pub palette: Arc<Palette>,
    pub search: SearchUi,
    pub pending_link_open: Option<String>,
    pub(crate) shared: Arc<Shared>,
    pub(crate) pending_inject: Option<Vec<u8>>,
    pub(crate) _ipc: Option<crate::ipc::Server>,
}

pub fn init() -> Model {
    let pending_inject = delegate::take_inject();
    let shared = Shared::new();
    delegate::publish(&shared);
    let engine = Engine::new(shared.delegate());
    let config = ConfigFile::load();
    let resolved = config.resolve();
    let font_size = resolved.font_size;
    let palette = config.resolve_palette();
    let profile = config.default_profile_name().to_owned();
    Model {
        engine,
        tabs: vec![Tab {
            title: "rune-term".to_string(),
            profile,
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
        terminal: rune_term_core::terminal::Terminal::new(palette.name.clone()),
        cell_points: metrics::cell_points_for(font_size),
        scale: 1.0,
        config,
        font_size,
        palette,
        search: SearchUi::default(),
        pending_link_open: None,
        shared: std::sync::Arc::clone(&shared),
        pending_inject,
        _ipc: Some(crate::ipc::start(shared)),
    }
}
