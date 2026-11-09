use std::sync::Arc;

use librio::Engine;
use rune::TextEdit;

use crate::chrome::{Leaf, LeafId, Node, Tab, LEAF, PANE_MARK};
use crate::delegate::{self, Shared};
use crate::metrics;
use crate::settings::ConfigFile;
use crate::surfaces::Surfaces;
use crate::theme::Palette;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Preedit {
    pub text: String,
    pub cursor: Option<(usize, usize)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettingsTab {
    Font,
    Theme,
}

pub struct ThemeEditor {
    pub original_name: String,
    pub name: TextEdit,
    pub author: TextEdit,
    pub url: TextEdit,
    pub bg: TextEdit,
    pub fg: TextEdit,
    pub fg_bright: TextEdit,
    pub muted: TextEdit,
    pub tab_bg: TextEdit,
    pub active_border: TextEdit,
    pub accent: TextEdit,
    pub ansi16: Vec<[u8; 3]>,
}

pub struct Model {
    pub engine: Engine,
    pub tabs: Vec<Tab>,
    pub active: usize,
    pub surfaces: Surfaces,
    pub cell_points: (f32, f32),
    pub scale: f32,
    pub config: ConfigFile,
    pub font_size: f32,
    pub palette: Arc<Palette>,
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
    pub(crate) theme_drawn: String,
    pub(crate) shared: Arc<Shared>,
    pub(crate) pending_inject: Option<Vec<u8>>,
    pub(crate) pending_link_open: Option<String>,
    pub(crate) settings_open: bool,
    pub(crate) settings_tab: SettingsTab,
    pub(crate) theme_editor: Option<ThemeEditor>,
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
        cell_points: metrics::cell_points_for(font_size),
        scale: 1.0,
        config,
        font_size,
        blink_phase_on: true,
        preedit: None,
        cursor_cell: (0, 0),
        search_open: false,
        query: TextEdit::new(""),
        search_drawn: String::new(),
        grid_shown: None,
        hover_drawn: None,
        preedit_drawn: None,
        theme_drawn: palette.name.clone(),
        palette,
        shared,
        pending_inject,
        pending_link_open: None,
        settings_open: false,
        settings_tab: SettingsTab::Font,
        theme_editor: None,
    }
}

impl ThemeEditor {
    pub fn from_custom(name: &str, theme: &crate::settings::CustomTheme) -> Self {
        Self {
            original_name: name.to_owned(),
            name: TextEdit::new(name),
            author: TextEdit::new(&theme.author),
            url: TextEdit::new(&theme.url),
            bg: TextEdit::new(hex6(theme.bg)),
            fg: TextEdit::new(hex6(theme.fg)),
            fg_bright: TextEdit::new(hex6(theme.fg_bright)),
            muted: TextEdit::new(hex6(theme.muted)),
            tab_bg: TextEdit::new(hex6(theme.tab_bg)),
            active_border: TextEdit::new(hex6(theme.active_border)),
            accent: TextEdit::new(hex6(theme.accent)),
            ansi16: {
                let mut a = theme.ansi16.clone();
                a.resize(16, [0, 0, 0]);
                a.truncate(16);
                a
            },
        }
    }

    pub fn to_custom(&self) -> Option<(String, crate::settings::CustomTheme)> {
        let name = self.name.text.trim().to_owned();
        if name.is_empty() {
            return None;
        }
        Some((
            name,
            crate::settings::CustomTheme {
                author: self.author.text.trim().to_owned(),
                url: self.url.text.trim().to_owned(),
                bg: parse_hex(&self.bg.text)?,
                fg: parse_hex(&self.fg.text)?,
                fg_bright: parse_hex(&self.fg_bright.text)?,
                muted: parse_hex(&self.muted.text)?,
                tab_bg: parse_hex(&self.tab_bg.text)?,
                active_border: parse_hex(&self.active_border.text)?,
                accent: parse_hex(&self.accent.text)?,
                ansi16: self.ansi16.clone(),
            },
        ))
    }
}

fn hex6(v: u32) -> String {
    format!("#{:06X}", v & 0x00FF_FFFF)
}

pub(crate) fn parse_hex(s: &str) -> Option<u32> {
    let s = s.trim();
    let s = s
        .strip_prefix('#')
        .or_else(|| s.strip_prefix("0x"))
        .or_else(|| s.strip_prefix("0X"))
        .unwrap_or(s);
    if s.len() != 6 {
        return None;
    }
    u32::from_str_radix(s, 16).ok()
}
