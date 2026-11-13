//! Settings and search UI/editor state.

use std::sync::Arc;

use rune::TextEdit;

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
    pub font_family_picker_open: bool,
    pub font_family_before_pick: Option<String>,
    pub font_search: TextEdit,
    pub font_picker_active: usize,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThemeColorField {
    Bg,
    Fg,
    FgBright,
    Muted,
    TabBg,
    ActiveBorder,
    Accent,
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
    pub picking: Option<ThemeColorField>,
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
    pub font_picker_open: bool,
    pub font_family_before_pick: Option<String>,
    pub font_search: TextEdit,
    pub font_picker_active: usize,
}

#[derive(Clone, Debug)]
pub struct SearchUi {
    pub open: bool,
    pub query: TextEdit,
}

impl Default for SearchUi {
    fn default() -> Self {
        Self {
            open: false,
            query: TextEdit::new(""),
        }
    }
}

#[derive(Clone, Debug)]
pub struct UiState {
    pub search: SearchUi,
    pub settings_mode: SettingsMode,
    pub pending_link_open: Option<String>,
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            search: SearchUi::default(),
            settings_mode: SettingsMode::Closed,
            pending_link_open: None,
        }
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
            font_picker_open: false,
            font_family_before_pick: None,
            font_search: TextEdit::new(""),
            font_picker_active: 0,
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
            picking: None,
        }
    }

    pub fn field_edit(&self, field: ThemeColorField) -> &TextEdit {
        match field {
            ThemeColorField::Bg => &self.bg,
            ThemeColorField::Fg => &self.fg,
            ThemeColorField::FgBright => &self.fg_bright,
            ThemeColorField::Muted => &self.muted,
            ThemeColorField::TabBg => &self.tab_bg,
            ThemeColorField::ActiveBorder => &self.active_border,
            ThemeColorField::Accent => &self.accent,
        }
    }

    pub fn field_edit_mut(&mut self, field: ThemeColorField) -> &mut TextEdit {
        match field {
            ThemeColorField::Bg => &mut self.bg,
            ThemeColorField::Fg => &mut self.fg,
            ThemeColorField::FgBright => &mut self.fg_bright,
            ThemeColorField::Muted => &mut self.muted,
            ThemeColorField::TabBg => &mut self.tab_bg,
            ThemeColorField::ActiveBorder => &mut self.active_border,
            ThemeColorField::Accent => &mut self.accent,
        }
    }

    pub fn set_field_hex(&mut self, field: ThemeColorField, rgb: u32) {
        *self.field_edit_mut(field) = TextEdit::new(hex6(rgb));
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
