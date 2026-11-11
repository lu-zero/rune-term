use std::sync::Arc;

use librio::Engine;
use rune::{Mods, TextEdit};

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
    Profiles,
}

#[derive(Clone, Debug)]
pub struct SettingsUi {
    pub tab: SettingsTab,
    pub theme_editor: Option<ThemeEditor>,
    pub profile_editor: Option<ProfileEditor>,
}

#[derive(Clone, Debug)]
pub enum SettingsMode {
    Closed,
    Open(Box<SettingsUi>),
}

impl SettingsMode {
    pub fn is_open(&self) -> bool {
        matches!(self, Self::Open(_))
    }

    pub fn close(&mut self) {
        *self = Self::Closed;
    }
}

#[derive(Clone, Debug)]
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
    pub created_at: u64,
}

#[derive(Clone, Debug)]
pub struct ProfileEditor {
    pub original_name: String,
    pub name: TextEdit,
    pub font_family: TextEdit,
    pub font_size: TextEdit,
    pub theme: TextEdit,
    pub backend: TextEdit,
    pub shell_program: TextEdit,
    pub shell_args: TextEdit,
    pub cwd: TextEdit,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct GridDrawStamp {
    pub shown: Option<librio::SurfaceId>,
    pub search: String,
    pub hover: Option<crate::hyperlink::HoverLink>,
    pub hover_active: bool,
    pub preedit: Option<Preedit>,
    pub theme: String,
}

impl GridDrawStamp {
    pub fn invalidate_theme(&mut self) {
        self.theme.clear();
    }
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
    pub(crate) grid_draw: GridDrawStamp,
    pub(crate) shared: Arc<Shared>,
    pub(crate) pending_inject: Option<Vec<u8>>,
    pub(crate) pending_link_open: Option<String>,
    pub(crate) settings_mode: SettingsMode,
    pub(crate) _ipc: Option<crate::ipc::Server>,
    pub held_mods: rune::Mods,
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
        grid_draw: GridDrawStamp {
            theme: palette.name.clone(),
            ..GridDrawStamp::default()
        },
        palette,
        shared: std::sync::Arc::clone(&shared),
        pending_inject,
        pending_link_open: None,
        settings_mode: SettingsMode::Closed,
        _ipc: Some(crate::ipc::start(shared)),
        held_mods: Mods::default(),
    }
}

impl ProfileEditor {
    pub fn from_profile(name: &str, profile: &crate::settings::Profile) -> Self {
        let (shell_program, shell_args) = match &profile.shell {
            Some(shell) => (
                shell.program.clone().unwrap_or_default(),
                shell.args.join(" "),
            ),
            None => (String::new(), String::new()),
        };
        Self {
            original_name: name.to_owned(),
            name: TextEdit::new(name),
            font_family: TextEdit::new(&profile.font_family),
            font_size: TextEdit::new(
                profile
                    .font_size
                    .map(|s| {
                        if (s.fract()).abs() < f32::EPSILON {
                            format!("{s:.0}")
                        } else {
                            format!("{s}")
                        }
                    })
                    .unwrap_or_default(),
            ),
            theme: TextEdit::new(profile.theme.clone().unwrap_or_default()),
            backend: TextEdit::new(&profile.backend),
            shell_program: TextEdit::new(shell_program),
            shell_args: TextEdit::new(shell_args),
            cwd: TextEdit::new(profile.cwd.clone().unwrap_or_default()),
        }
    }

    pub fn to_profile(&self) -> Option<(String, crate::settings::Profile)> {
        let name = self.name.text.trim().to_owned();
        if name.is_empty() {
            return None;
        }
        let font_size = {
            let t = self.font_size.text.trim();
            if t.is_empty() {
                None
            } else {
                Some(t.parse::<f32>().ok()?)
            }
        };
        let theme = {
            let t = self.theme.text.trim();
            if t.is_empty() {
                None
            } else {
                Some(t.to_owned())
            }
        };
        let backend = {
            let t = self.backend.text.trim();
            if t.is_empty() {
                "local".to_owned()
            } else {
                t.to_owned()
            }
        };
        let font_family = {
            let t = self.font_family.text.trim();
            if t.is_empty() {
                "Cascadia Code NF".to_owned()
            } else {
                t.to_owned()
            }
        };
        let cwd = {
            let t = self.cwd.text.trim();
            if t.is_empty() {
                None
            } else {
                Some(t.to_owned())
            }
        };
        let shell = {
            let program = self.shell_program.text.trim();
            let args = self.shell_args.text.trim();
            if program.is_empty() && args.is_empty() {
                None
            } else {
                Some(crate::settings::ShellSpec {
                    program: if program.is_empty() {
                        None
                    } else {
                        Some(program.to_owned())
                    },
                    args: if args.is_empty() {
                        Vec::new()
                    } else {
                        args.split_whitespace().map(str::to_owned).collect()
                    },
                })
            }
        };
        Some((
            name,
            crate::settings::Profile {
                font_family,
                font_size,
                theme,
                backend,
                shell,
                cwd,
            },
        ))
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
            created_at: theme.created_at,
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
                created_at: self.created_at,
            },
        ))
    }

    pub fn preview_palette(
        &self,
        fallback: &crate::settings::CustomTheme,
    ) -> Arc<crate::theme::Palette> {
        let custom = crate::settings::CustomTheme {
            author: self.author.text.trim().to_owned(),
            url: self.url.text.trim().to_owned(),
            bg: parse_hex(&self.bg.text).unwrap_or(fallback.bg),
            fg: parse_hex(&self.fg.text).unwrap_or(fallback.fg),
            fg_bright: parse_hex(&self.fg_bright.text).unwrap_or(fallback.fg_bright),
            muted: parse_hex(&self.muted.text).unwrap_or(fallback.muted),
            tab_bg: parse_hex(&self.tab_bg.text).unwrap_or(fallback.tab_bg),
            active_border: parse_hex(&self.active_border.text).unwrap_or(fallback.active_border),
            accent: parse_hex(&self.accent.text).unwrap_or(fallback.accent),
            ansi16: self.ansi16.clone(),
            created_at: self.created_at,
        };
        let name = {
            let n = self.name.text.trim();
            if n.is_empty() {
                &self.original_name
            } else {
                n
            }
        };
        custom.to_arc_palette(name)
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
