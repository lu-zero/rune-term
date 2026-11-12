use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::theme::{self, Palette};

pub const DEFAULT_PROFILE: &str = "Default";
pub const DEFAULT_FONT_SIZE: f32 = 14.0;
const MIN_FONT_SIZE: f32 = 6.0;
const MAX_FONT_SIZE: f32 = 96.0;
const CONFIG_NAME: &str = "rune-term.toml";

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct ConfigFile {
    pub version: u32,
    pub settings: Settings,
    pub profiles: BTreeMap<String, Profile>,
    pub custom_themes: BTreeMap<String, CustomTheme>,
    /// Set when the config file could not be read or parsed. The app runs on
    /// defaults, but saving would overwrite the file, so saves are refused.
    #[serde(skip)]
    load_error: Option<String>,
}

impl Default for ConfigFile {
    fn default() -> Self {
        let mut profiles = BTreeMap::new();
        profiles.insert(DEFAULT_PROFILE.to_owned(), Profile::default());
        Self {
            version: 1,
            settings: Settings::default(),
            profiles,
            custom_themes: BTreeMap::new(),
            load_error: None,
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
            default_profile: DEFAULT_PROFILE.to_owned(),
            font_size: DEFAULT_FONT_SIZE,
            theme: "Flexoki Black".to_owned(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(default)]
pub struct ShellSpec {
    /// Program to execute. When absent, the platform default shell is used.
    pub program: Option<String>,
    /// Arguments passed to the program.
    pub args: Vec<String>,
}

impl ShellSpec {
    pub fn is_empty(&self) -> bool {
        self.program.as_ref().is_none_or(|p| p.trim().is_empty()) && self.args.is_empty()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Profile {
    pub font_family: String,
    pub font_size: Option<f32>,
    pub theme: Option<String>,
    /// Backend kind. "local" is the default (in-process PTY via librio).
    pub backend: String,
    pub shell: Option<ShellSpec>,
    pub cwd: Option<String>,
}

impl Default for Profile {
    fn default() -> Self {
        Self {
            font_family: "Cascadia Code NF".to_owned(),
            font_size: None,
            theme: None,
            backend: "local".to_owned(),
            shell: None,
            cwd: None,
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
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
    pub created_at: u64,
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
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
            created_at: now_secs(),
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
    pub backend: String,
    pub shell: Option<ShellSpec>,
    pub cwd: Option<String>,
}

impl ConfigFile {
    pub fn load() -> Self {
        let path = config_path();
        match load_path(&path) {
            Ok(mut config) => {
                config.normalize();
                config
            }
            Err(err) if err.kind() == io::ErrorKind::NotFound => Self::default(),
            Err(err) => {
                eprintln!("rune-term: failed to load {}: {err}", path.display());
                Self {
                    load_error: Some(err.to_string()),
                    ..Self::default()
                }
            }
        }
    }

    pub(crate) fn normalize(&mut self) {
        if self.profiles.is_empty() {
            self.profiles
                .insert(DEFAULT_PROFILE.to_owned(), Profile::default());
        }
        if self.settings.default_profile.trim().is_empty() {
            self.settings.default_profile = DEFAULT_PROFILE.to_owned();
        }
    }

    /// Name of the profile used when none is specified.
    pub fn default_profile_name(&self) -> &str {
        let name = self.settings.default_profile.as_str();
        if self.profiles.contains_key(name) {
            name
        } else if self.profiles.contains_key(DEFAULT_PROFILE) {
            DEFAULT_PROFILE
        } else {
            self.profiles
                .keys()
                .next()
                .map(String::as_str)
                .unwrap_or(DEFAULT_PROFILE)
        }
    }

    pub fn profile_names(&self) -> impl Iterator<Item = &str> {
        self.profiles.keys().map(String::as_str)
    }

    pub fn profile(&self, name: &str) -> Option<&Profile> {
        self.profiles.get(name)
    }

    pub fn profile_mut(&mut self, name: &str) -> Option<&mut Profile> {
        self.profiles.get_mut(name)
    }

    pub fn resolve(&self) -> Resolved {
        self.resolve_profile(self.default_profile_name())
    }

    pub fn resolve_profile(&self, name: &str) -> Resolved {
        let profile = self
            .profiles
            .get(name)
            .or_else(|| self.profiles.get(DEFAULT_PROFILE));
        let font_size = profile
            .and_then(|p| p.font_size)
            .unwrap_or(self.settings.font_size);
        let theme = profile
            .and_then(|p| p.theme.clone())
            .unwrap_or_else(|| self.settings.theme.clone());
        let backend = profile
            .map(|p| p.backend.trim())
            .filter(|b| !b.is_empty())
            .unwrap_or("local")
            .to_owned();
        let shell = profile
            .and_then(|p| p.shell.clone())
            .filter(|s| !s.is_empty());
        let cwd = profile
            .and_then(|p| p.cwd.as_ref())
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_owned());
        Resolved {
            font_size: clamp_font_size(font_size),
            theme,
            backend,
            shell,
            cwd,
        }
    }

    pub fn resolve_palette(&self) -> Arc<Palette> {
        self.resolve_palette_for(self.default_profile_name())
    }

    pub fn resolve_palette_for(&self, profile: &str) -> Arc<Palette> {
        let name = self.resolve_profile(profile).theme;
        if let Some(custom) = self.custom_themes.get(&name) {
            return custom.to_arc_palette(&name);
        }
        theme::resolve_builtin(&name).unwrap_or_else(theme::default_palette)
    }

    pub fn set_font_size(&mut self, size: f32) {
        let size = clamp_font_size(size);
        self.settings.font_size = size;
        let name = self.default_profile_name().to_owned();
        self.profiles.entry(name).or_default().font_size = Some(size);
    }

    pub fn set_theme(&mut self, theme: String) {
        self.settings.theme = theme.clone();
        let name = self.default_profile_name().to_owned();
        self.profiles.entry(name).or_default().theme = Some(theme);
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

    pub fn ensure_profile(&mut self, name: &str) -> &mut Profile {
        self.profiles.entry(name.to_owned()).or_default()
    }

    pub fn set_default_profile(&mut self, name: &str) -> bool {
        if self.profiles.contains_key(name) {
            self.settings.default_profile = name.to_owned();
            true
        } else {
            false
        }
    }

    pub fn rename_profile(&mut self, from: &str, to: &str) -> bool {
        let to = to.trim();
        if to.is_empty() || from == to || !self.profiles.contains_key(from) {
            return false;
        }
        if self.profiles.contains_key(to) {
            return false;
        }
        let Some(profile) = self.profiles.remove(from) else {
            return false;
        };
        self.profiles.insert(to.to_owned(), profile);
        if self.settings.default_profile == from {
            self.settings.default_profile = to.to_owned();
        }
        true
    }

    pub fn validate_backend(&self, name: &str) -> Result<(), String> {
        let resolved = self.resolve_profile(name);
        match resolved.backend.as_str() {
            "local" => Ok(()),
            other => Err(format!("backend '{other}' is not available")),
        }
    }

    pub fn is_custom_theme(&self, name: &str) -> bool {
        self.custom_themes.contains_key(name)
    }

    pub fn save(&self) -> io::Result<()> {
        if let Some(err) = &self.load_error {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("config was not loaded: {err}"),
            ));
        }
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
    use etcetera::BaseStrategy;
    if let Ok(xdg) = etcetera::base_strategy::Xdg::new() {
        return xdg.config_dir().join("rune-term");
    }
    PathBuf::from(".").join("rune-term")
}

fn load_path(path: &Path) -> io::Result<ConfigFile> {
    let text = fs::read_to_string(path)?;
    toml::from_str(&text).map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_has_default_profile() {
        let cfg = ConfigFile::default();
        assert_eq!(cfg.default_profile_name(), DEFAULT_PROFILE);
        assert!(cfg.profile(DEFAULT_PROFILE).is_some());
        let resolved = cfg.resolve();
        assert_eq!(resolved.font_size, DEFAULT_FONT_SIZE);
    }

    #[test]
    fn parses_named_profiles_and_resolves_selected() {
        let toml = r#"
version = 1

[settings]
default_profile = "Work"
font_size = 14.0
theme = "Flexoki Black"

[profiles.Default]
font_family = "Cascadia Code NF"
font_size = 14.0

[profiles.Work]
font_family = "Cascadia Code NF"
font_size = 18.0
theme = "Flexoki Light"
"#;
        let mut cfg: ConfigFile = toml::from_str(toml).expect("parse");
        cfg.normalize();
        assert_eq!(cfg.default_profile_name(), "Work");
        let names: Vec<_> = cfg.profile_names().collect();
        assert!(names.contains(&"Default"));
        assert!(names.contains(&"Work"));
        let work = cfg.resolve_profile("Work");
        assert_eq!(work.font_size, 18.0);
        assert_eq!(work.theme, "Flexoki Light");
        let def = cfg.resolve_profile("Default");
        assert_eq!(def.font_size, 14.0);
    }

    #[test]
    fn missing_default_profile_falls_back() {
        let toml = r#"
version = 1

[settings]
default_profile = "Missing"
font_size = 15.0

[profiles.Only]
font_size = 12.0
"#;
        let mut cfg: ConfigFile = toml::from_str(toml).expect("parse");
        cfg.normalize();
        assert_eq!(cfg.default_profile_name(), "Only");
        assert_eq!(cfg.resolve().font_size, 12.0);
    }

    #[test]
    fn set_font_size_updates_default_profile_entry() {
        let mut cfg = ConfigFile::default();
        cfg.set_font_size(20.0);
        assert_eq!(cfg.settings.font_size, 20.0);
        assert_eq!(cfg.profile(DEFAULT_PROFILE).unwrap().font_size, Some(20.0));
        assert_eq!(cfg.resolve().font_size, 20.0);
    }

    #[test]
    fn parses_profile_launch_fields() {
        let toml = r#"
version = 1

[settings]
default_profile = "Dev"

[profiles.Dev]
backend = "local"
cwd = "/tmp/work"

[profiles.Dev.shell]
program = "/bin/zsh"
args = ["-l"]
"#;
        let mut cfg: ConfigFile = toml::from_str(toml).expect("parse");
        cfg.normalize();
        let resolved = cfg.resolve_profile("Dev");
        assert_eq!(resolved.backend, "local");
        assert_eq!(resolved.cwd.as_deref(), Some("/tmp/work"));
        let shell = resolved.shell.expect("shell");
        assert_eq!(shell.program.as_deref(), Some("/bin/zsh"));
        assert_eq!(shell.args, vec!["-l".to_string()]);
    }

    #[test]
    fn set_default_profile_and_roundtrip() {
        let mut cfg = ConfigFile::default();
        cfg.ensure_profile("Work").font_size = Some(16.0);
        assert!(cfg.set_default_profile("Work"));
        assert_eq!(cfg.default_profile_name(), "Work");
        assert!(!cfg.set_default_profile("Missing"));

        let text = toml::to_string_pretty(&cfg).expect("serialize");
        let mut loaded: ConfigFile = toml::from_str(&text).expect("parse");
        loaded.normalize();
        assert_eq!(loaded.default_profile_name(), "Work");
        assert!(loaded.profile("Default").is_some());
        assert_eq!(loaded.resolve_profile("Work").font_size, 16.0);
    }

    #[test]
    fn rename_profile_updates_name_and_default() {
        let mut cfg = ConfigFile::default();
        cfg.ensure_profile("Work").font_size = Some(16.0);
        cfg.set_default_profile("Work");
        assert!(cfg.rename_profile("Work", "Office"));
        assert!(!cfg.profile_names().any(|n| n == "Work"));
        assert!(cfg.profile("Office").is_some());
        assert_eq!(cfg.default_profile_name(), "Office");
        assert!(!cfg.rename_profile("Office", "Default"));
        assert!(!cfg.rename_profile("Missing", "X"));
    }

    #[test]
    fn validate_backend_accepts_local_and_rejects_unknown() {
        let mut cfg = ConfigFile::default();
        assert!(cfg.validate_backend(DEFAULT_PROFILE).is_ok());
        cfg.ensure_profile("Remote").backend = "ssh".to_owned();
        assert!(cfg.validate_backend("Remote").is_err());
    }

    #[test]
    fn an_unreadable_config_is_not_overwritten() {
        let cfg = ConfigFile {
            load_error: Some("bad".to_owned()),
            ..ConfigFile::default()
        };
        let err = cfg.save().expect_err("must not overwrite");
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    }
}
