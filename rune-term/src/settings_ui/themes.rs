//! The themes tab: the editor for the selected theme and the operations on it.
//! A built-in theme is read-only. A theme from `themes/` can be renamed,
//! recolored and removed, and a file that shares a built-in name replaces it.

use std::sync::Arc;

use std::collections::BTreeMap;

use rune::{col, el, row, text, El, ScrollSpec, TextEdit};
use rune_kit::{Field, Kit, Popover, SegmentedControl, TextInput};

use super::SettingsModel;
use crate::scheme::{Appearance, Scheme, Slot};
use crate::settings::ConfigFile;
use crate::theme::{self, Palette};

const SWATCH: f32 = 28.0;
const SWATCH_GAP: f32 = 6.0;
const CELL: f32 = 12.0;
const CELL_GAP: f32 = 2.0;
/// Six cells and the gaps between them. A group does not wrap on its own.
const GROUP: f32 = CELL * 6.0 + CELL_GAP * 5.0;
/// Three of these share the default editor row. Two no longer fit at the window's minimum, so the fields stack.
const FIELD_MIN: f32 = 160.0;

const BASIC: [Slot; 7] = [
    Slot::Background,
    Slot::Foreground,
    Slot::BrightForeground,
    Slot::Cursor,
    Slot::CursorText,
    Slot::Selection,
    Slot::SelectionText,
];

const UI: [Slot; 5] = [
    Slot::Muted,
    Slot::Accent,
    Slot::Split,
    Slot::Tabs,
    Slot::Scrollbar,
];

/// The theme being edited.
pub struct ThemeDraft {
    /// The theme as it was loaded, for revert. `None` for a built-in theme,
    /// which cannot be changed.
    saved: Option<Scheme>,
    /// The name the theme has in the config, before this edit.
    pub original: String,
    pub name: TextEdit,
    pub author: TextEdit,
    pub url: TextEdit,
    pub scheme: Scheme,
    /// The color the editor is changing.
    pub slot: Slot,
    /// The color editor is open on `slot`. A built-in theme never opens it.
    pub editor_open: bool,
    /// The `#RRGGBB` the field is showing. A half-typed value stays until a
    /// slider or another color replaces it.
    pub hex: TextEdit,
}

impl ThemeDraft {
    /// The draft for the theme the list has selected.
    pub fn from_selected(model: &SettingsModel) -> Self {
        match entries(&model.config)
            .get(model.selected)
            .and_then(|entry| {
                entry.name_for(model.theme_mode.appearance(crate::settings::system_dark()))
            }) {
            Some(name) => Self::from_name(name, &model.config),
            None => Self::empty(),
        }
    }

    pub fn from_name(name: &str, config: &ConfigFile) -> Self {
        let custom = config.custom_themes.get(name).cloned();
        let scheme = match &custom {
            Some(scheme) => scheme.clone(),
            None => Scheme::from_palette(palette(config, name).as_ref()),
        };
        let hex = color_picker::hex6(scheme.color(Slot::Background));
        Self {
            saved: custom,
            original: name.to_owned(),
            name: TextEdit::new(name),
            author: TextEdit::new(scheme.author.clone()),
            url: TextEdit::new(scheme.url.clone()),
            scheme,
            slot: Slot::Background,
            editor_open: false,
            hex: TextEdit::new(hex),
        }
    }

    fn empty() -> Self {
        Self {
            saved: None,
            original: String::new(),
            name: TextEdit::new(""),
            author: TextEdit::new(""),
            url: TextEdit::new(""),
            scheme: Scheme::default(),
            slot: Slot::Background,
            editor_open: false,
            hex: TextEdit::new(color_picker::hex6(0)),
        }
    }

    /// Shows `slot` in the field, replacing whatever was being typed.
    pub fn select_slot(&mut self, slot: Slot) {
        if self.slot == slot {
            return;
        }
        self.slot = slot;
        self.show_hex(shown_rgb(&self.scheme, slot));
    }

    pub fn set_slot_color(&mut self, rgb: u32) {
        if let Slot::Indexed(index) = self.slot {
            if theme::hex(rgb) == theme::xterm_index(index) {
                self.scheme.indexed.remove(&index);
                self.show_hex(rgb);
                return;
            }
        }
        self.scheme.set_color(self.slot, rgb);
        self.show_hex(rgb);
    }

    fn show_hex(&mut self, rgb: u32) {
        let hex = color_picker::hex6(rgb);
        if self.hex.text != hex {
            self.hex.replace_text(hex);
        }
    }

    /// Whether the theme can be written: only the user's own can.
    pub fn is_custom(&self) -> bool {
        self.saved.is_some()
    }

    /// Whether the draft differs from the theme it was loaded from.
    pub fn is_dirty(&self) -> bool {
        let Some(saved) = &self.saved else {
            return false;
        };
        self.name.text.trim() != self.original || &self.edited() != saved
    }

    /// Puts the fields back as they were loaded.
    pub fn revert(&mut self, config: &ConfigFile) {
        let original = self.original.clone();
        *self = Self::from_name(&original, config);
    }

    /// The palette the draft describes. A built-in theme keeps the palette it
    /// was drawn with; a custom one is the scheme.
    pub fn palette(&self, config: &ConfigFile) -> Arc<Palette> {
        match &self.saved {
            Some(_) => self.edited().to_palette(&self.original),
            None => theme::resolve_builtin(&self.original)
                .unwrap_or_else(|| palette(config, &self.original)),
        }
    }

    fn edited(&self) -> Scheme {
        let mut scheme = self.scheme.clone();
        scheme.author = self.author.text.trim().to_owned();
        scheme.url = self.url.text.trim().to_owned();
        scheme
    }
}

/// Every theme, by name. A file that shares a built-in name is listed once.
pub fn names(config: &ConfigFile) -> Vec<String> {
    let mut names: Vec<String> = theme::builtins()
        .iter()
        .filter(|palette| !config.custom_themes.contains_key(&palette.name))
        .map(|palette| palette.name.clone())
        .collect();
    names.extend(config.custom_themes.keys().cloned());
    names.sort_by_key(|name| name.to_lowercase());
    names
}

/// Which of a paired theme's variants the editor applies.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ThemeMode {
    #[default]
    Adaptive,
    Dark,
    Light,
}

impl ThemeMode {
    /// The variant to edit and show while the desktop choice is `dark`.
    pub fn appearance(self, system_dark: bool) -> Appearance {
        match self {
            Self::Light => Appearance::Light,
            Self::Adaptive if !system_dark => Appearance::Light,
            Self::Adaptive | Self::Dark => Appearance::Dark,
        }
    }
}

/// One row in the themes list. A dark and a light theme with the same base
/// name share the row so both variants can be edited in one place.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThemeEntry {
    pub name: String,
    pub dark: Option<String>,
    pub light: Option<String>,
}

impl ThemeEntry {
    pub fn name_for(&self, variant: Appearance) -> Option<&str> {
        match variant {
            Appearance::Dark => self.dark.as_deref().or(self.light.as_deref()),
            Appearance::Light => self.light.as_deref().or(self.dark.as_deref()),
        }
    }

    pub fn is_pair(&self) -> bool {
        self.dark.is_some() && self.light.is_some()
    }

    pub fn label(&self) -> &'static str {
        match (self.dark.is_some(), self.light.is_some()) {
            (true, true) => "Adaptive",
            (true, false) => "Dark",
            (false, true) => "Light",
            (false, false) => "Dark",
        }
    }
}

fn base_name(name: &str) -> String {
    let lower = name.to_ascii_lowercase();
    let suffix = if lower.ends_with(" dark") {
        5
    } else if lower.ends_with(" light") {
        6
    } else {
        return name.to_owned();
    };
    name[..name.len() - suffix].trim_end().to_owned()
}

pub fn appearance(config: &ConfigFile, name: &str) -> Appearance {
    match config.custom_themes.get(name) {
        Some(scheme) => scheme.appearance,
        None if name.to_ascii_lowercase().contains("light") => Appearance::Light,
        None => Appearance::Dark,
    }
}

/// The themes a built-in or custom name belongs to, grouped into list rows.
/// A configured adaptive pair also groups two names that share no base.
pub fn entries(config: &ConfigFile) -> Vec<ThemeEntry> {
    let pair = config
        .settings
        .adaptive_theme
        .as_ref()
        .filter(|pair| pair.ready());
    let mut groups: BTreeMap<String, ThemeEntry> = BTreeMap::new();
    for name in names(config) {
        let in_pair = pair.is_some_and(|pair| pair.dark == name || pair.light == name);
        let (key, shown) = match pair.filter(|_| in_pair) {
            Some(pair) => (
                format!(
                    "adaptive:{}|{}",
                    pair.dark.to_lowercase(),
                    pair.light.to_lowercase()
                ),
                base_name(&pair.dark),
            ),
            None => (base_name(&name).to_lowercase(), base_name(&name)),
        };
        let entry = groups.entry(key).or_insert_with(|| ThemeEntry {
            name: shown,
            dark: None,
            light: None,
        });
        match appearance(config, &name) {
            Appearance::Dark => entry.dark = Some(name),
            Appearance::Light => entry.light = Some(name),
        }
    }
    let mut entries: Vec<_> = groups.into_values().collect();
    entries.sort_by_key(|entry| entry.name.to_lowercase());
    entries
}

pub fn initial_mode(entry: &ThemeEntry) -> ThemeMode {
    match (entry.dark.is_some(), entry.light.is_some()) {
        (false, true) => ThemeMode::Light,
        (true, true) => ThemeMode::Adaptive,
        _ => ThemeMode::Dark,
    }
}

/// The palette a theme name resolves to.
pub fn palette(config: &ConfigFile, name: &str) -> Arc<Palette> {
    match config.custom_themes.get(name) {
        Some(scheme) => scheme.to_palette(name),
        None => theme::resolve_builtin(name).unwrap_or_else(theme::default_palette),
    }
}

/// Whether the theme is one the user made.
pub fn is_custom(config: &ConfigFile, name: &str) -> bool {
    config.custom_themes.contains_key(name)
}

/// The row a theme has in the list.
pub fn index_of(config: &ConfigFile, name: &str) -> usize {
    entries(config)
        .iter()
        .position(|entry| {
            entry.dark.as_deref() == Some(name) || entry.light.as_deref() == Some(name)
        })
        .unwrap_or(0)
}

/// Shows the theme the list has selected in the terminal and in this window.
pub fn preview_selected(model: &mut SettingsModel) {
    let palette = model.theme.palette(&model.config);
    super::preview(model, palette);
}

pub fn editor(kit: &Kit, model: &SettingsModel, overlays: &mut Vec<El>) -> El {
    let body = col()
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .scroll(ScrollSpec {
            stick_bottom: false,
        })
        .child(
            col()
                .w_full()
                .px(20.0)
                .py(8.0)
                .gap(16.0)
                .child(identity(kit, model))
                .child(basic_ui(kit, model, overlays))
                .child(ansi_set(kit, model, overlays))
                .child(cube(kit, model, overlays))
                .child(gray(kit, model, overlays)),
        );
    super::with_actions(mode_selector(kit, model), actions(kit, model), body)
}

/// Adaptive or one of the two variants, on the left of the theme actions.
fn mode_selector(kit: &Kit, model: &SettingsModel) -> Option<El> {
    if !entries(&model.config).get(model.selected)?.is_pair() {
        return None;
    }
    let at = match model.theme_mode {
        ThemeMode::Adaptive => 0,
        ThemeMode::Dark => 1,
        ThemeMode::Light => 2,
    };
    Some(
        SegmentedControl::new(&["Adaptive", "Dark", "Light"], Some(at))
            .label("Theme mode")
            .fit()
            .on_pick(|m: &mut SettingsModel, at, _tx| {
                let mode = [ThemeMode::Adaptive, ThemeMode::Dark, ThemeMode::Light][at];
                if m.theme_mode == mode {
                    return;
                }
                if commit(m) {
                    super::write_config(m);
                }
                m.theme_mode = mode;
                m.theme = ThemeDraft::from_selected(m);
                preview_selected(m);
            })
            .build(kit),
    )
}

/// A string that cannot become `#RRGGBB`. A short hex prefix is still being typed.
pub(crate) fn hex_unusable(text: &str) -> bool {
    let text = text.trim();
    if text.is_empty() || color_picker::parse_hex(text).is_some() {
        return false;
    }
    let body = text
        .strip_prefix('#')
        .or_else(|| text.strip_prefix("0x"))
        .or_else(|| text.strip_prefix("0X"))
        .unwrap_or(text);
    body.len() > 6 || body.chars().any(|c| !c.is_ascii_hexdigit())
}

/// Writes a finished `#RRGGBB` into the color the field is editing.
pub(crate) fn apply_typed_color(model: &mut SettingsModel) {
    let Some(rgb) = color_picker::parse_hex(&model.theme.hex.text) else {
        return;
    };
    if shown_rgb(&model.theme.scheme, model.theme.slot) == rgb {
        return;
    }
    model.theme.set_slot_color(rgb);
    preview_selected(model);
}

/// The color a swatch draws. A cube or ramp cell with no override is the xterm color.
fn shown_rgb(scheme: &Scheme, slot: Slot) -> u32 {
    match slot {
        Slot::Indexed(index) => scheme
            .indexed
            .get(&index)
            .copied()
            .map(rgb_of)
            .unwrap_or_else(|| rgb_of(theme::xterm_index(index))),
        _ => scheme.color(slot),
    }
}

fn rgb_of(rgb: [u8; 3]) -> u32 {
    u32::from(rgb[0]) << 16 | u32::from(rgb[1]) << 8 | u32::from(rgb[2])
}

fn basic_ui(kit: &Kit, model: &SettingsModel, overlays: &mut Vec<El>) -> El {
    row()
        .w_full()
        .gap(16.0)
        .flex_wrap()
        .items_start()
        .child(
            colour_set(kit, model, "Basic", &BASIC, overlays)
                .grow(1.0)
                .min_w(swatches_width(BASIC.len())),
        )
        .child(
            colour_set(kit, model, "UI", &UI, overlays)
                .grow(1.0)
                .min_w(swatches_width(UI.len())),
        )
}

fn swatches_width(count: usize) -> f32 {
    let count = count as f32;
    count * SWATCH + (count - 1.0) * SWATCH_GAP
}

fn ansi_set(kit: &Kit, model: &SettingsModel, overlays: &mut Vec<El>) -> El {
    let normal: [Slot; 8] = std::array::from_fn(|index| Slot::Ansi(index as u8));
    let bright: [Slot; 8] = std::array::from_fn(|index| Slot::Ansi(index as u8 + 8));
    col().w_full().gap(8.0).child(heading(kit, "ANSI")).child(
        col()
            .w_full()
            .gap(6.0)
            .child(swatch_row(kit, model, &normal, overlays))
            .child(swatch_row(kit, model, &bright, overlays)),
    )
}

fn cube(kit: &Kit, model: &SettingsModel, overlays: &mut Vec<El>) -> El {
    let mut line = row().w_full().gap(8.0).flex_wrap().items_start();
    for red in 0..6u8 {
        for green in 0..6u8 {
            let mut group = row().gap(CELL_GAP).w(GROUP).min_w(GROUP);
            for blue in 0..6u8 {
                let index = 16 + 36 * red + 6 * green + blue;
                group = group.child(swatch_button(
                    kit,
                    model,
                    Slot::Indexed(index),
                    CELL,
                    false,
                    overlays,
                ));
            }
            line = line.child(group);
        }
    }
    col()
        .w_full()
        .gap(8.0)
        .child(heading(kit, "Cube"))
        .child(line)
}

fn gray(kit: &Kit, model: &SettingsModel, overlays: &mut Vec<El>) -> El {
    let mut line = row().w_full().gap(CELL_GAP).flex_wrap().items_start();
    for index in 232..=255u8 {
        line = line.child(swatch_button(
            kit,
            model,
            Slot::Indexed(index),
            CELL,
            false,
            overlays,
        ));
    }
    col()
        .w_full()
        .gap(8.0)
        .child(heading(kit, "Gray"))
        .child(line)
}

fn colour_set(
    kit: &Kit,
    model: &SettingsModel,
    title: &str,
    slots: &[Slot],
    overlays: &mut Vec<El>,
) -> El {
    col()
        .gap(8.0)
        .min_w(0.0)
        .child(heading(kit, title))
        .child(swatch_row(kit, model, slots, overlays))
}

fn swatch_row(kit: &Kit, model: &SettingsModel, slots: &[Slot], overlays: &mut Vec<El>) -> El {
    let mut swatches = row().w_full().gap(SWATCH_GAP).flex_wrap().items_start();
    for slot in slots {
        swatches = swatches.child(swatch_button(kit, model, *slot, SWATCH, true, overlays));
    }
    swatches
}

fn swatch_button(
    kit: &Kit,
    model: &SettingsModel,
    slot: Slot,
    size: f32,
    focusable: bool,
    overlays: &mut Vec<El>,
) -> El {
    let open = model.theme.is_custom() && model.theme.editor_open && model.theme.slot == slot;
    let mut button = el()
        .key(format!("slot-{}", slot.label()))
        .role("option")
        .w(size)
        .h(size)
        .min_w(size)
        .radius(if focusable { kit.radius.sm } else { 2.0 })
        .fill(theme::linear(shown_rgb(&model.theme.scheme, slot)))
        .border(
            if focusable { 2.0 } else { 1.0 },
            if open {
                kit.c.text_accent
            } else {
                kit.c.border_muted
            },
        )
        .help(slot.label())
        .focusable(focusable);
    if focusable {
        button = button.focus_selected(open);
    }
    if model.theme.is_custom() {
        button = button.on_press(move |m: &mut SettingsModel| {
            // A press on the open swatch lands on the scrim, which closes it.
            // This runs only while the editor is shut.
            m.theme.select_slot(slot);
            m.theme.editor_open = true;
        });
    }
    if !open {
        return button;
    }
    Popover::new("theme-color-editor")
        .trigger(button)
        .title(slot.label())
        .open(true)
        .on_close(|m: &mut SettingsModel| m.theme.editor_open = false)
        .child(color_editor(kit, model))
        .build(kit, overlays)
}

fn color_editor(kit: &Kit, model: &SettingsModel) -> El {
    let color = shown_rgb(&model.theme.scheme, model.theme.slot);
    let style = color_picker::Style {
        track: kit.c.surface_raised,
        track_border: kit.c.border_muted,
        thumb: kit.c.white,
        thumb_border: kit.c.border_strong,
        label: kit.c.text_muted,
    };
    let mut editor = col()
        .w(240.0)
        .gap(8.0)
        .child(
            super::when_finished(
                TextInput::new(&model.theme.hex)
                    .placeholder("#RRGGBB")
                    .width(None)
                    .invalid(hex_unusable(&model.theme.hex.text))
                    .on_edit(|m: &mut SettingsModel| &mut m.theme.hex)
                    .on_change(|m: &mut SettingsModel, _tx| apply_typed_color(m)),
                reflect_color,
            )
            .build(kit),
        )
        .child(color_picker::rgb_sliders::<SettingsModel, _>(
            "theme-color",
            color,
            |m: &mut SettingsModel, rgb: u32| {
                m.theme.set_slot_color(rgb);
                preview_selected(m);
            },
            style,
        ));
    if let Slot::Indexed(index) = model.theme.slot {
        if model.theme.scheme.indexed.contains_key(&index) {
            editor = editor.child(indexed_remove(kit));
        }
    }
    editor
}

fn heading(kit: &Kit, title: &str) -> El {
    text(title)
        .font(rune::TextStyle {
            line_height: 16.0,
            ..kit.ty.ui_medium
        })
        .color(kit.c.text_base)
}

fn identity(kit: &Kit, model: &SettingsModel) -> El {
    let (name, author, source) = if model.theme.is_custom() {
        fields(kit, model)
    } else {
        let draft = &model.theme;
        (
            readout(kit, "Name", draft.original.clone()),
            readout(kit, "Author", draft.scheme.author.clone()),
            source_readout(kit, draft.scheme.url.clone()),
        )
    };
    let appearance = if model.theme.is_custom() {
        let at = match model.theme.scheme.appearance {
            Appearance::Dark => 0,
            Appearance::Light => 1,
        };
        Field::new()
            .label("Appearance")
            .control(
                SegmentedControl::new(&["Dark", "Light"], Some(at))
                    .label("Theme appearance")
                    .fit()
                    .on_pick(|m: &mut SettingsModel, at, _tx| {
                        reflect_appearance(
                            m,
                            if at == 0 {
                                Appearance::Dark
                            } else {
                                Appearance::Light
                            },
                        );
                    })
                    .build(kit),
            )
            .build(kit)
    } else {
        readout(
            kit,
            "Appearance",
            appearance(&model.config, &model.theme.original)
                .label()
                .to_owned(),
        )
    };
    row()
        .w_full()
        .gap(12.0)
        .flex_wrap()
        .items_start()
        .child(share(name))
        .child(share(author))
        .child(share(source))
        .child(share(appearance))
}

/// A row share that wraps onto its own line once `FIELD_MIN` no longer fits.
fn share(child: El) -> El {
    col().grow(1.0).min_w(FIELD_MIN).child(child)
}

fn readout(kit: &Kit, title: &str, value: String) -> El {
    col().gap(8.0).min_w(0.0).child(heading(kit, title)).child(
        text(value)
            .font(kit.ty.ui)
            .color(kit.c.text_base)
            .truncate(),
    )
}

fn source_readout(kit: &Kit, url: String) -> El {
    let mut line = row().w_full().gap(6.0).center_y().child(
        text(url.clone())
            .font(kit.ty.ui)
            .color(kit.c.text_base)
            .truncate()
            .grow(1.0)
            .min_w(0.0),
    );
    if !url.is_empty() {
        line = line.child(super::link(kit, url));
    }
    col()
        .gap(8.0)
        .min_w(0.0)
        .child(heading(kit, "Source"))
        .child(line)
}

/// The theme's fields, for a theme the user owns.
fn fields(kit: &Kit, model: &SettingsModel) -> (El, El, El) {
    let draft = &model.theme;
    (
        Field::new()
            .label("Name")
            .control(
                super::when_finished(
                    TextInput::new(&draft.name)
                        .placeholder("Theme name")
                        .width(None)
                        .on_edit(|m: &mut SettingsModel| &mut m.theme.name),
                    reflect_name,
                )
                .build(kit),
            )
            .build(kit),
        Field::new()
            .label("Author")
            .control(
                super::when_finished(
                    TextInput::new(&draft.author)
                        .placeholder("Who made it")
                        .width(None)
                        .on_edit(|m: &mut SettingsModel| &mut m.theme.author),
                    reflect_author,
                )
                .build(kit),
            )
            .build(kit),
        Field::new()
            .label("Source")
            .control(
                super::when_finished(
                    TextInput::new(&draft.url)
                        .placeholder("https://")
                        .width(None)
                        .on_edit(|m: &mut SettingsModel| &mut m.theme.url),
                    reflect_url,
                )
                .build(kit),
            )
            .build(kit),
    )
}

/// The actions the selected theme has.
pub fn actions(kit: &Kit, model: &SettingsModel) -> Vec<El> {
    let custom = model.theme.is_custom();
    let dirty = model.theme.is_dirty();
    vec![
        super::action(
            kit,
            "theme-save",
            &super::SAVE,
            "Save",
            custom && dirty,
            save_theme,
        ),
        super::action(
            kit,
            "theme-revert",
            &super::REVERT,
            "Revert",
            dirty,
            revert_theme,
        ),
        super::action(
            kit,
            "theme-add-counterpart",
            &super::ADD_THEME,
            if model.theme.scheme.appearance == Appearance::Dark {
                "Add Light counterpart"
            } else {
                "Add Dark counterpart"
            },
            !entries(&model.config)
                .get(model.selected)
                .is_some_and(ThemeEntry::is_pair),
            add_counterpart,
        ),
        super::action(
            kit,
            "theme-duplicate",
            kit.glyph.copy,
            "Duplicate",
            true,
            duplicate_theme,
        ),
        super::action(
            kit,
            "theme-delete",
            kit.glyph.close,
            "Delete",
            custom,
            delete_theme,
        ),
    ]
}

fn reflect_appearance(model: &mut SettingsModel, next: Appearance) {
    if model.theme.scheme.appearance == next {
        return;
    }
    model.theme.scheme.appearance = next;
    if !commit(model) {
        return;
    }
    let name = model.theme.original.clone();
    if let Some(mut pair) = model.config.settings.adaptive_theme.clone() {
        let other = if pair.dark == name {
            Some(pair.light.clone())
        } else if pair.light == name {
            Some(pair.dark.clone())
        } else {
            None
        };
        if let Some(other) = other {
            if appearance(&model.config, &other) == next {
                std::mem::swap(&mut pair.dark, &mut pair.light);
                model.config.settings.adaptive_theme = Some(pair);
            }
        }
    }
    if super::write_config(model) {
        super::push(model);
    }
    preview_selected(model);
}

/// Puts the name into the list. The rest of the draft stays, so a field that
/// is still focused keeps its caret.
pub(crate) fn reflect_name(model: &mut SettingsModel) {
    let name = model.theme.name.text.trim().to_owned();
    if name.is_empty() {
        model.status = Some("A theme needs a name".to_owned());
        return;
    }
    if !crate::settings::name_ok(&name) {
        model.status = Some(format!("'{name}' cannot be a file name"));
        return;
    }
    if name == model.theme.original {
        show(&mut model.theme.name, &name);
        model.status = None;
        return;
    }
    let original = model.theme.original.clone();
    if !model.config.rename_theme(&original, &name) {
        model.status = Some(format!("A theme named '{name}' already exists"));
        return;
    }
    model.theme.original = name.clone();
    show(&mut model.theme.name, &name);
    model.profile.retarget_theme(&original, &name);
    model.selected = index_of(&model.config, &name);
    model.status = None;
    preview_selected(model);
    if super::write_config(model) {
        super::push(model);
    }
}

fn reflect_author(model: &mut SettingsModel) {
    reflect_field(model, |draft, theme| {
        let author = draft.author.text.trim().to_owned();
        show(&mut draft.author, &author);
        let changed = theme.author != author;
        theme.author = author;
        Ok(changed)
    });
}

fn reflect_url(model: &mut SettingsModel) {
    reflect_field(model, |draft, theme| {
        let url = draft.url.text.trim().to_owned();
        show(&mut draft.url, &url);
        let changed = theme.url != url;
        theme.url = url;
        Ok(changed)
    });
}

fn indexed_remove(kit: &Kit) -> El {
    row()
        .key("indexed-remove")
        .w_full()
        .h(28.0)
        .px(6.0)
        .center_y()
        .radius(kit.radius.sm)
        .hover(kit.c.hover)
        .focusable(true)
        .on_press(remove_indexed)
        .child(text("Remove").font(kit.ty.ui).color(kit.c.text_base))
}

pub(crate) fn remove_indexed(model: &mut SettingsModel) {
    let Slot::Indexed(index) = model.theme.slot else {
        return;
    };
    if !model.theme.is_custom() {
        return;
    }
    model.theme.scheme.indexed.remove(&index);
    model.theme.editor_open = false;
    model.theme.select_slot(Slot::Background);
    reflect_field(model, |_draft, theme| {
        Ok(theme.indexed.remove(&index).is_some())
    });
}

pub(crate) fn reflect_color(model: &mut SettingsModel) {
    let Some(rgb) = color_picker::parse_hex(&model.theme.hex.text) else {
        model.status = Some("Enter a color as #RRGGBB".to_owned());
        return;
    };
    let slot = model.theme.slot;
    model.theme.set_slot_color(rgb);
    if let Slot::Indexed(index) = slot {
        if theme::hex(rgb) == theme::xterm_index(index) {
            reflect_field(model, |_draft, theme| {
                Ok(theme.indexed.remove(&index).is_some())
            });
            return;
        }
    }
    reflect_field(model, |draft, theme| {
        let changed = theme.color(draft.slot) != rgb;
        theme.set_color(draft.slot, rgb);
        Ok(changed)
    });
}

/// Writes one finished field. The rest of the draft stays, so a field that is
/// still focused keeps its caret.
fn reflect_field(
    model: &mut SettingsModel,
    edit: impl FnOnce(&mut ThemeDraft, &mut Scheme) -> Result<bool, String>,
) {
    let Some(mut theme) = model
        .config
        .custom_themes
        .get(&model.theme.original)
        .cloned()
    else {
        return;
    };
    match edit(&mut model.theme, &mut theme) {
        Err(err) => model.status = Some(err),
        Ok(false) => model.status = None,
        Ok(true) => {
            if let Some(saved) = &mut model.theme.saved {
                *saved = theme.clone();
            }
            let name = model.theme.original.clone();
            model.config.set_custom_theme(&name, theme);
            model.status = None;
            preview_selected(model);
            if super::write_config(model) {
                super::push(model);
            }
        }
    }
}

fn show(edit: &mut TextEdit, text: &str) {
    if edit.text != text {
        edit.replace_text(text);
    }
}

/// Writes the draft into the config, renaming the theme when its name changed.
pub(crate) fn commit(model: &mut SettingsModel) -> bool {
    let original = model.theme.original.clone();
    if !model.config.is_custom_theme(&original) {
        return false;
    }
    let name = model.theme.name.text.trim().to_owned();
    if name.is_empty() {
        model.status = Some("A theme needs a name".to_owned());
        return false;
    }
    if !crate::settings::name_ok(&name) {
        model.status = Some(format!("'{name}' cannot be a file name"));
        return false;
    }
    if name != original && !model.config.rename_theme(&original, &name) {
        model.status = Some(format!("A theme named '{name}' already exists"));
        return false;
    }
    model.profile.retarget_theme(&original, &name);
    let theme = model.theme.edited();
    let changed = name != original || model.config.custom_themes.get(&name) != Some(&theme);
    model.config.set_custom_theme(&name, theme.clone());
    model.theme.scheme = theme.clone();
    model.theme.saved = Some(theme);
    model.theme.original = name.clone();
    model.theme.name = TextEdit::new(name);
    changed
}

/// Makes the theme the one the terminal uses.
pub fn apply_theme(model: &mut SettingsModel) {
    let name = model.theme.original.clone();
    if name.is_empty() {
        return;
    }
    let entry = entries(&model.config).get(model.selected).cloned();
    if let (ThemeMode::Adaptive, Some((Some(dark), Some(light)))) = (
        model.theme_mode,
        entry.map(|entry| (entry.dark, entry.light)),
    ) {
        model.config.set_adaptive_theme(
            dark,
            light,
            model.theme_mode.appearance(crate::settings::system_dark()),
        );
    } else {
        model.config.set_theme(name);
        model.config.settings.adaptive_theme = None;
    }
    model.previewed = Some(model.theme.palette(&model.config));
    if super::write_config(model) {
        super::push(model);
    }
}

/// Writes the draft into the config and hands it to the terminal.
pub fn save_theme(model: &mut SettingsModel) {
    if !commit(model) {
        return;
    }
    select_by_name(model, &model.theme.original.clone());
    if super::write_config(model) {
        super::push(model);
    }
}

/// Puts the draft back as it was loaded, and the terminal with it.
pub fn revert_theme(model: &mut SettingsModel) {
    let config = model.config.clone();
    model.theme.revert(&config);
    model.status = None;
    super::restore(model);
}

/// Adds the other appearance and makes the two an adaptive pair.
pub fn add_counterpart(model: &mut SettingsModel) {
    let name = model.theme.original.clone();
    if name.is_empty() || model.config.add_counterpart(&name).is_none() {
        return;
    }
    select_by_name(model, &name);
    model.theme_mode = ThemeMode::Adaptive;
    model.theme = ThemeDraft::from_selected(model);
    if super::write_config(model) {
        super::push(model);
    }
    preview_selected(model);
}

/// Copies the theme into a custom one and selects it.
pub fn duplicate_theme(model: &mut SettingsModel) {
    let name = model.theme.original.clone();
    if name.is_empty() {
        return;
    }
    let copy = model.config.copy_theme(&name);
    select_by_name(model, &copy);
    model.theme_mode = entries(&model.config)
        .get(model.selected)
        .map_or(ThemeMode::Dark, initial_mode);
    model.theme = ThemeDraft::from_selected(model);
    if super::write_config(model) {
        super::push(model);
    }
    preview_selected(model);
}

/// Removes the custom theme.
pub fn delete_theme(model: &mut SettingsModel) {
    let name = model.theme.original.clone();
    if name.is_empty() {
        return;
    }
    if !model.config.remove_theme(&name) {
        model.status = Some("A built-in theme cannot be removed".to_owned());
        return;
    }
    model.selected = model
        .selected
        .min(entries(&model.config).len().saturating_sub(1));
    model.theme_mode = entries(&model.config)
        .get(model.selected)
        .map_or(ThemeMode::Dark, initial_mode);
    model.theme = ThemeDraft::from_selected(model);
    if super::write_config(model) {
        super::push(model);
    }
    preview_selected(model);
}

/// Adds a custom theme with the colors the terminal is using.
pub fn create_theme(model: &mut SettingsModel) {
    let palette = model.config.resolve_palette();
    let name = model.config.unique_theme_name("New Theme");
    let theme = Scheme::from_palette(&palette);
    model.config.set_custom_theme(&name, theme);
    select_by_name(model, &name);
    model.theme_mode = entries(&model.config)
        .get(model.selected)
        .map_or(ThemeMode::Dark, initial_mode);
    model.theme = ThemeDraft::from_selected(model);
    if super::write_config(model) {
        super::push(model);
    }
    preview_selected(model);
}

/// The list is sorted, so a name moves the row it sits on.
fn select_by_name(model: &mut SettingsModel, name: &str) {
    if let Some(index) = entries(&model.config).iter().position(|entry| {
        entry.dark.as_deref() == Some(name) || entry.light.as_deref() == Some(name)
    }) {
        model.selected = index;
    }
    model.theme = ThemeDraft::from_selected(model);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_named_like_a_builtin_is_listed_once() {
        let mut config = ConfigFile::default();
        config.set_custom_theme("Dracula", Scheme::default());
        let listed: Vec<_> = names(&config)
            .into_iter()
            .filter(|name| name == "Dracula")
            .collect();
        assert_eq!(listed, vec!["Dracula".to_owned()]);
        assert!(is_custom(&config, "Dracula"));
    }

    #[test]
    fn the_appearance_of_a_custom_theme_can_be_flipped() {
        let dir = std::env::temp_dir().join("rune-term-appearance-test");
        let _ = std::fs::remove_dir_all(&dir);
        let mut model = SettingsModel::with_config(ConfigFile::default().at_dir(dir), None);
        let copy = model.config.copy_theme("Dracula");
        select_by_name(&mut model, &copy);
        reflect_appearance(&mut model, Appearance::Light);
        assert_eq!(
            model.config.custom_themes.get(&copy).map(|s| s.appearance),
            Some(Appearance::Light)
        );
        assert_eq!(model.theme.scheme.appearance, Appearance::Light);
    }

    #[test]
    fn dark_and_light_variants_share_one_entry() {
        let rows = entries(&ConfigFile::default());
        let flexoki = rows.iter().find(|row| row.name == "Flexoki").expect("pair");
        assert_eq!(flexoki.dark.as_deref(), Some("Flexoki Dark"));
        assert_eq!(flexoki.light.as_deref(), Some("Flexoki Light"));
        assert!(flexoki.is_pair());
        assert_eq!(flexoki.label(), "Adaptive");
        assert!(rows.iter().all(|row| row.name != "Flexoki Light"));
        let dracula = rows
            .iter()
            .find(|row| row.name == "Dracula")
            .expect("Dracula");
        assert_eq!(dracula.label(), "Dark");
    }
}
