use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::theme::{self, Palette};

pub const DEFAULT_FONT_SIZE: f32 = 14.0;
const MIN_FONT_SIZE: f32 = 6.0;
const MAX_FONT_SIZE: f32 = 96.0;
const CONFIG_NAME: &str = "rune-term.toml";

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct ConfigFile {
    pub version: u32,
    pub settings: Settings,
    pub profiles: Profiles,
    pub custom_themes: BTreeMap<String, CustomTheme>,
}

impl Default for ConfigFile {
    fn default() -> Self {
        Self {
            version: 1,
            settings: Settings::default(),
            profiles: Profiles::default(),
            custom_themes: BTreeMap::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub default_profile: String,
    pub font_size: f32,
    pub theme: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            default_profile: "Default".to_owned(),
            font_size: DEFAULT_FONT_SIZE,
            theme: "Flexoki Black".to_owned(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Profile {
    pub font_family: String,
    pub font_size: Option<f32>,
    pub theme: Option<String>,
}

impl Default for Profile {
    fn default() -> Self {
        Self {
            font_family: "Cascadia Code NF".to_owned(),
            font_size: None,
            theme: None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
#[derive(Default)]
pub struct Profiles {
    #[serde(rename = "Default")]
    pub default: Profile,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct CustomTheme {
    pub author: String,
    pub url: String,
    pub bg: u32,
    pub fg: u32,
    pub fg_bright: u32,
    pub muted: u32,
    pub tab_bg: u32,
    pub active_border: u32,
    pub accent: u32,
    pub ansi16: Vec<[u8; 3]>,
}

impl Default for CustomTheme {
    fn default() -> Self {
        Self::from_palette(&theme::default_palette())
    }
}

impl CustomTheme {
    pub fn from_palette(p: &Palette) -> Self {
        Self {
            author: p.author.clone(),
            url: p.url.clone(),
            bg: p.bg,
            fg: p.fg,
            fg_bright: p.fg_bright,
            muted: p.muted,
            tab_bg: p.tab_bg,
            active_border: p.active_border,
            accent: p.accent,
            ansi16: p.ansi16.to_vec(),
        }
    }

    pub fn to_arc_palette(&self, name: &str) -> Arc<Palette> {
        let mut ansi16 = [[0u8; 3]; 16];
        for (i, slot) in ansi16.iter_mut().enumerate() {
            if let Some(rgb) = self.ansi16.get(i) {
                *slot = *rgb;
            }
        }
        Arc::new(Palette {
            name: name.to_owned(),
            bg: self.bg,
            fg: self.fg,
            fg_bright: self.fg_bright,
            muted: self.muted,
            tab_bg: self.tab_bg,
            active_border: self.active_border,
            accent: self.accent,
            ansi16,
            author: self.author.clone(),
            url: self.url.clone(),
        })
    }
}

#[derive(Clone, Debug)]
pub struct Resolved {
    pub font_size: f32,
    pub theme: String,
}

impl ConfigFile {
    pub fn load() -> Self {
        let path = config_path();
        match load_path(&path) {
            Ok(config) => config,
            Err(err) if err.kind() == io::ErrorKind::NotFound => Self::default(),
            Err(err) => {
                eprintln!("rune-term: failed to load {}: {err}", path.display());
                Self::default()
            }
        }
    }

    pub fn resolve(&self) -> Resolved {
        let profile = &self.profiles.default;
        let font_size = profile.font_size.unwrap_or(self.settings.font_size);
        let theme = profile
            .theme
            .clone()
            .unwrap_or_else(|| self.settings.theme.clone());
        Resolved {
            font_size: clamp_font_size(font_size),
            theme,
        }
    }

    pub fn resolve_palette(&self) -> Arc<Palette> {
        let name = self.resolve().theme;
        if let Some(custom) = self.custom_themes.get(&name) {
            return custom.to_arc_palette(&name);
        }
        theme::resolve_builtin(&name).unwrap_or_else(theme::default_palette)
    }

    pub fn set_font_size(&mut self, size: f32) {
        let size = clamp_font_size(size);
        self.settings.font_size = size;
        self.profiles.default.font_size = Some(size);
    }

    pub fn set_theme(&mut self, theme: String) {
        self.settings.theme = theme.clone();
        self.profiles.default.theme = Some(theme);
    }

    pub fn copy_theme(&mut self, source_name: &str) -> String {
        let source = if let Some(custom) = self.custom_themes.get(source_name) {
            custom.clone()
        } else if let Some(builtin) = theme::resolve_builtin(source_name) {
            CustomTheme::from_palette(&builtin)
        } else {
            CustomTheme::default()
        };
        let base = format!("{source_name} Copy");
        let mut name = base.clone();
        let mut n = 2u32;
        while self.custom_themes.contains_key(&name) || theme::resolve_builtin(&name).is_some() {
            name = format!("{base} {n}");
            n += 1;
        }
        self.custom_themes.insert(name.clone(), source);
        self.set_theme(name.clone());
        name
    }

    pub fn is_custom_theme(&self, name: &str) -> bool {
        self.custom_themes.contains_key(name)
    }

    pub fn save(&self) -> io::Result<()> {
        let path = config_path();
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let text = toml::to_string_pretty(self)
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
        let tmp = path.with_extension("toml.tmp");
        fs::write(&tmp, text)?;
        fs::rename(&tmp, path)?;
        Ok(())
    }
}

pub fn clamp_font_size(size: f32) -> f32 {
    if !size.is_finite() {
        return DEFAULT_FONT_SIZE;
    }
    size.clamp(MIN_FONT_SIZE, MAX_FONT_SIZE)
}

pub fn config_path() -> PathBuf {
    config_dir().join(CONFIG_NAME)
}

fn config_dir() -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home)
                .join("Library")
                .join("Application Support")
                .join("rune-term");
        }
    }
    #[cfg(target_os = "windows")]
    {
        if let Some(appdata) = std::env::var_os("APPDATA") {
            return PathBuf::from(appdata).join("rune-term");
        }
    }
    if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME") {
        return PathBuf::from(xdg).join("rune-term");
    }
    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home).join(".config").join("rune-term");
    }
    PathBuf::from(".").join("rune-term")
}

fn load_path(path: &Path) -> io::Result<ConfigFile> {
    let text = fs::read_to_string(path)?;
    toml::from_str(&text).map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))
}
