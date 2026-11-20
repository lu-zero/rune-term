//! A color scheme in WezTerm's TOML shape: a `[colors]` table and `[metadata]`.
//!
//! <https://wezterm.org/config/appearance.html#defining-a-color-scheme-in-a-separate-file>
//!
//! A file needs eight `ansi` colors. Anything else may be omitted. `brights`
//! fills the upper eight. Omitted cursor and selection colors invert foreground
//! and background. `foreground_bright`, `muted`, and `accent` fall back to
//! bright white, bright black, and blue. Colors are `#RGB`, `#RRGGBB`,
//! `#RRGGBBAA`, or `rgb()`.
//!
//! Indexes 16–255 in `indexed` replace the xterm cube and gray ramp.

use std::collections::BTreeMap;
use std::sync::Arc;

use serde::Deserialize;

use crate::theme::{self, Palette};

/// Whether a scheme is drawn on a dark or a light background.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Appearance {
    #[default]
    Dark,
    Light,
}

impl Appearance {
    pub fn parse(text: &str) -> Option<Self> {
        match text.trim().to_ascii_lowercase().as_str() {
            "dark" => Some(Self::Dark),
            "light" => Some(Self::Light),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Dark => "dark",
            Self::Light => "light",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Dark => "Dark",
            Self::Light => "Light",
        }
    }

    pub fn flip(self) -> Self {
        match self {
            Self::Dark => Self::Light,
            Self::Light => Self::Dark,
        }
    }

    /// A theme file without metadata still gets an appearance from its background.
    pub fn infer(background: u32) -> Self {
        let red = ((background >> 16) & 0xff) as f32;
        let green = ((background >> 8) & 0xff) as f32;
        let blue = (background & 0xff) as f32;
        let luma = (0.2126 * red + 0.7152 * green + 0.0722 * blue) / 255.0;
        if luma > 0.5 {
            Self::Light
        } else {
            Self::Dark
        }
    }
}

/// The colors a theme file edits. `foreground_bright`, `muted`, and `accent`
/// are rune-term's; a WezTerm file that omits them is filled from the ANSI slots.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Scheme {
    pub foreground: u32,
    pub background: u32,
    pub fg_bright: u32,
    pub muted: u32,
    pub accent: u32,
    pub cursor_fg: u32,
    pub cursor_bg: u32,
    pub selection_fg: u32,
    pub selection_bg: u32,
    pub split: u32,
    pub scrollbar_thumb: u32,
    pub tab_bar: u32,
    pub ansi: [[u8; 3]; 16],
    pub indexed: BTreeMap<u8, [u8; 3]>,
    pub author: String,
    pub url: String,
    pub appearance: Appearance,
}

impl Default for Scheme {
    fn default() -> Self {
        Self::from_palette(theme::default_palette().as_ref())
    }
}

impl Scheme {
    pub fn from_palette(palette: &Palette) -> Self {
        Self {
            foreground: palette.fg,
            background: palette.bg,
            fg_bright: palette.fg_bright,
            muted: palette.muted,
            accent: palette.accent,
            cursor_fg: palette.cursor_fg,
            cursor_bg: palette.cursor_bg,
            selection_fg: palette.selection_fg,
            selection_bg: palette.selection_bg,
            split: palette.active_border,
            scrollbar_thumb: palette.scrollbar,
            tab_bar: palette.tab_bg,
            ansi: palette.ansi16,
            indexed: palette.indexed.clone(),
            author: palette.author.clone(),
            url: palette.url.clone(),
            appearance: Appearance::infer(palette.bg),
        }
    }

    pub fn to_palette(&self, name: &str) -> Arc<Palette> {
        Arc::new(Palette {
            name: name.to_owned(),
            bg: self.background,
            fg: self.foreground,
            fg_bright: self.fg_bright,
            muted: self.muted,
            tab_bg: self.tab_bar,
            active_border: self.split,
            accent: self.accent,
            ansi16: self.ansi,
            cursor_fg: self.cursor_fg,
            cursor_bg: self.cursor_bg,
            selection_fg: self.selection_fg,
            selection_bg: self.selection_bg,
            scrollbar: self.scrollbar_thumb,
            indexed: self.indexed.clone(),
            author: self.author.clone(),
            url: self.url.clone(),
        })
    }

    pub fn color(&self, slot: Slot) -> u32 {
        match slot {
            Slot::Background => self.background,
            Slot::Foreground => self.foreground,
            Slot::BrightForeground => self.fg_bright,
            Slot::Muted => self.muted,
            Slot::Accent => self.accent,
            Slot::Cursor => self.cursor_bg,
            Slot::CursorText => self.cursor_fg,
            Slot::Selection => self.selection_bg,
            Slot::SelectionText => self.selection_fg,
            Slot::Split => self.split,
            Slot::Tabs => self.tab_bar,
            Slot::Scrollbar => self.scrollbar_thumb,
            Slot::Ansi(index) => rgb_u32(self.ansi[usize::from(index).min(15)]),
            Slot::Indexed(index) => self.indexed.get(&index).copied().map(rgb_u32).unwrap_or(0),
        }
    }

    pub fn set_color(&mut self, slot: Slot, rgb: u32) {
        match slot {
            Slot::Background => self.background = rgb,
            Slot::Foreground => self.foreground = rgb,
            Slot::BrightForeground => self.fg_bright = rgb,
            Slot::Muted => self.muted = rgb,
            Slot::Accent => self.accent = rgb,
            Slot::Cursor => self.cursor_bg = rgb,
            Slot::CursorText => self.cursor_fg = rgb,
            Slot::Selection => self.selection_bg = rgb,
            Slot::SelectionText => self.selection_fg = rgb,
            Slot::Split => self.split = rgb,
            Slot::Tabs => self.tab_bar = rgb,
            Slot::Scrollbar => self.scrollbar_thumb = rgb,
            Slot::Ansi(index) => self.ansi[usize::from(index).min(15)] = theme::hex(rgb),
            Slot::Indexed(index) if index >= 16 => {
                self.indexed.insert(index, theme::hex(rgb));
            }
            Slot::Indexed(_) => {}
        }
    }

    pub fn from_toml(text: &str) -> Result<Self, String> {
        let file: File = toml::from_str(text).map_err(|err| err.to_string())?;
        let ansi = eight(&file.colors.ansi, "ansi")?;
        let brights = match &file.colors.brights {
            Some(colors) => eight(colors, "brights")?,
            None => ansi,
        };
        let mut slots = [[0u8; 3]; 16];
        slots[..8].copy_from_slice(&ansi);
        slots[8..].copy_from_slice(&brights);
        let foreground = optional(file.colors.foreground.as_deref(), 0x00CC_CCCC)?;
        let background = optional(file.colors.background.as_deref(), 0x0000_0000)?;
        let cursor_fg = optional(file.colors.cursor_fg.as_deref(), background)?;
        let cursor_bg = optional(file.colors.cursor_bg.as_deref(), foreground)?;
        let selection_fg = optional(file.colors.selection_fg.as_deref(), background)?;
        let selection_bg = optional(file.colors.selection_bg.as_deref(), foreground)?;
        let split = optional(file.colors.split.as_deref(), rgb_u32(slots[8]))?;
        let scrollbar_thumb = optional(file.colors.scrollbar_thumb.as_deref(), rgb_u32(slots[8]))?;
        let fg_bright = optional(file.colors.foreground_bright.as_deref(), rgb_u32(slots[15]))?;
        let muted = optional(file.colors.muted.as_deref(), rgb_u32(slots[8]))?;
        let accent = optional(file.colors.accent.as_deref(), rgb_u32(slots[4]))?;
        let tab_bar = match file
            .colors
            .tab_bar
            .as_ref()
            .and_then(|bar| bar.background.as_deref())
        {
            Some(color) => parse_color(color)?,
            None => background,
        };
        let mut indexed = BTreeMap::new();
        for (key, color) in file.colors.indexed {
            let index: u8 = key
                .parse()
                .map_err(|_| format!("indexed color '{key}' is not 16–255"))?;
            if index < 16 {
                return Err(format!("indexed color {index} belongs in ansi or brights"));
            }
            indexed.insert(index, theme::hex(parse_color(&color)?));
        }
        Ok(Self {
            foreground,
            background,
            fg_bright,
            muted,
            accent,
            cursor_fg,
            cursor_bg,
            selection_fg,
            selection_bg,
            split,
            scrollbar_thumb,
            tab_bar,
            ansi: slots,
            indexed,
            author: file.metadata.author.unwrap_or_default(),
            url: file.metadata.origin_url.unwrap_or_default(),
            appearance: file
                .metadata
                .appearance
                .as_deref()
                .and_then(Appearance::parse)
                .unwrap_or_else(|| Appearance::infer(background)),
        })
    }

    pub fn to_toml(&self, name: &str) -> Result<String, String> {
        let mut colors = toml::map::Map::new();
        colors.insert("foreground".into(), hex_value(self.foreground));
        colors.insert("background".into(), hex_value(self.background));
        colors.insert("foreground_bright".into(), hex_value(self.fg_bright));
        colors.insert("muted".into(), hex_value(self.muted));
        colors.insert("accent".into(), hex_value(self.accent));
        colors.insert("cursor_fg".into(), hex_value(self.cursor_fg));
        colors.insert("cursor_bg".into(), hex_value(self.cursor_bg));
        colors.insert("selection_fg".into(), hex_value(self.selection_fg));
        colors.insert("selection_bg".into(), hex_value(self.selection_bg));
        colors.insert("split".into(), hex_value(self.split));
        colors.insert("scrollbar_thumb".into(), hex_value(self.scrollbar_thumb));
        colors.insert(
            "ansi".into(),
            toml::Value::Array(self.ansi[..8].iter().copied().map(hex_bytes).collect()),
        );
        colors.insert(
            "brights".into(),
            toml::Value::Array(self.ansi[8..].iter().copied().map(hex_bytes).collect()),
        );
        if !self.indexed.is_empty() {
            let mut indexed = toml::map::Map::new();
            for (index, rgb) in &self.indexed {
                indexed.insert(index.to_string(), hex_bytes(*rgb));
            }
            colors.insert("indexed".into(), toml::Value::Table(indexed));
        }
        let mut tab_bar = toml::map::Map::new();
        tab_bar.insert("background".into(), hex_value(self.tab_bar));
        colors.insert("tab_bar".into(), toml::Value::Table(tab_bar));
        let mut metadata = toml::map::Map::new();
        metadata.insert("name".into(), toml::Value::String(name.to_owned()));
        metadata.insert(
            "appearance".into(),
            toml::Value::String(self.appearance.as_str().to_owned()),
        );
        if !self.author.is_empty() {
            metadata.insert("author".into(), toml::Value::String(self.author.clone()));
        }
        if !self.url.is_empty() {
            metadata.insert("origin_url".into(), toml::Value::String(self.url.clone()));
        }
        let mut root = toml::map::Map::new();
        root.insert("colors".into(), toml::Value::Table(colors));
        root.insert("metadata".into(), toml::Value::Table(metadata));
        toml::to_string_pretty(&toml::Value::Table(root)).map_err(|err| err.to_string())
    }
}

/// One color the editor can change.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Slot {
    Background,
    Foreground,
    BrightForeground,
    Muted,
    Accent,
    Cursor,
    CursorText,
    Selection,
    SelectionText,
    Split,
    Tabs,
    Scrollbar,
    Ansi(u8),
    /// A palette index from 16 to 255.
    Indexed(u8),
}

impl Slot {
    pub const CHROME: [Slot; 12] = [
        Slot::Background,
        Slot::Foreground,
        Slot::BrightForeground,
        Slot::Muted,
        Slot::Accent,
        Slot::Cursor,
        Slot::CursorText,
        Slot::Selection,
        Slot::SelectionText,
        Slot::Split,
        Slot::Tabs,
        Slot::Scrollbar,
    ];

    pub fn label(self) -> String {
        match self {
            Slot::Background => "Background".to_owned(),
            Slot::Foreground => "Foreground".to_owned(),
            Slot::BrightForeground => "Bright foreground".to_owned(),
            Slot::Muted => "Muted".to_owned(),
            Slot::Accent => "Accent".to_owned(),
            Slot::Cursor => "Cursor".to_owned(),
            Slot::CursorText => "Cursor text".to_owned(),
            Slot::Selection => "Selection".to_owned(),
            Slot::SelectionText => "Selection text".to_owned(),
            Slot::Split => "Split".to_owned(),
            Slot::Tabs => "Tabs".to_owned(),
            Slot::Scrollbar => "Scrollbar".to_owned(),
            Slot::Ansi(index) => ansi_label(index).to_owned(),
            Slot::Indexed(index) => format!("Index {index}"),
        }
    }
}

fn ansi_label(index: u8) -> &'static str {
    match index {
        0 => "Black",
        1 => "Red",
        2 => "Green",
        3 => "Yellow",
        4 => "Blue",
        5 => "Magenta",
        6 => "Cyan",
        7 => "White",
        8 => "Bright black",
        9 => "Bright red",
        10 => "Bright green",
        11 => "Bright yellow",
        12 => "Bright blue",
        13 => "Bright magenta",
        14 => "Bright cyan",
        _ => "Bright white",
    }
}

fn eight(colors: &[String], name: &str) -> Result<[[u8; 3]; 8], String> {
    if colors.len() != 8 {
        return Err(format!("{name} needs 8 colors"));
    }
    let mut out = [[0u8; 3]; 8];
    for (slot, color) in out.iter_mut().zip(colors) {
        *slot = theme::hex(parse_color(color)?);
    }
    Ok(out)
}

fn optional(color: Option<&str>, fallback: u32) -> Result<u32, String> {
    match color {
        Some(color) => parse_color(color),
        None => Ok(fallback),
    }
}

fn parse_color(text: &str) -> Result<u32, String> {
    let text = text.trim();
    if let Some(rgb) = hex_color(text).or_else(|| rgb_fn(text)) {
        return Ok(rgb);
    }
    Err(format!("'{text}' is not a #RRGGBB or rgb() color"))
}

fn hex_color(text: &str) -> Option<u32> {
    let body = text.strip_prefix('#')?;
    let expanded;
    let digits: &str = match body.len() {
        3 | 4 => {
            expanded = body
                .chars()
                .take(3)
                .flat_map(|ch| [ch, ch])
                .collect::<String>();
            &expanded
        }
        6 | 8 => &body[..6],
        _ => return None,
    };
    let packed = u32::from_str_radix(digits, 16).ok()?;
    Some(packed)
}

fn rgb_fn(text: &str) -> Option<u32> {
    let rest = text
        .strip_prefix("rgba(")
        .or_else(|| text.strip_prefix("rgb("))?
        .strip_suffix(')')?;
    let parts: Vec<&str> = rest
        .split(|ch: char| ch == ',' || ch == '/' || ch.is_whitespace())
        .filter(|part| !part.is_empty())
        .collect();
    if parts.len() < 3 {
        return None;
    }
    let channel = |text: &str| -> Option<u8> {
        if let Some(percent) = text.strip_suffix('%') {
            let value: f32 = percent.parse().ok()?;
            return Some((value / 100.0 * 255.0).round().clamp(0.0, 255.0) as u8);
        }
        text.parse().ok()
    };
    Some(rgb_u32([
        channel(parts[0])?,
        channel(parts[1])?,
        channel(parts[2])?,
    ]))
}

fn rgb_u32(rgb: [u8; 3]) -> u32 {
    (u32::from(rgb[0]) << 16) | (u32::from(rgb[1]) << 8) | u32::from(rgb[2])
}

fn hex_value(rgb: u32) -> toml::Value {
    toml::Value::String(format!("#{:06X}", rgb & 0x00FF_FFFF))
}

fn hex_bytes(rgb: [u8; 3]) -> toml::Value {
    hex_value(rgb_u32(rgb))
}

#[derive(Deserialize)]
struct File {
    colors: ColorsFile,
    #[serde(default)]
    metadata: MetaFile,
}

#[derive(Deserialize)]
struct ColorsFile {
    #[serde(default)]
    foreground: Option<String>,
    #[serde(default)]
    background: Option<String>,
    #[serde(default)]
    cursor_fg: Option<String>,
    #[serde(default)]
    cursor_bg: Option<String>,
    #[serde(default)]
    selection_fg: Option<String>,
    #[serde(default)]
    selection_bg: Option<String>,
    #[serde(default)]
    split: Option<String>,
    #[serde(default)]
    scrollbar_thumb: Option<String>,
    #[serde(default)]
    foreground_bright: Option<String>,
    #[serde(default)]
    muted: Option<String>,
    #[serde(default)]
    accent: Option<String>,
    ansi: Vec<String>,
    #[serde(default)]
    brights: Option<Vec<String>>,
    #[serde(default)]
    indexed: BTreeMap<String, String>,
    #[serde(default)]
    tab_bar: Option<TabBarFile>,
}

#[derive(Deserialize, Default)]
struct TabBarFile {
    #[serde(default)]
    background: Option<String>,
}

#[derive(Deserialize, Default)]
struct MetaFile {
    #[serde(default)]
    author: Option<String>,
    #[serde(default)]
    origin_url: Option<String>,
    #[serde(default)]
    appearance: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_wezterm_scheme_roundtrips_the_colors_it_names() {
        let text = r##"
[colors]
foreground = "#CECDC3"
background = "#000000"
cursor_fg = "#000000"
cursor_bg = "#CECDC3"
selection_fg = "#000000"
selection_bg = "#CECDC3"
ansi = ["#100F0F", "#AF3029", "#66800B", "#AD8301", "#205EA6", "#A02F6F", "#24837B", "#CECDC3"]
brights = ["#6F6E69", "#D14D41", "#879A39", "#D0A215", "#4385BE", "#CE5D97", "#3AA99F", "#E6E4D9"]
split = "#343331"
scrollbar_thumb = "#878580"

[colors.indexed]
16 = "#E8BBD0"

[colors.tab_bar]
background = "#1C1B1A"

[metadata]
name = "Night"
author = "Ada"
origin_url = "https://example.com"
"##;
        let scheme = Scheme::from_toml(text).expect("parse");
        assert_eq!(scheme.foreground, 0x00CE_CDC3);
        assert_eq!(scheme.background, 0);
        assert_eq!(scheme.cursor_fg, 0);
        assert_eq!(scheme.ansi[4], theme::hex(0x0020_5EA6));
        assert_eq!(scheme.ansi[8], theme::hex(0x006F_6E69));
        assert_eq!(scheme.indexed[&16], theme::hex(0x00E8_BBD0));
        assert_eq!(scheme.tab_bar, 0x001C_1B1A);
        assert_eq!(scheme.author, "Ada");
        let again = Scheme::from_toml(&scheme.to_toml("Night").expect("write")).expect("reread");
        assert_eq!(again, scheme);
        let palette = scheme.to_palette("Night");
        assert_eq!(palette.accent, 0x0020_5EA6);
        assert_eq!(palette.muted, 0x006F_6E69);
        assert_eq!(palette.fg_bright, 0x00E6_E4D9);
    }

    #[test]
    fn a_copied_palette_keeps_a_muted_color_that_is_not_bright_black() {
        let palette = theme::default_palette();
        let scheme = Scheme::from_palette(palette.as_ref());
        assert_eq!(scheme.muted, palette.muted);
        assert_ne!(rgb_u32(scheme.ansi[8]), palette.muted);
        assert_eq!(scheme.to_palette(&palette.name).muted, palette.muted);
    }

    #[test]
    fn a_named_chrome_color_overrides_the_ansi_slot() {
        let text = r##"
[colors]
muted = "#112233"
accent = "#ABCDEF"
ansi = ["#010101", "#020202", "#030303", "#040404", "#050505", "#060606", "#070707", "#080808"]
brights = ["#111111", "#000000", "#000000", "#000000", "#000000", "#000000", "#000000", "#FFFFFF"]
"##;
        let scheme = Scheme::from_toml(text).expect("parse");
        assert_eq!(scheme.muted, 0x0011_2233);
        assert_eq!(scheme.accent, 0x00AB_CDEF);
        assert_eq!(scheme.fg_bright, 0x00FF_FFFF);
        assert_eq!(scheme.color(Slot::Muted), 0x0011_2233);
        let again = Scheme::from_toml(&scheme.to_toml("Night").expect("write")).expect("reread");
        assert_eq!(again.muted, scheme.muted);
        assert_eq!(again.accent, scheme.accent);
        assert_eq!(again.fg_bright, scheme.fg_bright);
    }

    #[test]
    fn appearance_is_written_and_inferred_for_older_files() {
        let text = r##"
[colors]
background = "#F0F0F0"
ansi = ["#010101", "#020202", "#030303", "#040404", "#050505", "#060606", "#070707", "#080808"]
"##;
        let scheme = Scheme::from_toml(text).expect("parse");
        assert_eq!(scheme.appearance, Appearance::Light);
        let written = scheme.to_toml("Paper").expect("write");
        assert!(written.contains("appearance = \"light\""));
        assert_eq!(
            Scheme::from_toml(&written).expect("reread").appearance,
            Appearance::Light
        );
        let dark = written.replace("appearance = \"light\"", "appearance = \"dark\"");
        assert_eq!(
            Scheme::from_toml(&dark).expect("dark").appearance,
            Appearance::Dark
        );
    }
}
