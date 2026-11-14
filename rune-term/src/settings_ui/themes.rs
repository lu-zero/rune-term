//! The themes tab: the editor for the selected theme and the operations on it.
//! A built-in theme is read-only; a custom one can be renamed, recolored and
//! removed.

use std::sync::Arc;

use rune::{col, el, row, text, Color, El, ScrollSpec, TextEdit};
use rune_kit::{Field, Kit, TextInput};

use super::SettingsModel;
use crate::settings::{ConfigFile, CustomTheme};
use crate::theme::{self, Palette};

/// A color a theme carries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Slot {
    Bg,
    Fg,
    FgBright,
    Muted,
    TabBg,
    ActiveBorder,
    Accent,
}

impl Slot {
    pub const ALL: [Slot; 7] = [
        Slot::Bg,
        Slot::Fg,
        Slot::FgBright,
        Slot::Muted,
        Slot::TabBg,
        Slot::ActiveBorder,
        Slot::Accent,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Slot::Bg => "Background",
            Slot::Fg => "Foreground",
            Slot::FgBright => "Bright foreground",
            Slot::Muted => "Muted",
            Slot::TabBg => "Tabs",
            Slot::ActiveBorder => "Border",
            Slot::Accent => "Accent",
        }
    }
}

/// The colors a theme carries.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Colors {
    pub bg: u32,
    pub fg: u32,
    pub fg_bright: u32,
    pub muted: u32,
    pub tab_bg: u32,
    pub active_border: u32,
    pub accent: u32,
}

impl Colors {
    pub fn from_theme(theme: &CustomTheme) -> Self {
        Self {
            bg: theme.bg,
            fg: theme.fg,
            fg_bright: theme.fg_bright,
            muted: theme.muted,
            tab_bg: theme.tab_bg,
            active_border: theme.active_border,
            accent: theme.accent,
        }
    }

    pub fn from_palette(palette: &Palette) -> Self {
        Self {
            bg: palette.bg,
            fg: palette.fg,
            fg_bright: palette.fg_bright,
            muted: palette.muted,
            tab_bg: palette.tab_bg,
            active_border: palette.active_border,
            accent: palette.accent,
        }
    }

    pub fn into_theme(self, mut theme: CustomTheme) -> CustomTheme {
        theme.bg = self.bg;
        theme.fg = self.fg;
        theme.fg_bright = self.fg_bright;
        theme.muted = self.muted;
        theme.tab_bg = self.tab_bg;
        theme.active_border = self.active_border;
        theme.accent = self.accent;
        theme
    }

    pub fn get(&self, slot: Slot) -> u32 {
        match slot {
            Slot::Bg => self.bg,
            Slot::Fg => self.fg,
            Slot::FgBright => self.fg_bright,
            Slot::Muted => self.muted,
            Slot::TabBg => self.tab_bg,
            Slot::ActiveBorder => self.active_border,
            Slot::Accent => self.accent,
        }
    }

    pub fn set(&mut self, slot: Slot, value: u32) {
        match slot {
            Slot::Bg => self.bg = value,
            Slot::Fg => self.fg = value,
            Slot::FgBright => self.fg_bright = value,
            Slot::Muted => self.muted = value,
            Slot::TabBg => self.tab_bg = value,
            Slot::ActiveBorder => self.active_border = value,
            Slot::Accent => self.accent = value,
        }
    }
}

/// The theme being edited.
pub struct ThemeDraft {
    /// The theme as it was loaded, for revert. `None` for a built-in theme,
    /// which cannot be changed.
    saved: Option<CustomTheme>,
    /// The name the theme has in the config, before this edit.
    pub original: String,
    pub name: TextEdit,
    pub author: TextEdit,
    pub url: TextEdit,
    pub colors: Colors,
    /// The color the editor is changing.
    pub slot: Slot,
}

impl ThemeDraft {
    /// The draft for the theme the list has selected.
    pub fn from_selected(model: &SettingsModel) -> Self {
        let names = names(&model.config);
        match names.get(model.selected) {
            Some(name) => Self::from_colors(name, &model.config),
            None => Self::empty(),
        }
    }

    pub fn from_colors(name: &str, config: &ConfigFile) -> Self {
        let custom = config.custom_themes.get(name).cloned();
        let colors = match &custom {
            Some(custom) => Colors::from_theme(custom),
            None => Colors::from_palette(&palette(config, name)),
        };
        Self {
            saved: custom,
            original: name.to_owned(),
            name: TextEdit::new(name),
            author: TextEdit::new(
                config
                    .custom_themes
                    .get(name)
                    .map(|t| t.author.clone())
                    .unwrap_or_default(),
            ),
            url: TextEdit::new(
                config
                    .custom_themes
                    .get(name)
                    .map(|t| t.url.clone())
                    .unwrap_or_default(),
            ),
            colors,
            slot: Slot::Bg,
        }
    }

    fn empty() -> Self {
        Self {
            saved: None,
            original: String::new(),
            name: TextEdit::new(""),
            author: TextEdit::new(""),
            url: TextEdit::new(""),
            colors: Colors::default(),
            slot: Slot::Bg,
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
        self.name.text.trim() != self.original
            || self.author.text.trim() != saved.author
            || self.url.text.trim() != saved.url
            || self.colors != Colors::from_theme(saved)
    }

    /// Puts the fields back as they were loaded.
    pub fn revert(&mut self, config: &ConfigFile) {
        let original = self.original.clone();
        match &self.saved {
            Some(saved) => {
                let mut draft = Self::from_colors(&original, config);
                draft.saved = Some(saved.clone());
                draft.name = TextEdit::new(original);
                *self = draft;
            }
            None => *self = Self::from_colors(&original, config),
        }
    }

    /// The palette the draft describes, with the ANSI colors it does not edit.
    pub fn palette(&self, config: &ConfigFile) -> Arc<Palette> {
        let base = palette(config, &self.original);
        // A built-in theme keeps its own credit; a custom one has the fields.
        let (author, url) = match &self.saved {
            Some(_) => (
                self.author.text.trim().to_owned(),
                self.url.text.trim().to_owned(),
            ),
            None => (base.author.clone(), base.url.clone()),
        };
        Arc::new(Palette {
            name: self.original.clone(),
            bg: self.colors.bg,
            fg: self.colors.fg,
            fg_bright: self.colors.fg_bright,
            muted: self.colors.muted,
            tab_bg: self.colors.tab_bg,
            active_border: self.colors.active_border,
            accent: self.colors.accent,
            ansi16: base.ansi16,
            author,
            url,
        })
    }
}

/// Every theme, by name. The built-in ones and the custom ones are one list.
pub fn names(config: &ConfigFile) -> Vec<String> {
    let mut names: Vec<String> = theme::builtins().iter().map(|p| p.name.clone()).collect();
    names.extend(config.custom_themes.keys().cloned());
    names.sort_by_key(|name| name.to_lowercase());
    names
}

/// The palette a theme name resolves to.
pub fn palette(config: &ConfigFile, name: &str) -> Arc<Palette> {
    match config.custom_themes.get(name) {
        Some(custom) => custom.to_arc_palette(name),
        None => theme::resolve_builtin(name).unwrap_or_else(theme::default_palette),
    }
}

/// Whether the theme is one the user made.
pub fn is_custom(config: &ConfigFile, name: &str) -> bool {
    config.custom_themes.contains_key(name)
}

/// The row a theme has in the list.
pub fn index_of(config: &ConfigFile, name: &str) -> usize {
    names(config).iter().position(|n| n == name).unwrap_or(0)
}

/// Shows the theme the list has selected in the terminal and in this window.
pub fn preview_selected(model: &mut SettingsModel) {
    let palette = model.theme.palette(&model.config);
    super::preview(model, palette);
}

pub fn editor(kit: &Kit, model: &SettingsModel) -> El {
    let draft = &model.theme;
    let custom = draft.is_custom();
    let mut body = col()
        .w_full()
        .h_full()
        .grow(1.0)
        .min_h(0.0)
        .px(20.0)
        .py(16.0)
        .gap(14.0)
        .scroll(ScrollSpec {
            stick_bottom: false,
        })
        .child(credit(kit, model))
        .child(color_list(kit, model));
    if custom {
        body = body
            .child(
                Field::new()
                    .label("Name")
                    .control(
                        TextInput::new(&draft.name)
                            .placeholder("Theme name")
                            .width(None)
                            .on_edit(|m: &mut SettingsModel| &mut m.theme.name)
                            .build(kit),
                    )
                    .build(kit),
            )
            .child(
                row()
                    .w_full()
                    .gap(12.0)
                    .child(
                        Field::new()
                            .label("Author")
                            .control(
                                TextInput::new(&draft.author)
                                    .placeholder("Who made it")
                                    .width(None)
                                    .on_edit(|m: &mut SettingsModel| &mut m.theme.author)
                                    .build(kit),
                            )
                            .build(kit)
                            .grow(1.0)
                            .min_w(0.0),
                    )
                    .child(
                        Field::new()
                            .label("Source")
                            .control(
                                TextInput::new(&draft.url)
                                    .placeholder("https://")
                                    .width(None)
                                    .on_edit(|m: &mut SettingsModel| &mut m.theme.url)
                                    .build(kit),
                            )
                            .build(kit)
                            .grow(1.0)
                            .min_w(0.0),
                    ),
            )
            .child(color_editor(kit, model));
    }
    body
}

/// Who made the theme, and where to read more about it. A built-in theme
/// cannot be changed, and says so.
fn credit(kit: &Kit, model: &SettingsModel) -> El {
    let palette = model.theme.palette(&model.config);
    let author = palette.author.clone();
    let url = palette.url.clone();
    row()
        .w_full()
        .gap(6.0)
        .center_y()
        .child(
            text(format!("Palette by {author}"))
                .font(kit.ty.small)
                .color(kit.c.text_muted),
        )
        .child(super::link(kit, url))
        .child(el().grow(1.0))
        .child(read_only(kit, model))
}

/// The lock on a built-in theme.
fn read_only(kit: &Kit, model: &SettingsModel) -> El {
    if model.theme.is_custom() {
        return el().w(0.0).h(0.0);
    }
    el().key("theme-read-only")
        .w(22.0)
        .h(22.0)
        .center()
        .help("A built-in theme is read-only. Duplicate it to change it.")
        .child(rune::icon(kit.glyph.lock, 16.0).color(kit.c.text_faint))
}

/// The theme's colors, each one a target for the editor.
fn color_list(kit: &Kit, model: &SettingsModel) -> El {
    let mut list = col().w_full().gap(2.0);
    for slot in Slot::ALL {
        let selected = model.theme.slot == slot;
        let color = model.theme.colors.get(slot);
        list = list.child(
            row()
                .key(format!("slot-{}", slot.label()))
                .role("option")
                .w_full()
                .h(28.0)
                .px(6.0)
                .gap(10.0)
                .center_y()
                .radius(kit.radius.sm)
                .fill(if selected {
                    kit.c.surface_raised
                } else {
                    Color::TRANSPARENT
                })
                .hover(kit.c.hover)
                .focusable(true)
                .focus_selected(selected)
                .on_press(move |m: &mut SettingsModel| {
                    m.theme.slot = slot;
                    preview_selected(m);
                })
                .child(swatch(kit, color))
                .child(
                    text(slot.label())
                        .font(kit.ty.ui)
                        .color(kit.c.text_base)
                        .grow(1.0)
                        .min_w(0.0),
                ),
        );
    }
    list
}

/// A colour, with an edge a colour that matches the surface can still show.
fn swatch(kit: &Kit, rgb: u32) -> El {
    el().w(20.0)
        .h(20.0)
        .radius(4.0)
        .fill(theme::linear(rgb))
        .border(1.0, kit.c.text_muted)
}

/// The editor for the color the list has selected.
fn color_editor(kit: &Kit, model: &SettingsModel) -> El {
    let slot = model.theme.slot;
    let color = model.theme.colors.get(slot);
    let hsv = color_picker::Hsv::from_rgb_u32(color);
    let style = color_picker::Style {
        track: kit.c.surface_raised,
        track_border: kit.c.border_muted,
        thumb: kit.c.white,
        thumb_border: kit.c.border_strong,
        label: kit.c.text_muted,
        text: kit.c.text_base,
    };
    col()
        .w_full()
        .gap(8.0)
        .child(
            text(slot.label())
                .font(kit.ty.small_medium)
                .color(kit.c.text_muted),
        )
        .child(color_picker::hsv_editor::<SettingsModel, _>(
            "theme-color",
            hsv,
            |m: &mut SettingsModel, hsv: color_picker::Hsv| {
                m.theme.colors.set(m.theme.slot, hsv.to_rgb_u32());
                preview_selected(m);
            },
            style,
        ))
}

/// The actions the selected theme has.
pub fn actions(kit: &Kit, model: &SettingsModel) -> El {
    let custom = model.theme.is_custom();
    let dirty = model.theme.is_dirty();
    row()
        .gap(4.0)
        .child(super::action(
            kit,
            "theme-save",
            &super::SAVE,
            "Save",
            custom && dirty,
            save_theme,
        ))
        .child(super::action(
            kit,
            "theme-revert",
            &super::REVERT,
            "Revert",
            dirty,
            revert_theme,
        ))
        .child(super::action(
            kit,
            "theme-duplicate",
            kit.glyph.copy,
            "Duplicate",
            true,
            duplicate_theme,
        ))
        .child(super::action(
            kit,
            "theme-delete",
            kit.glyph.close,
            "Delete",
            custom,
            delete_theme,
        ))
}

/// Writes the draft into the config, renaming the theme when its name changed.
pub(crate) fn commit(model: &mut SettingsModel) -> bool {
    let original = model.theme.original.clone();
    let Some(mut theme) = model.config.custom_themes.get(&original).cloned() else {
        return false;
    };
    let name = model.theme.name.text.trim().to_owned();
    if name.is_empty() {
        model.status = Some("A theme needs a name".to_owned());
        return false;
    }
    if name != original && !model.config.rename_theme(&original, &name) {
        model.status = Some(format!("A theme named '{name}' already exists"));
        return false;
    }
    theme.author = model.theme.author.text.trim().to_owned();
    theme.url = model.theme.url.text.trim().to_owned();
    let theme = model.theme.colors.into_theme(theme);
    let changed = name != original || model.config.custom_themes.get(&original) != Some(&theme);
    model.config.set_custom_theme(&name, theme);
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
    model.config.set_theme(name);
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

/// Copies the theme into a custom one and selects it.
pub fn duplicate_theme(model: &mut SettingsModel) {
    let name = model.theme.original.clone();
    if name.is_empty() {
        return;
    }
    let copy = model.config.copy_theme(&name);
    select_by_name(model, &copy);
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
        .min(names(&model.config).len().saturating_sub(1));
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
    let theme = CustomTheme::from_palette(&palette);
    model.config.set_custom_theme(&name, theme);
    select_by_name(model, &name);
    if super::write_config(model) {
        super::push(model);
    }
    preview_selected(model);
}

/// The list is sorted, so a name moves the row it sits on.
fn select_by_name(model: &mut SettingsModel, name: &str) {
    if let Some(index) = names(&model.config).iter().position(|key| key == name) {
        model.selected = index;
    }
    model.theme = ThemeDraft::from_selected(model);
}
