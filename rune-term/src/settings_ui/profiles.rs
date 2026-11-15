//! The profiles tab: the editor for the selected profile and the operations
//! on it. A field reaches the config when Enter is pressed or it loses focus.
//! A half-typed value stays in the draft.

use rune::{col, row, Cx, El, ScrollSpec, TextEdit};
use rune_kit::{Field, Kit, Select, SelectAppearance, TextInput};

use super::SettingsModel;
use crate::settings::{self, Profile, ShellSpec};

/// The profile being edited.
pub struct ProfileDraft {
    /// The profile as it was loaded, for revert.
    saved: Profile,
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
            saved: profile.clone(),
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

    /// Whether the draft differs from the profile it was loaded from.
    pub fn is_dirty(&self) -> bool {
        match self.to_profile() {
            Ok((name, profile)) => name != self.original || profile != self.saved,
            Err(_) => true,
        }
    }

    /// The profile's theme followed a rename of that theme.
    pub(crate) fn retarget_theme(&mut self, from: &str, to: &str) {
        if self.theme == from {
            self.theme = to.to_owned();
        }
        if self.saved.theme.as_deref() == Some(from) {
            self.saved.theme = Some(to.to_owned());
        }
    }

    /// Puts the fields back as they were loaded.
    pub fn revert(&mut self) {
        let (original, saved) = (self.original.clone(), self.saved.clone());
        *self = Self::from_profile(&original, &saved);
    }

    /// The profile the draft describes, or why it cannot make one.
    pub fn to_profile(&self) -> Result<(String, Profile), String> {
        let name = self.name.text.trim().to_owned();
        if name.is_empty() {
            return Err("A profile needs a name".to_owned());
        }
        if !settings::name_ok(&name) {
            return Err(format!("'{name}' cannot be a file name"));
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

pub fn editor(
    kit: &Kit,
    model: &SettingsModel,
    overlays: &mut Vec<El>,
    cx: &Cx<SettingsModel>,
) -> El {
    let draft = &model.profile;
    let body = col()
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .px(20.0)
        .pt(8.0)
        .pb(16.0)
        .gap(14.0)
        .scroll(ScrollSpec {
            stick_bottom: false,
        })
        .child(
            row()
                .w_full()
                .gap(12.0)
                .child(
                    Field::new()
                        .label("Name")
                        .control(
                            super::when_finished(
                                TextInput::new(&draft.name)
                                    .placeholder("Profile name")
                                    .width(None)
                                    .on_edit(|m: &mut SettingsModel| &mut m.profile.name),
                                reflect_name,
                            )
                            .build(kit),
                        )
                        .build(kit)
                        .grow(1.0)
                        .min_w(0.0),
                )
                .child(
                    Field::new()
                        .label("Theme")
                        .control(theme_select(kit, model, overlays, cx))
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
                            super::when_finished(
                                TextInput::new(&draft.font_family)
                                    .placeholder("Cascadia Code NF")
                                    .width(None)
                                    .on_edit(|m: &mut SettingsModel| &mut m.profile.font_family),
                                reflect_font_family,
                            )
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
                            super::when_finished(
                                TextInput::new(&draft.font_size)
                                    .placeholder("14")
                                    .width(None)
                                    .on_edit(|m: &mut SettingsModel| &mut m.profile.font_size),
                                reflect_font_size,
                            )
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
                            super::when_finished(
                                TextInput::new(&draft.shell_program)
                                    .placeholder("Default shell")
                                    .width(None)
                                    .on_edit(|m: &mut SettingsModel| &mut m.profile.shell_program),
                                reflect_shell_program,
                            )
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
                            super::when_finished(
                                TextInput::new(&draft.shell_args)
                                    .placeholder("-l")
                                    .width(None)
                                    .on_edit(|m: &mut SettingsModel| &mut m.profile.shell_args),
                                reflect_shell_args,
                            )
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
                    super::when_finished(
                        TextInput::new(&draft.cwd)
                            .placeholder("Use current directory")
                            .width(None)
                            .on_edit(|m: &mut SettingsModel| &mut m.profile.cwd),
                        reflect_cwd,
                    )
                    .build(kit),
                )
                .build(kit),
        );
    super::with_actions(actions(kit, model), body)
}

/// The theme the profile uses, chosen from the built-in and the custom ones.
/// An empty theme inherits the global one.
fn theme_select(
    kit: &Kit,
    model: &SettingsModel,
    overlays: &mut Vec<El>,
    cx: &Cx<SettingsModel>,
) -> El {
    let names = super::themes::names(&model.config);
    let current = match model.profile.theme.is_empty() {
        true => None,
        false => Some(model.profile.theme.as_str()),
    };
    let inherit = if model.config.follows_appearance() {
        "Appearance".to_owned()
    } else {
        model.config.settings.theme.clone()
    };
    let mut select = Select::new("profile-theme", names.as_slice())
        .label("Theme")
        .appearance(SelectAppearance::Field)
        .placeholder(format!("Inherit ({inherit})"))
        .current(current)
        .fill()
        .open(model.profile.theme_open);
    if let Some([width, _]) = cx.layout_size("profile-theme-trigger") {
        select = select.menu_width(width);
    }
    select
        .on_toggle(|m: &mut SettingsModel| {
            m.profile.theme_open = !m.profile.theme_open;
        })
        .on_close(|m: &mut SettingsModel| {
            m.profile.theme_open = false;
        })
        .on_choose(|m: &mut SettingsModel, value: &str, _tx| {
            m.profile.theme = value.to_owned();
            m.profile.theme_open = false;
            reflect_profile_theme(m);
            preview_theme(m);
        })
        .build(kit, overlays)
}

/// Shows the theme the profile has in the terminal and in this window.
pub fn preview_theme(model: &mut SettingsModel) {
    let palette = match model.profile.theme.is_empty() {
        true => model.config.resolve_palette(),
        false => super::themes::palette(&model.config, &model.profile.theme),
    };
    super::preview(model, palette);
}

pub fn actions(kit: &Kit, model: &SettingsModel) -> Vec<El> {
    let dirty = model.profile.is_dirty();
    vec![
        super::action(kit, "profile-save", &super::SAVE, "Save", dirty, save),
        super::action(
            kit,
            "profile-revert",
            &super::REVERT,
            "Revert",
            dirty,
            revert,
        ),
        super::action(
            kit,
            "profile-duplicate",
            kit.glyph.copy,
            "Duplicate",
            true,
            duplicate,
        ),
        super::action(
            kit,
            "profile-delete",
            kit.glyph.close,
            "Delete",
            model.config.profiles.len() > 1,
            delete,
        ),
    ]
}

/// Puts the name into the list. The rest of the draft stays, so a field that
/// is still focused keeps its caret.
pub(crate) fn reflect_name(model: &mut SettingsModel) {
    let name = model.profile.name.text.trim().to_owned();
    if name.is_empty() {
        model.status = Some("A profile needs a name".to_owned());
        return;
    }
    if !settings::name_ok(&name) {
        model.status = Some(format!("'{name}' cannot be a file name"));
        return;
    }
    if name == model.profile.original {
        if model.profile.name.text != name {
            model.profile.name.replace_text(name);
        }
        model.status = None;
        return;
    }
    if !model.config.rename_profile(&model.profile.original, &name) {
        model.status = Some(format!("A profile named '{name}' already exists"));
        return;
    }
    model.profile.original = name.clone();
    if model.profile.name.text != name {
        model.profile.name.replace_text(&name);
    }
    if let Some(index) = model.config.profiles.keys().position(|key| key == &name) {
        model.selected = index;
    }
    model.status = None;
    if super::write_config(model) {
        super::push(model);
    }
}

/// Writes one finished field. The rest of the draft stays, so a field that is
/// still focused keeps its caret.
fn reflect_field(
    model: &mut SettingsModel,
    edit: impl FnOnce(&mut ProfileDraft, &mut Profile) -> Result<bool, String>,
) {
    let Some(mut profile) = model.config.profiles.get(&model.profile.original).cloned() else {
        return;
    };
    match edit(&mut model.profile, &mut profile) {
        Err(err) => model.status = Some(err),
        Ok(false) => model.status = None,
        Ok(true) => {
            model.profile.saved = profile.clone();
            let name = model.profile.original.clone();
            model.config.profiles.insert(name, profile);
            model.status = None;
            if super::write_config(model) {
                super::push(model);
            }
        }
    }
}

fn reflect_font_family(model: &mut SettingsModel) {
    reflect_field(model, |draft, profile| {
        let family = draft.font_family.text.trim().to_owned();
        if family.is_empty() {
            return Err("A profile needs a font family".to_owned());
        }
        show(&mut draft.font_family, &family);
        let changed = profile.font_family != family;
        profile.font_family = family;
        Ok(changed)
    });
}

pub(crate) fn reflect_font_size(model: &mut SettingsModel) {
    reflect_field(model, |draft, profile| {
        let text = draft.font_size.text.trim();
        let size = match text {
            "" => None,
            text => Some(settings::clamp_font_size(
                text.parse::<f32>()
                    .map_err(|_| format!("'{text}' is not a font size"))?,
            )),
        };
        let shown = size.map(format_size).unwrap_or_default();
        show(&mut draft.font_size, &shown);
        let changed = profile.font_size != size;
        profile.font_size = size;
        Ok(changed)
    });
}

fn reflect_shell_program(model: &mut SettingsModel) {
    reflect_field(model, |draft, profile| {
        let program = non_empty(&draft.shell_program.text);
        show(&mut draft.shell_program, program.as_deref().unwrap_or(""));
        Ok(set_program(&mut profile.shell, program))
    });
}

fn reflect_shell_args(model: &mut SettingsModel) {
    reflect_field(model, |draft, profile| {
        let args: Vec<String> = draft
            .shell_args
            .text
            .split_whitespace()
            .map(str::to_owned)
            .collect();
        show(&mut draft.shell_args, &args.join(" "));
        Ok(set_args(&mut profile.shell, args))
    });
}

pub(crate) fn reflect_cwd(model: &mut SettingsModel) {
    reflect_field(model, |draft, profile| {
        let cwd = non_empty(&draft.cwd.text);
        show(&mut draft.cwd, cwd.as_deref().unwrap_or(""));
        let changed = profile.cwd != cwd;
        profile.cwd = cwd;
        Ok(changed)
    });
}

fn reflect_profile_theme(model: &mut SettingsModel) {
    reflect_field(model, |draft, profile| {
        let theme = non_empty(&draft.theme);
        let changed = profile.theme != theme;
        profile.theme = theme;
        Ok(changed)
    });
}

fn show(edit: &mut TextEdit, text: &str) {
    if edit.text != text {
        edit.replace_text(text);
    }
}

fn set_program(shell: &mut Option<ShellSpec>, program: Option<String>) -> bool {
    let current = shell.as_ref().and_then(|spec| spec.program.clone());
    if current == program {
        return false;
    }
    match shell {
        Some(spec) => spec.program = program,
        None => {
            *shell = Some(ShellSpec {
                program,
                args: Vec::new(),
            })
        }
    }
    if shell.as_ref().is_some_and(ShellSpec::is_empty) {
        *shell = None;
    }
    true
}

fn set_args(shell: &mut Option<ShellSpec>, args: Vec<String>) -> bool {
    let current = shell
        .as_ref()
        .map(|spec| spec.args.clone())
        .unwrap_or_default();
    if current == args {
        return false;
    }
    match shell {
        Some(spec) => spec.args = args,
        None => {
            *shell = Some(ShellSpec {
                program: None,
                args,
            })
        }
    }
    if shell.as_ref().is_some_and(ShellSpec::is_empty) {
        *shell = None;
    }
    true
}

/// Writes the draft into the config, renaming the profile when its name
/// changed. `false` when it could not.
pub(crate) fn commit(model: &mut SettingsModel) -> bool {
    let (name, profile) = match model.profile.to_profile() {
        Ok(profile) => profile,
        Err(err) => {
            model.status = Some(err);
            return false;
        }
    };
    let original = model.profile.original.clone();
    if name != original && !model.config.rename_profile(&original, &name) {
        model.status = Some(format!("A profile named '{name}' already exists"));
        return false;
    }
    let changed = name != original || model.config.profiles.get(&original) != Some(&profile);
    model.config.profiles.insert(name.clone(), profile);
    model.profile.original = name.clone();
    changed
}

/// Writes the draft into the config and hands it to the terminal.
pub fn save(model: &mut SettingsModel) {
    if !commit(model) {
        return;
    }
    select_by_name(model, &model.profile.original.clone());
    if super::write_config(model) {
        super::push(model);
    }
}

/// Puts the draft back as it was loaded, and the terminal with it.
pub fn revert(model: &mut SettingsModel) {
    model.profile.revert();
    model.status = None;
    super::restore(model);
}

/// Makes the profile the one new sessions use.
pub fn set_default(model: &mut SettingsModel, name: &str) {
    if model.config.set_default_profile(name) && super::write_config(model) {
        super::push(model);
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
    if super::write_config(model) {
        super::push(model);
    }
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
    if super::write_config(model) {
        super::push(model);
    }
}

/// Adds a profile with the default settings.
pub fn create(model: &mut SettingsModel) {
    let name = model.config.unique_profile_name("Profile");
    model
        .config
        .profiles
        .insert(name.clone(), Profile::default());
    select_by_name(model, &name);
    if super::write_config(model) {
        super::push(model);
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
