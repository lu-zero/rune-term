use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::scheme::Scheme;
use crate::theme::{self, Palette};

pub use rune_term_core::font::{clamp_font_size, DEFAULT_FONT_SIZE, MAX_FONT_SIZE, MIN_FONT_SIZE};

pub const DEFAULT_PROFILE: &str = "Default";
pub const DEFAULT_THEME: &str = "Flexoki Black";
const CONFIG_NAME: &str = "config.toml";

/// `config.toml`, `profiles/`, and `themes/` under one directory.
#[derive(Clone, Debug)]
pub struct ConfigFile {
    pub version: u32,
    pub settings: Settings,
    pub profiles: BTreeMap<String, Profile>,
    pub custom_themes: BTreeMap<String, Scheme>,
    /// Set when the directory could not be read. The app runs on defaults, and
    /// saving is refused so the files are not overwritten.
    load_error: Option<String>,
    /// The directory the config is written to. `None` is the platform's.
    dir: Option<PathBuf>,
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
            dir: None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub default_profile: String,
    pub font_size: f32,
    pub theme: String,
    /// When both names are set, the system appearance picks one instead of [`Self::theme`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adaptive_theme: Option<AdaptiveTheme>,
    /// Ask before a close ends a program other than a shell at its prompt.
    pub confirm_close: bool,
    /// What closing a session's tab or last pane does, unless its profile says.
    pub on_close: OnClose,
    /// A right-click goes to a program that reports the mouse instead of opening the menu.
    pub report_right_click: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            default_profile: DEFAULT_PROFILE.to_owned(),
            font_size: DEFAULT_FONT_SIZE,
            theme: DEFAULT_THEME.to_owned(),
            adaptive_theme: None,
            confirm_close: true,
            on_close: OnClose::Kill,
            report_right_click: false,
        }
    }
}

/// A dark scheme and a light scheme. The names are themes, built in or from `themes/`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdaptiveTheme {
    #[serde(default)]
    pub dark: String,
    #[serde(default)]
    pub light: String,
}

impl AdaptiveTheme {
    fn ready(&self) -> bool {
        !self.dark.trim().is_empty() && !self.light.trim().is_empty()
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

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Profile {
    pub font_family: String,
    pub font_size: Option<f32>,
    pub theme: Option<String>,
    /// Backend kind: "local", the default, a shell on a PTY; "tmux" or "rmux", a session.
    pub backend: String,
    pub shell: Option<ShellSpec>,
    pub cwd: Option<String>,
    /// The ssh host a "tmux" or "rmux" session runs on; without one it runs here.
    pub host: Option<String>,
    /// The tmux or rmux server, by its `-L` name; without one, the default server.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server: Option<String>,
    /// How a "tmux" or "rmux" session on a host is reached.
    #[serde(skip_serializing_if = "Transport::is_ssh")]
    pub transport: Transport,
    /// The session a "tmux" or "rmux" profile attaches, created when missing.
    /// Without one each open starts a new session.
    pub session: Option<String>,
    /// What closing a "tmux" or "rmux" tab or pane does to the window; without
    /// one, [`Settings::on_close`] says.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on_close: Option<OnClose>,
    #[serde(skip_serializing_if = "OptionAsAlt::is_none")]
    pub option_as_alt: OptionAsAlt,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OnClose {
    /// The window or pane ends, as iTerm2 and WezTerm close one.
    #[default]
    Kill,
    /// The window keeps running, hidden, and the menu shows it again.
    Detach,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Transport {
    /// Control mode over ssh, for as long as ssh's connection lasts.
    #[default]
    Ssh,
    /// Through rune-term-agent, which ssh starts and QUIC reaches after, roaming.
    Agent,
}

/// The Option keys that send Alt, as Meta, rather than the layout's characters. Only macOS reads it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OptionAsAlt {
    #[default]
    None,
    Left,
    Right,
    Both,
}

impl OptionAsAlt {
    fn is_none(&self) -> bool {
        *self == OptionAsAlt::None
    }
}

impl Transport {
    fn is_ssh(&self) -> bool {
        *self == Transport::Ssh
    }
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
            host: None,
            server: None,
            transport: Transport::Ssh,
            session: None,
            on_close: None,
            option_as_alt: OptionAsAlt::None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Resolved {
    pub font_family: String,
    pub font_size: f32,
    pub theme: String,
    pub backend: String,
    pub shell: Option<ShellSpec>,
    pub cwd: Option<String>,
}

impl ConfigFile {
    /// The Option keys that are Alt in a tab opened with `profile`.
    pub fn option_as_alt(&self, profile: &str) -> OptionAsAlt {
        self.profiles
            .get(profile)
            .or_else(|| self.profiles.get(DEFAULT_PROFILE))
            .map_or(OptionAsAlt::None, |profile| profile.option_as_alt)
    }

    /// What closing a tab of a session opened with `profile` does.
    pub fn on_close(&self, profile: &str) -> OnClose {
        self.profile(profile)
            .and_then(|profile| profile.on_close)
            .unwrap_or(self.settings.on_close)
    }

    pub fn load() -> Self {
        let dir = config_dir();
        match Self::open(&dir) {
            Ok(config) => config,
            Err(err) => {
                eprintln!("rune-term: failed to load {}: {err}", dir.display());
                Self {
                    load_error: Some(err.to_string()),
                    ..Self::default().at_dir(dir)
                }
            }
        }
    }

    /// Reads `config.toml`, `profiles/`, and `themes/` from `dir`.
    pub fn open(dir: &Path) -> io::Result<Self> {
        let root_path = dir.join(CONFIG_NAME);
        let root_text = match fs::read_to_string(&root_path) {
            Ok(text) => Some(text),
            Err(err) if err.kind() == io::ErrorKind::NotFound => None,
            Err(err) => return Err(err),
        };
        if root_text.is_none() && !has_parts(dir) {
            return Ok(Self::default().at_dir(dir));
        }
        let mut config = Self::default();
        config.profiles.clear();
        if let Some(text) = root_text {
            let file: RootFile = toml::from_str(&text)
                .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
            config.version = file.version;
            config.settings = file.settings;
        }
        config.profiles = read_table(&dir.join("profiles"), |text| {
            toml::from_str::<Profile>(text).map_err(|err| err.to_string())
        })?;
        config.custom_themes = read_table(&dir.join("themes"), Scheme::from_toml)?;
        config.normalize();
        Ok(config.at_dir(dir))
    }

    /// Writes the config to this directory instead of the platform's.
    pub fn at_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.dir = Some(dir.into());
        self
    }

    /// The directory the config is written to.
    pub fn dir(&self) -> PathBuf {
        self.dir.clone().unwrap_or_else(config_dir)
    }

    /// Whether a profile with no theme of its own follows the appearance pair.
    pub fn follows_appearance(&self) -> bool {
        self.settings
            .adaptive_theme
            .as_ref()
            .is_some_and(AdaptiveTheme::ready)
    }

    /// The theme the appearance pair names. Anything incomplete uses [`Settings::theme`].
    pub fn appearance_theme(&self, dark: bool) -> String {
        match self.settings.adaptive_theme.as_ref() {
            Some(pair) if pair.ready() => {
                if dark {
                    pair.dark.clone()
                } else {
                    pair.light.clone()
                }
            }
            _ => self.settings.theme.clone(),
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
        let font_family = profile
            .map(|p| p.font_family.trim())
            .filter(|f| !f.is_empty())
            .unwrap_or("Cascadia Code NF")
            .to_owned();
        let font_size = profile
            .and_then(|p| p.font_size)
            .unwrap_or(self.settings.font_size);
        let theme = self.chosen_theme(profile);
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
            font_family,
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
            return custom.to_palette(&name);
        }
        theme::resolve_builtin(&name).unwrap_or_else(theme::default_palette)
    }

    /// A profile theme wins. Otherwise the appearance pair, when both names are set.
    fn chosen_theme(&self, profile: Option<&Profile>) -> String {
        if let Some(name) = profile
            .and_then(|profile| profile.theme.as_deref())
            .map(str::trim)
            .filter(|name| !name.is_empty())
        {
            return name.to_owned();
        }
        if self.follows_appearance() {
            self.appearance_theme(system_dark())
        } else {
            self.settings.theme.clone()
        }
    }

    pub fn set_font_size(&mut self, size: f32) {
        let size = clamp_font_size(size);
        self.settings.font_size = size;
        let name = self.default_profile_name().to_owned();
        self.profiles.entry(name).or_default().font_size = Some(size);
    }

    pub fn set_font_family(&mut self, family: &str) {
        let family = family.trim();
        if family.is_empty() {
            return;
        }
        let name = self.default_profile_name().to_owned();
        self.profiles.entry(name).or_default().font_family = family.to_owned();
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
            Scheme::from_palette(&builtin)
        } else {
            Scheme::default()
        };
        let name = self.unique_theme_name(&format!("{source_name} Copy"));
        self.custom_themes.insert(name.clone(), source);
        self.set_theme(name.clone());
        name
    }

    /// A theme name no built-in or custom theme already holds.
    pub fn unique_theme_name(&self, base: &str) -> String {
        let base = base.trim();
        let mut name = base.to_owned();
        let mut n = 2u32;
        while self.custom_themes.contains_key(&name) || theme::resolve_builtin(&name).is_some() {
            name = format!("{base} {n}");
            n += 1;
        }
        name
    }

    /// A profile name no profile already holds.
    pub fn unique_profile_name(&self, base: &str) -> String {
        let base = base.trim();
        let mut name = base.to_owned();
        let mut n = 2u32;
        while self.profiles.contains_key(&name) {
            name = format!("{base} {n}");
            n += 1;
        }
        name
    }

    /// Renames a custom theme and repoints everything that used it. Built-in
    /// themes cannot be renamed.
    pub fn rename_theme(&mut self, from: &str, to: &str) -> bool {
        let to = to.trim();
        if to.is_empty() || from == to || !name_ok(to) {
            return false;
        }
        if !self.custom_themes.contains_key(from) {
            return false;
        }
        if self.custom_themes.contains_key(to) || theme::resolve_builtin(to).is_some() {
            return false;
        }
        let Some(theme) = self.custom_themes.remove(from) else {
            return false;
        };
        self.custom_themes.insert(to.to_owned(), theme);
        if self.settings.theme == from {
            self.settings.theme = to.to_owned();
        }
        if let Some(pair) = &mut self.settings.adaptive_theme {
            if pair.dark == from {
                pair.dark = to.to_owned();
            }
            if pair.light == from {
                pair.light = to.to_owned();
            }
        }
        for profile in self.profiles.values_mut() {
            if profile.theme.as_deref() == Some(from) {
                profile.theme = Some(to.to_owned());
            }
        }
        true
    }

    /// Removes a custom theme and clears what pointed at it. Built-in themes
    /// cannot be removed.
    pub fn remove_theme(&mut self, name: &str) -> bool {
        if self.custom_themes.remove(name).is_none() {
            return false;
        }
        // A file that only hid a built-in theme leaves the name meaning that theme.
        if theme::resolve_builtin(name).is_some() {
            return true;
        }
        if self.settings.theme == name {
            self.settings.theme = DEFAULT_THEME.to_owned();
        }
        if let Some(pair) = &mut self.settings.adaptive_theme {
            if pair.dark == name {
                pair.dark.clear();
            }
            if pair.light == name {
                pair.light.clear();
            }
        }
        if self
            .settings
            .adaptive_theme
            .as_ref()
            .is_some_and(|pair| pair.dark.trim().is_empty() && pair.light.trim().is_empty())
        {
            self.settings.adaptive_theme = None;
        }
        for profile in self.profiles.values_mut() {
            if profile.theme.as_deref() == Some(name) {
                profile.theme = None;
            }
        }
        true
    }

    /// Adds a custom theme, replacing one with the same name.
    pub fn set_custom_theme(&mut self, name: &str, theme: Scheme) {
        if name_ok(name) {
            self.custom_themes.insert(name.to_owned(), theme);
        }
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

    /// Removes a profile. The last one stays, and the default moves to it.
    pub fn remove_profile(&mut self, name: &str) -> bool {
        if self.profiles.len() <= 1 || !self.profiles.contains_key(name) {
            return false;
        }
        if self.profiles.remove(name).is_none() {
            return false;
        }
        if self.settings.default_profile == name {
            self.settings.default_profile = self
                .profiles
                .keys()
                .next()
                .cloned()
                .unwrap_or_else(|| DEFAULT_PROFILE.to_owned());
        }
        true
    }

    pub fn rename_profile(&mut self, from: &str, to: &str) -> bool {
        let to = to.trim();
        if to.is_empty() || from == to || !name_ok(to) || !self.profiles.contains_key(from) {
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
            "local" | "tmux" | "rmux" => Ok(()),
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
        let dir = self.dir();
        fs::create_dir_all(&dir)?;
        let mut profiles = BTreeMap::new();
        for (name, profile) in &self.profiles {
            if !name_ok(name) {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("'{name}' cannot be a file name"),
                ));
            }
            let text = toml::to_string_pretty(profile)
                .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
            profiles.insert(name.clone(), text);
        }
        let mut themes = BTreeMap::new();
        for (name, scheme) in &self.custom_themes {
            if !name_ok(name) {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("'{name}' cannot be a file name"),
                ));
            }
            let text = scheme
                .to_toml(name)
                .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
            themes.insert(name.clone(), text);
        }
        write_table(&dir.join("profiles"), &profiles)?;
        write_table(&dir.join("themes"), &themes)?;
        let root = RootFile {
            version: self.version,
            settings: self.settings.clone(),
        };
        let text = toml::to_string_pretty(&root)
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
        write_atomic(&dir.join(CONFIG_NAME), &text)
    }
}

/// `config.toml` inside the platform config directory.
pub fn config_path() -> PathBuf {
    config_dir().join(CONFIG_NAME)
}

/// A name that can be the stem of a `profiles/` or `themes/` file.
/// A server name the menu can carry in its ids, and that names one socket file.
pub fn server_ok(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
}

pub fn name_ok(name: &str) -> bool {
    if name.is_empty() || name == "." || name == ".." || name.starts_with('.') {
        return false;
    }
    !name.chars().any(|ch| {
        ch.is_control() || matches!(ch, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|')
    })
}

fn config_dir() -> PathBuf {
    use etcetera::BaseStrategy;
    if let Ok(xdg) = etcetera::base_strategy::Xdg::new() {
        return xdg.config_dir().join("rune-term");
    }
    PathBuf::from(".").join("rune-term")
}

#[derive(Serialize, Deserialize)]
struct RootFile {
    version: u32,
    #[serde(default)]
    settings: Settings,
}

fn has_parts(dir: &Path) -> bool {
    dir.join("profiles").is_dir() || dir.join("themes").is_dir()
}

fn read_table<T>(
    dir: &Path,
    parse: impl Fn(&str) -> Result<T, String>,
) -> io::Result<BTreeMap<String, T>> {
    let mut out = BTreeMap::new();
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(out),
        Err(err) => return Err(err),
    };
    for entry in entries {
        let path = entry?.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("toml") {
            continue;
        }
        let Some(name) = path.file_stem().and_then(|stem| stem.to_str()) else {
            continue;
        };
        if !name_ok(name) {
            continue;
        }
        let text = fs::read_to_string(&path)?;
        let value = parse(&text).map_err(|err| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("{}: {err}", path.display()),
            )
        })?;
        out.insert(name.to_owned(), value);
    }
    Ok(out)
}

fn write_table(dir: &Path, files: &BTreeMap<String, String>) -> io::Result<()> {
    fs::create_dir_all(dir)?;
    for (name, text) in files {
        write_atomic(&dir.join(format!("{name}.toml")), text)?;
    }
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("toml") {
            continue;
        }
        let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) else {
            continue;
        };
        if name_ok(stem) && !files.contains_key(stem) {
            fs::remove_file(path)?;
        }
    }
    Ok(())
}

fn write_atomic(path: &Path, text: &str) -> io::Result<()> {
    let tmp = path.with_extension("toml.tmp");
    fs::write(&tmp, text)?;
    fs::rename(&tmp, path)?;
    Ok(())
}

/// `defaults` prints `Dark` and fails when the desktop is light. Unknown stays dark.
fn system_dark() -> bool {
    if !cfg!(target_os = "macos") {
        return true;
    }
    match std::process::Command::new("defaults")
        .args(["read", "-g", "AppleInterfaceStyle"])
        .output()
    {
        Ok(output) => {
            output.status.success()
                && String::from_utf8_lossy(&output.stdout)
                    .to_ascii_lowercase()
                    .contains("dark")
        }
        Err(_) => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removing_a_profile_moves_the_default_and_spares_the_last_one() {
        let mut cfg = ConfigFile::default();
        cfg.ensure_profile("Work");
        cfg.set_default_profile("Work");
        assert!(cfg.remove_profile("Work"));
        assert!(cfg.profile("Work").is_none());
        assert_eq!(cfg.default_profile_name(), DEFAULT_PROFILE);
        assert!(!cfg.remove_profile(DEFAULT_PROFILE));
        assert!(cfg.profile(DEFAULT_PROFILE).is_some());
    }

    #[test]
    fn a_tab_whose_profile_is_gone_takes_the_default_profiles_option_keys() {
        let mut cfg = ConfigFile::default();
        cfg.ensure_profile("Work");
        cfg.profiles.get_mut(DEFAULT_PROFILE).unwrap().option_as_alt = OptionAsAlt::Left;
        cfg.profiles.get_mut("Work").unwrap().option_as_alt = OptionAsAlt::Both;
        assert_eq!(cfg.option_as_alt("Work"), OptionAsAlt::Both);
        assert!(cfg.remove_profile("Work"));
        assert_eq!(cfg.option_as_alt("Work"), OptionAsAlt::Left);
    }

    #[test]
    fn renaming_a_theme_repoints_what_used_it() {
        let mut cfg = ConfigFile::default();
        let copy = cfg.copy_theme("Dracula");
        cfg.ensure_profile("Work").theme = Some(copy.clone());
        cfg.settings.adaptive_theme = Some(AdaptiveTheme {
            dark: copy.clone(),
            light: "Flexoki Light".to_owned(),
        });
        assert!(cfg.rename_theme(&copy, "Night"));
        assert!(cfg.custom_themes.contains_key("Night"));
        assert!(!cfg.custom_themes.contains_key(&copy));
        assert_eq!(cfg.settings.theme, "Night");
        assert_eq!(
            cfg.settings
                .adaptive_theme
                .as_ref()
                .map(|pair| pair.dark.as_str()),
            Some("Night")
        );
        assert_eq!(
            cfg.profile("Work").and_then(|p| p.theme.clone()),
            Some("Night".to_owned())
        );
    }

    #[test]
    fn removing_a_theme_clears_what_pointed_at_it() {
        let mut cfg = ConfigFile::default();
        let copy = cfg.copy_theme("Dracula");
        cfg.ensure_profile("Work").theme = Some(copy.clone());
        cfg.settings.adaptive_theme = Some(AdaptiveTheme {
            dark: copy.clone(),
            light: copy.clone(),
        });
        assert!(cfg.remove_theme(&copy));
        assert!(cfg.custom_themes.is_empty());
        assert_eq!(cfg.settings.theme, DEFAULT_THEME);
        assert!(cfg.settings.adaptive_theme.is_none());
        assert!(cfg.profile("Work").and_then(|p| p.theme.clone()).is_none());
        assert!(!cfg.remove_theme("Dracula"));
    }

    #[test]
    fn a_free_name_does_not_collide_with_a_builtin_or_a_profile() {
        let mut cfg = ConfigFile::default();
        cfg.ensure_profile("Work");
        assert_eq!(cfg.unique_profile_name("Work"), "Work 2");
        assert_eq!(cfg.unique_theme_name("Dracula"), "Dracula 2");
        cfg.set_custom_theme("Dracula 2", Scheme::default());
        assert_eq!(cfg.unique_theme_name("Dracula"), "Dracula 3");
    }

    #[test]
    fn a_name_that_cannot_be_a_file_is_refused() {
        let mut cfg = ConfigFile::default();
        cfg.ensure_profile("Work");
        assert!(!cfg.rename_profile("Work", "a/b"));
        assert!(cfg.profile("Work").is_some());
        let copy = cfg.copy_theme("Dracula");
        assert!(!cfg.rename_theme(&copy, ".hidden"));
        assert!(cfg.is_custom_theme(&copy));
    }

    #[test]
    fn the_config_roundtrips_through_its_directory() {
        let dir = scratch("roundtrip");
        let mut cfg = ConfigFile::default().at_dir(&dir);
        cfg.ensure_profile("Work").font_size = Some(18.0);
        cfg.settings.adaptive_theme = Some(AdaptiveTheme {
            dark: "Flexoki Black".to_owned(),
            light: "Flexoki Light".to_owned(),
        });
        cfg.set_custom_theme("Night", Scheme::default());
        cfg.save().expect("save");
        let loaded = ConfigFile::open(&dir).expect("open");
        assert_eq!(loaded.profile("Work").and_then(|p| p.font_size), Some(18.0));
        assert_eq!(loaded.settings.adaptive_theme, cfg.settings.adaptive_theme);
        assert_eq!(loaded.custom_themes.get("Night"), Some(&Scheme::default()));
        assert!(dir.join("profiles/Work.toml").is_file());
        assert!(dir.join("themes/Night.toml").is_file());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_missing_config_still_loads_theme_files() {
        let dir = scratch("dropped");
        fs::create_dir_all(dir.join("themes")).unwrap();
        let scheme = Scheme::default();
        fs::write(
            dir.join("themes/Night.toml"),
            scheme.to_toml("Night").unwrap(),
        )
        .unwrap();
        let cfg = ConfigFile::open(&dir).unwrap();
        assert!(cfg.custom_themes.contains_key("Night"));
        assert!(cfg.profile(DEFAULT_PROFILE).is_some());
        cfg.save().unwrap();
        assert!(dir.join("themes/Night.toml").is_file());
        assert!(dir.join("config.toml").is_file());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_bad_theme_file_is_refused() {
        let dir = scratch("bad-theme");
        fs::create_dir_all(dir.join("themes")).unwrap();
        fs::write(dir.join("themes/Bad.toml"), "colors = 1\n").unwrap();
        let err = ConfigFile::open(&dir).expect_err("bad theme");
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn saving_removes_a_profile_file_that_was_deleted() {
        let dir = scratch("delete-profile");
        let mut cfg = ConfigFile::default().at_dir(&dir);
        cfg.ensure_profile("Work");
        cfg.save().unwrap();
        assert!(dir.join("profiles/Work.toml").is_file());
        assert!(cfg.remove_profile("Work"));
        cfg.save().unwrap();
        assert!(!dir.join("profiles/Work.toml").exists());
        assert!(dir
            .join("profiles")
            .join(format!("{DEFAULT_PROFILE}.toml"))
            .is_file());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_theme_file_hides_the_builtin_until_it_is_removed() {
        let mut cfg = ConfigFile::default();
        cfg.set_custom_theme("Dracula", Scheme::default());
        cfg.set_theme("Dracula".to_owned());
        assert_eq!(cfg.resolve_palette().bg, Scheme::default().background);
        assert!(cfg.remove_theme("Dracula"));
        assert_eq!(cfg.settings.theme, "Dracula");
        assert!(!cfg.is_custom_theme("Dracula"));
        assert_ne!(cfg.resolve_palette().bg, Scheme::default().background);
    }

    #[test]
    fn the_appearance_pair_is_used_when_the_profile_has_no_theme() {
        let mut cfg = ConfigFile::default();
        cfg.settings.adaptive_theme = Some(AdaptiveTheme {
            dark: "Nord".to_owned(),
            light: "Flexoki Light".to_owned(),
        });
        assert_eq!(cfg.appearance_theme(true), "Nord");
        assert_eq!(cfg.appearance_theme(false), "Flexoki Light");
        cfg.ensure_profile("Work").theme = Some("Dracula".to_owned());
        assert_eq!(cfg.resolve_profile("Work").theme, "Dracula");
        let chosen = cfg.resolve_profile(DEFAULT_PROFILE).theme;
        assert!(chosen == "Nord" || chosen == "Flexoki Light");

        cfg.settings.adaptive_theme = Some(AdaptiveTheme {
            dark: "Nord".to_owned(),
            light: String::new(),
        });
        assert_eq!(cfg.appearance_theme(false), cfg.settings.theme);
        assert_eq!(
            cfg.resolve_profile(DEFAULT_PROFILE).theme,
            cfg.settings.theme
        );
    }

    #[test]
    fn default_config_has_default_profile() {
        let cfg = ConfigFile::default();
        assert_eq!(cfg.default_profile_name(), DEFAULT_PROFILE);
        assert!(cfg.profile(DEFAULT_PROFILE).is_some());
        let resolved = cfg.resolve();
        assert_eq!(resolved.font_size, DEFAULT_FONT_SIZE);
    }

    #[test]
    fn profiles_resolve_from_their_files() {
        let dir = scratch("profiles");
        fs::write(
            dir.join("config.toml"),
            r#"
version = 1

[settings]
default_profile = "Work"
font_size = 14.0
theme = "Flexoki Black"
"#,
        )
        .unwrap();
        fs::create_dir_all(dir.join("profiles")).unwrap();
        fs::write(
            dir.join("profiles/Default.toml"),
            "font_family = \"Cascadia Code NF\"\nfont_size = 14.0\n",
        )
        .unwrap();
        fs::write(
            dir.join("profiles/Work.toml"),
            r#"
font_family = "Cascadia Code NF"
font_size = 18.0
theme = "Flexoki Light"
"#,
        )
        .unwrap();
        let cfg = ConfigFile::open(&dir).unwrap();
        assert_eq!(cfg.default_profile_name(), "Work");
        let names: Vec<_> = cfg.profile_names().collect();
        assert!(names.contains(&"Default"));
        assert!(names.contains(&"Work"));
        let work = cfg.resolve_profile("Work");
        assert_eq!(work.font_size, 18.0);
        assert_eq!(work.theme, "Flexoki Light");
        assert_eq!(cfg.resolve_profile("Default").font_size, 14.0);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_default_profile_falls_back() {
        let dir = scratch("missing-default");
        fs::write(
            dir.join("config.toml"),
            r#"
version = 1

[settings]
default_profile = "Missing"
font_size = 15.0
"#,
        )
        .unwrap();
        fs::create_dir_all(dir.join("profiles")).unwrap();
        fs::write(dir.join("profiles/Only.toml"), "font_size = 12.0\n").unwrap();
        let cfg = ConfigFile::open(&dir).unwrap();
        assert_eq!(cfg.default_profile_name(), "Only");
        assert_eq!(cfg.resolve().font_size, 12.0);
        let _ = fs::remove_dir_all(&dir);
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
        let dir = scratch("launch");
        fs::write(
            dir.join("config.toml"),
            "version = 1\n\n[settings]\ndefault_profile = \"Dev\"\n",
        )
        .unwrap();
        fs::create_dir_all(dir.join("profiles")).unwrap();
        fs::write(
            dir.join("profiles/Dev.toml"),
            r#"
backend = "local"
cwd = "/tmp/work"

[shell]
program = "/bin/zsh"
args = ["-l"]
"#,
        )
        .unwrap();
        let cfg = ConfigFile::open(&dir).unwrap();
        let resolved = cfg.resolve_profile("Dev");
        assert_eq!(resolved.backend, "local");
        assert_eq!(resolved.cwd.as_deref(), Some("/tmp/work"));
        let shell = resolved.shell.expect("shell");
        assert_eq!(shell.program.as_deref(), Some("/bin/zsh"));
        assert_eq!(shell.args, vec!["-l".to_string()]);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn set_default_profile_and_roundtrip() {
        let dir = scratch("default-profile");
        let mut cfg = ConfigFile::default().at_dir(&dir);
        cfg.ensure_profile("Work").font_size = Some(16.0);
        assert!(cfg.set_default_profile("Work"));
        assert_eq!(cfg.default_profile_name(), "Work");
        assert!(!cfg.set_default_profile("Missing"));
        cfg.save().unwrap();
        let loaded = ConfigFile::open(&dir).unwrap();
        assert_eq!(loaded.default_profile_name(), "Work");
        assert!(loaded.profile("Default").is_some());
        assert_eq!(loaded.resolve_profile("Work").font_size, 16.0);
        let _ = fs::remove_dir_all(&dir);
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("rune-term-settings-{name}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("scratch");
        dir
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
    fn validate_backend_accepts_local_and_sessions_and_rejects_unknown() {
        let mut cfg = ConfigFile::default();
        assert!(cfg.validate_backend(DEFAULT_PROFILE).is_ok());
        cfg.ensure_profile("Work").backend = "rmux".to_owned();
        assert!(cfg.validate_backend("Work").is_ok());
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
