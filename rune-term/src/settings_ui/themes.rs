//! The themes tab: the editor for the selected theme and the operations on it.
//! A built-in theme is read-only. A theme from `themes/` can be renamed,
//! recolored and removed, and a file that shares a built-in name replaces it.

use std::sync::Arc;

use rune::{col, el, row, text, El, ScrollSpec, TextEdit};
use rune_kit::{Field, Kit, Popover, TextInput};

use super::SettingsModel;
use crate::scheme::{Scheme, Slot};
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
        let names = names(&model.config);
        match names.get(model.selected) {
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
    names(config).iter().position(|n| n == name).unwrap_or(0)
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
    super::with_actions(actions(kit, model), body)
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
    row()
        .w_full()
        .gap(12.0)
        .flex_wrap()
        .items_start()
        .child(share(name))
        .child(share(author))
        .child(share(source))
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
    let theme = Scheme::from_palette(&palette);
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
}
