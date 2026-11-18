use std::sync::Arc;

use librio::Engine;
use rune::AppHandle;

use crate::chrome::{Leaf, LeafId, Node, Tab, TabGroup, LEAF, PANE_MARK};
use crate::delegate::{self, Shared};
use crate::metrics;
use crate::search::SearchUi;
use crate::settings::ConfigFile;
use crate::settings_ui::SettingsModel;
use crate::theme::Palette;

pub struct Model {
    pub engine: Engine,
    pub tabs: Vec<Tab>,
    pub tab_groups: Vec<TabGroup>,
    pub active: usize,
    pub terminal: rune_term_core::terminal::Terminal,
    pub cell_points: (f32, f32),
    pub scale: f32,
    pub config: ConfigFile,
    pub font_size: f32,
    pub face: term_grid::Face,
    pub palette: Arc<Palette>,
    pub search: SearchUi,
    pub pending_link_open: Option<String>,
    /// The name prompt for a new session or a saved one, while it is open.
    pub(crate) name_prompt: Option<crate::mux::NamePrompt>,
    /// What went wrong opening or keeping a session, until it is closed.
    pub(crate) notice: Option<crate::chrome::Notice>,
    pub(crate) closing: Option<crate::closing::Closing>,
    pub(crate) strip_drag: Option<crate::tabs::StripDrag>,
    /// A drop just ended on the strip. Rune runs the press its release also makes after it.
    pub(crate) strip_dropped: bool,
    /// The settings window. It is the terminal's to open.
    pub settings: Option<AppHandle<SettingsModel>>,
    pub(crate) shared: Arc<Shared>,
    pub(crate) pending_inject: Option<Vec<u8>>,
    /// Child output from `RUNE_TERM_INJECT`, held until the pane has a size so a line is not wrapped at the default 80 columns.
    pub(crate) inject_when_sized: Option<Vec<u8>>,
    pub(crate) _ipc: Option<crate::ipc::Server>,
    pub(crate) muxes: Vec<crate::mux::Mux>,
    /// The hosts Connect to Host named this run, which the menu lists with a profile's.
    pub(crate) hosts: Vec<crate::mux::Target>,
}

pub fn init() -> Model {
    let pending_inject = delegate::take_inject();
    let inject_when_sized = match pending_inject {
        Some(_) => None,
        None => delegate::read_inject_file(),
    };
    let shared = Shared::new();
    delegate::publish(&shared);
    let engine = Engine::new(shared.delegate());
    let config = ConfigFile::load();
    let resolved = config.resolve();
    let font_size = resolved.font_size;
    let face = term_grid::Face::load(&resolved.font_family);
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
            group: None,
            window: None,
            fit: None,
        }],
        tab_groups: Vec::new(),
        active: 0,
        terminal: rune_term_core::terminal::Terminal::new(palette.name.clone()),
        cell_points: metrics::cell_points_for(&face, font_size),
        scale: 1.0,
        config,
        font_size,
        face,
        palette,
        search: SearchUi::default(),
        pending_link_open: None,
        name_prompt: None,
        notice: None,
        closing: None,
        strip_drag: None,
        strip_dropped: false,
        settings: None,
        shared: std::sync::Arc::clone(&shared),
        pending_inject,
        inject_when_sized,
        _ipc: Some(crate::ipc::start(shared)),
        muxes: Vec::new(),
        hosts: Vec::new(),
    }
}
