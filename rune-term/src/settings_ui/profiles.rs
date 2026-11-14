//! The profiles tab: the editor for the selected profile and the operations
//! on it. The editor works on a copy, so a half-typed name never reaches the
//! config.

use rune::{col, row, text, El, ScrollSpec, TextEdit, Tx};
use rune_kit::{
    Button, ButtonVariant, ControlSize, Field, Kit, Select, SelectAppearance, TextInput,
};

use super::SettingsModel;
use crate::settings::{self, Profile, ShellSpec};

/// The profile being edited.
pub struct ProfileDraft {
    /// The name the profile has in the config, before this edit.
    pub original: String,
    pub name: TextEdit,
    pub font_family: TextEdit,
    pub font_size: TextEdit,
    /// The theme's name; empty means the global theme.
    pub theme: String,
    /// Whether the theme list is open.
    pub theme_open: bool,
    pub cwd: TextEdit,
    pub shell_program: TextEdit,
    pub shell_args: TextEdit,
}

impl ProfileDraft {
    /// The draft for the profile the list has selected.
    pub fn from_selected(model: &SettingsModel) -> Self {
        let name = model
            .config
            .profiles
            .keys()
            .nth(model.selected)
            .cloned()
            .unwrap_or_else(|| settings::DEFAULT_PROFILE.to_owned());
        let profile = model
            .config
            .profile(&name)
            .cloned()
            .unwrap_or_else(Profile::default);
        Self::from_profile(&name, &profile)
    }

    pub fn from_profile(name: &str, profile: &Profile) -> Self {
        let (shell_program, shell_args) = profile
            .shell
            .as_ref()
            .map(|shell| {
                (
                    shell.program.clone().unwrap_or_default(),
                    shell.args.join(" "),
                )
            })
            .unwrap_or_default();
        Self {
            original: name.to_owned(),
            name: TextEdit::new(name),
            font_family: TextEdit::new(profile.font_family.clone()),
            font_size: TextEdit::new(profile.font_size.map(format_size).unwrap_or_default()),
            theme: profile.theme.clone().unwrap_or_default(),
            theme_open: false,
            cwd: TextEdit::new(profile.cwd.clone().unwrap_or_default()),
            shell_program: TextEdit::new(shell_program),
            shell_args: TextEdit::new(shell_args),
        }
    }

    /// The profile the draft describes, or why it cannot make one.
    pub fn to_profile(&self) -> Result<(String, Profile), String> {
        let name = self.name.text.trim().to_owned();
        if name.is_empty() {
            return Err("A profile needs a name".to_owned());
        }
        let font_family = self.font_family.text.trim().to_owned();
        if font_family.is_empty() {
            return Err("A profile needs a font family".to_owned());
        }
        let font_size = match self.font_size.text.trim() {
            "" => None,
            text => Some(settings::clamp_font_size(
                text.parse::<f32>()
                    .map_err(|_| format!("'{text}' is not a font size"))?,
            )),
        };
        let shell_program = non_empty(&self.shell_program.text);
        let shell_args: Vec<String> = self
            .shell_args
            .text
            .split_whitespace()
            .map(str::to_owned)
            .collect();
        let shell = if shell_program.is_some() || !shell_args.is_empty() {
            Some(ShellSpec {
                program: shell_program,
                args: shell_args,
            })
        } else {
            None
        };
        Ok((
            name,
            Profile {
                font_family,
                font_size,
                theme: non_empty(&self.theme),
                backend: "local".to_owned(),
                shell,
                cwd: non_empty(&self.cwd.text),
            },
        ))
    }
}

fn non_empty(text: &str) -> Option<String> {
    let text = text.trim();
    if text.is_empty() {
        None
    } else {
        Some(text.to_owned())
    }
}

fn format_size(size: f32) -> String {
    if size.fract().abs() < 0.001 {
        format!("{size:.0}")
    } else {
        format!("{size:.2}")
    }
}

pub fn editor(kit: &Kit, model: &SettingsModel, overlays: &mut Vec<El>) -> El {
    let draft = &model.profile;
    col()
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
        .child(
            text(draft.original.clone())
                .font(kit.ty.ui_medium)
                .color(kit.c.text_base),
        )
        .child(actions(kit, model))
        .child(
            row()
                .w_full()
                .gap(12.0)
                .child(
                    Field::new()
                        .label("Name")
                        .control(
                            TextInput::new(&draft.name)
                                .placeholder("Profile name")
                                .width(None)
                                .on_edit(|m: &mut SettingsModel| &mut m.profile.name)
                                .build(kit),
                        )
                        .build(kit)
                        .grow(1.0)
                        .min_w(0.0),
                )
                .child(
                    Field::new()
                        .label("Theme")
                        .control(theme_select(kit, model, overlays))
                        .build(kit)
                        .grow(1.0)
                        .min_w(0.0),
                ),
        )
        .child(
            row()
                .w_full()
                .gap(12.0)
                .child(
                    Field::new()
                        .label("Font family")
                        .control(
                            TextInput::new(&draft.font_family)
                                .placeholder("Cascadia Code NF")
                                .width(None)
                                .on_edit(|m: &mut SettingsModel| &mut m.profile.font_family)
                                .build(kit),
                        )
                        .build(kit)
                        .grow(1.0)
                        .min_w(0.0),
                )
                .child(
                    Field::new()
                        .label("Font size")
                        .control(
                            TextInput::new(&draft.font_size)
                                .placeholder("14")
                                .width(None)
                                .on_edit(|m: &mut SettingsModel| &mut m.profile.font_size)
                                .build(kit),
                        )
                        .build(kit)
                        .grow(1.0)
                        .min_w(0.0),
                ),
        )
        .child(
            row()
                .w_full()
                .gap(12.0)
                .child(
                    Field::new()
                        .label("Shell program")
                        .control(
                            TextInput::new(&draft.shell_program)
                                .placeholder("Default shell")
                                .width(None)
                                .on_edit(|m: &mut SettingsModel| &mut m.profile.shell_program)
                                .build(kit),
                        )
                        .build(kit)
                        .grow(1.0)
                        .min_w(0.0),
                )
                .child(
                    Field::new()
                        .label("Shell arguments")
                        .control(
                            TextInput::new(&draft.shell_args)
                                .placeholder("-l")
                                .width(None)
                                .on_edit(|m: &mut SettingsModel| &mut m.profile.shell_args)
                                .build(kit),
                        )
                        .build(kit)
                        .grow(1.0)
                        .min_w(0.0),
                ),
        )
        .child(
            Field::new()
                .label("Working directory")
                .control(
                    TextInput::new(&draft.cwd)
                        .placeholder("Use current directory")
                        .width(None)
                        .on_edit(|m: &mut SettingsModel| &mut m.profile.cwd)
                        .build(kit),
                )
                .build(kit),
        )
}

/// The theme the profile uses, chosen from the built-in and the custom ones.
/// An empty theme inherits the global one.
fn theme_select(kit: &Kit, model: &SettingsModel, overlays: &mut Vec<El>) -> El {
    let names = super::themes::names(&model.config);
    let current = match model.profile.theme.is_empty() {
        true => None,
        false => Some(model.profile.theme.as_str()),
    };
    Select::new("profile-theme", names.as_slice())
        .label("Theme")
        .appearance(SelectAppearance::Field)
        .placeholder(format!("Inherit ({})", model.config.settings.theme))
        .current(current)
        .width(280.0)
        .open(model.profile.theme_open)
        .on_toggle(|m: &mut SettingsModel| {
            m.profile.theme_open = !m.profile.theme_open;
        })
        .on_close(|m: &mut SettingsModel| {
            m.profile.theme_open = false;
        })
        .on_choose(|m: &mut SettingsModel, value: &str, _tx| {
            m.profile.theme = value.to_owned();
            m.profile.theme_open = false;
            super::preview(m, value);
        })
        .build(kit, overlays)
}

fn actions(kit: &Kit, model: &SettingsModel) -> El {
    let is_default = model.config.default_profile_name() == model.profile.original;
    let dirty = is_dirty(model);
    row()
        .w_full()
        .gap(8.0)
        .flex_wrap()
        .child(
            Button::new("Save")
                .variant(ButtonVariant::Contrast)
                .control_size(ControlSize::Small)
                .on_press_tx(save)
                .build(kit)
                .key("profile-save"),
        )
        .child(
            Button::new("Revert")
                .variant(ButtonVariant::Neutral)
                .control_size(ControlSize::Small)
                .disabled(!dirty)
                .on_press(revert)
                .build(kit)
                .key("profile-revert"),
        )
        .child(
            Button::new("Set as default")
                .variant(ButtonVariant::Neutral)
                .control_size(ControlSize::Small)
                .disabled(is_default)
                .on_press(set_default)
                .build(kit)
                .key("profile-default"),
        )
        .child(
            Button::new("Duplicate")
                .variant(ButtonVariant::Ghost)
                .control_size(ControlSize::Small)
                .on_press(duplicate)
                .build(kit)
                .key("profile-duplicate"),
        )
        .child(
            Button::new("Delete")
                .variant(ButtonVariant::Ghost)
                .control_size(ControlSize::Small)
                .on_press(delete)
                .build(kit)
                .key("profile-delete"),
        )
        .child(
            Button::new("New profile")
                .variant(ButtonVariant::Ghost)
                .control_size(ControlSize::Small)
                .on_press(create)
                .build(kit)
                .key("profile-new"),
        )
}

/// Writes the draft into the config and closes the window on success.
pub fn save(model: &mut SettingsModel, tx: &mut Tx) {
    let (name, profile) = match model.profile.to_profile() {
        Ok(profile) => profile,
        Err(err) => {
            model.status = Some(err);
            return;
        }
    };
    let original = model.profile.original.clone();
    if name != original && !model.config.rename_profile(&original, &name) {
        model.status = Some(format!("A profile named '{name}' already exists"));
        return;
    }
    model.config.profiles.insert(name.clone(), profile);
    model.profile.original = name.clone();
    select_by_name(model, &name);
    if super::save_and_apply(model) {
        tx.close_window();
    }
}

/// Puts the draft back as the config holds it, and the terminal with it.
pub fn revert(model: &mut SettingsModel) {
    model.profile = ProfileDraft::from_selected(model);
    model.status = None;
    super::restore(model);
}

/// Whether the draft differs from the profile the config holds.
pub(crate) fn is_dirty(model: &SettingsModel) -> bool {
    let Some(saved) = model.config.profile(&model.profile.original) else {
        return true;
    };
    match model.profile.to_profile() {
        Ok((name, profile)) => name != model.profile.original || profile != *saved,
        Err(_) => true,
    }
}

/// Copies the selected profile under a free name.
pub fn duplicate(model: &mut SettingsModel) {
    let Some(name) = selected_name(model) else {
        return;
    };
    let Some(profile) = model.config.profile(&name).cloned() else {
        return;
    };
    let copy = model.config.unique_profile_name(&format!("{name} Copy"));
    model.config.profiles.insert(copy.clone(), profile);
    select_by_name(model, &copy);
    super::save_and_apply(model);
}

/// Removes the selected profile.
pub fn delete(model: &mut SettingsModel) {
    let Some(name) = selected_name(model) else {
        return;
    };
    if !model.config.remove_profile(&name) {
        model.status = Some("The last profile stays".to_owned());
        return;
    }
    model.selected = model
        .selected
        .min(model.config.profiles.len().saturating_sub(1));
    model.profile = ProfileDraft::from_selected(model);
    super::save_and_apply(model);
}

/// Adds a profile with the default settings.
pub fn create(model: &mut SettingsModel) {
    let name = model.config.unique_profile_name("Profile");
    model
        .config
        .profiles
        .insert(name.clone(), Profile::default());
    select_by_name(model, &name);
    super::save_and_apply(model);
}

/// Makes the selected profile the one new sessions use.
pub fn set_default(model: &mut SettingsModel) {
    let Some(name) = selected_name(model) else {
        return;
    };
    if model.config.set_default_profile(&name) {
        super::save_and_apply(model);
    }
}

fn selected_name(model: &SettingsModel) -> Option<String> {
    model.config.profiles.keys().nth(model.selected).cloned()
}

/// The list is sorted, so a name moves the row it sits on.
fn select_by_name(model: &mut SettingsModel, name: &str) {
    if let Some(index) = model.config.profiles.keys().position(|key| key == name) {
        model.selected = index;
    }
    model.profile = ProfileDraft::from_selected(model);
}
