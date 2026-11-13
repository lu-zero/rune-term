//! Settings, theme, and profile actions driven by app UI.

use rune::Tx;

use crate::model::Model;

pub fn apply_font_family(model: &mut Model, _tx: &mut Tx, family: &str) {
    let family = family.trim();
    if family.is_empty() {
        return;
    }
    model.config.set_font_family(family);
    // Font family is stored in config/profile; terminal atlas rendering is not
    // wired to it yet, but settings UI reflects the saved selection.
    if let Err(err) = model.config.save() {
        eprintln!("rune-term: failed to save settings: {err}");
    }
}

pub fn apply_theme(model: &mut Model, _tx: &mut Tx, name: &str) {
    model.config.set_theme(name.to_owned());
    model.palette = model.config.resolve_palette();
    if let Err(err) = model.config.save() {
        eprintln!("rune-term: failed to save settings: {err}");
    }
}

pub fn copy_theme_named(model: &mut Model, _tx: &mut Tx, source_name: &str) {
    let name = model.config.copy_theme(source_name);
    model.palette = model.config.resolve_palette();
    if let Some(custom) = model.config.custom_themes.get(&name).cloned() {
        model.ui.settings_mode =
            crate::settings_ui::SettingsMode::Open(Box::new(crate::settings_ui::SettingsUi {
                tab: crate::settings_ui::SettingsTab::Theme,
                theme_editor: Some(crate::settings_ui::ThemeEditor::from_custom(&name, &custom)),
                profile_editor: None,
                font_family_picker_open: false,
                font_family_before_pick: None,
                font_search: rune::TextEdit::new(""),
                font_picker_active: 0,
            }));
    } else if let crate::settings_ui::SettingsMode::Open(ui) = &mut model.ui.settings_mode {
        ui.tab = crate::settings_ui::SettingsTab::Theme;
        ui.theme_editor = None;
    } else {
        model.ui.settings_mode =
            crate::settings_ui::SettingsMode::Open(Box::new(crate::settings_ui::SettingsUi {
                tab: crate::settings_ui::SettingsTab::Theme,
                theme_editor: None,
                profile_editor: None,
                font_family_picker_open: false,
                font_family_before_pick: None,
                font_search: rune::TextEdit::new(""),
                font_picker_active: 0,
            }));
    }
    model.terminal.invalidate_theme();
    if let Err(err) = model.config.save() {
        eprintln!("rune-term: failed to save settings: {err}");
    }
}

pub fn edit_theme_named(model: &mut Model, tx: &mut Tx, name: &str) {
    if let Some(custom) = model.config.custom_themes.get(name).cloned() {
        model.config.set_theme(name.to_owned());
        model.palette = model.config.resolve_palette();
        model.ui.settings_mode =
            crate::settings_ui::SettingsMode::Open(Box::new(crate::settings_ui::SettingsUi {
                tab: crate::settings_ui::SettingsTab::Theme,
                theme_editor: Some(crate::settings_ui::ThemeEditor::from_custom(name, &custom)),
                profile_editor: None,
                font_family_picker_open: false,
                font_family_before_pick: None,
                font_search: rune::TextEdit::new(""),
                font_picker_active: 0,
            }));
        model.terminal.invalidate_theme();
    } else if crate::theme::resolve_builtin(name).is_some() {
        copy_theme_named(model, tx, name);
    } else if let crate::settings_ui::SettingsMode::Open(ui) = &mut model.ui.settings_mode {
        ui.tab = crate::settings_ui::SettingsTab::Theme;
        ui.theme_editor = None;
    }
}

pub fn cancel_theme_edit(model: &mut Model, _tx: &mut Tx) {
    if let crate::settings_ui::SettingsMode::Open(ui) = &mut model.ui.settings_mode {
        ui.tab = crate::settings_ui::SettingsTab::Theme;
        ui.theme_editor = None;
    }
    model.palette = model.config.resolve_palette();
    model.terminal.invalidate_theme();
}

pub fn preview_theme_edit(model: &mut Model, _tx: &mut Tx) {
    let crate::settings_ui::SettingsMode::Open(ui) = &model.ui.settings_mode else {
        return;
    };
    let Some(editor) = ui.theme_editor.as_ref() else {
        return;
    };
    let fallback = model
        .config
        .custom_themes
        .get(&editor.original_name)
        .cloned()
        .unwrap_or_else(|| crate::settings::CustomTheme::from_palette(&model.palette));
    model.palette = editor.preview_palette(&fallback);
    model.terminal.invalidate_theme();
}

pub fn select_theme_color_field(model: &mut Model, field: crate::settings_ui::ThemeColorField) {
    if let crate::settings_ui::SettingsMode::Open(ui) = &mut model.ui.settings_mode {
        if let Some(editor) = ui.theme_editor.as_mut() {
            editor.picking = if editor.picking == Some(field) {
                None
            } else {
                Some(field)
            };
        }
    }
}

pub fn save_theme_edit(model: &mut Model, _tx: &mut Tx) {
    let editor = match &mut model.ui.settings_mode {
        crate::settings_ui::SettingsMode::Open(ui) => {
            ui.tab = crate::settings_ui::SettingsTab::Theme;
            ui.theme_editor.take()
        }
        _ => return,
    };
    let Some(editor) = editor else {
        return;
    };
    let Some((name, custom)) = editor.to_custom() else {
        if let crate::settings_ui::SettingsMode::Open(ui) = &mut model.ui.settings_mode {
            ui.theme_editor = Some(editor);
        }
        return;
    };
    let original = editor.original_name.clone();
    if name != original {
        model.config.custom_themes.remove(&original);
    }
    // Avoid overwriting a built-in or different custom theme name.
    if name != original
        && (crate::theme::resolve_builtin(&name).is_some()
            || model.config.custom_themes.contains_key(&name))
    {
        if let crate::settings_ui::SettingsMode::Open(ui) = &mut model.ui.settings_mode {
            ui.theme_editor = Some(crate::settings_ui::ThemeEditor::from_custom(
                &original, &custom,
            ));
        }
        model.terminal.invalidate_theme();
        return;
    }
    model.config.custom_themes.insert(name.clone(), custom);
    model.config.set_theme(name);
    model.palette = model.config.resolve_palette();
    model.terminal.invalidate_theme();
    if let Err(err) = model.config.save() {
        eprintln!("rune-term: failed to save settings: {err}");
    }
}

pub fn delete_theme_named(model: &mut Model, _tx: &mut Tx, name: &str) {
    if !model.config.is_custom_theme(name) {
        return;
    }
    model.config.custom_themes.remove(name);
    if let crate::settings_ui::SettingsMode::Open(ui) = &mut model.ui.settings_mode {
        if ui
            .theme_editor
            .as_ref()
            .is_some_and(|e| e.original_name == name)
        {
            ui.tab = crate::settings_ui::SettingsTab::Theme;
            ui.theme_editor = None;
        }
    }
    if model.palette.name == name {
        model.config.set_theme("Flexoki Black".to_owned());
        model.palette = model.config.resolve_palette();
        model.terminal.invalidate_theme();
    }
    if let Err(err) = model.config.save() {
        eprintln!("rune-term: failed to save settings: {err}");
    }
}

pub fn set_default_profile(model: &mut Model, _tx: &mut Tx, name: &str) {
    if model.config.set_default_profile(name) {
        let resolved = model.config.resolve();
        if (resolved.font_size - model.font_size).abs() >= f32::EPSILON {
            model.font_size = resolved.font_size;
            model.cell_points = crate::metrics::cell_points_for(resolved.font_size);
        }
        if resolved.theme != model.palette.name {
            model.palette = model.config.resolve_palette();
            model.terminal.invalidate_theme();
        }
        if let Err(err) = model.config.save() {
            eprintln!("rune-term: failed to save settings: {err}");
        }
    }
}

pub fn rename_profile(model: &mut Model, _tx: &mut Tx, from: &str, to: &str) -> bool {
    let to = to.trim().to_owned();
    if !model.config.rename_profile(from, &to) {
        return false;
    }
    for tab in &mut model.tabs {
        if tab.profile == from {
            tab.profile = to.clone();
        }
    }
    crate::tabs::apply_active_profile(model);
    if let Err(err) = model.config.save() {
        eprintln!("rune-term: failed to save settings: {err}");
        return false;
    }
    true
}

#[allow(dead_code)]
pub fn update_profile_field(
    model: &mut Model,
    _tx: &mut Tx,
    name: &str,
    f: impl FnOnce(&mut crate::settings::Profile),
) -> bool {
    let Some(profile) = model.config.profiles.get_mut(name) else {
        return false;
    };
    f(profile);
    if let Some(tab) = model.tabs.get(model.active) {
        if tab.profile == name {
            crate::tabs::apply_active_profile(model);
        }
    }
    if let Err(err) = model.config.save() {
        eprintln!("rune-term: failed to save settings: {err}");
        return false;
    }
    true
}

pub fn duplicate_profile(model: &mut Model, _tx: &mut Tx, source: &str) -> Option<String> {
    let profile = model.config.profile(source).cloned()?;
    let base = format!("{source} Copy");
    let mut name = base.clone();
    let mut n = 2u32;
    while model.config.profile(&name).is_some() {
        name = format!("{base} {n}");
        n += 1;
    }
    model.config.profiles.insert(name.clone(), profile);
    if let Err(err) = model.config.save() {
        eprintln!("rune-term: failed to save settings: {err}");
        return None;
    }
    Some(name)
}

pub fn delete_profile(model: &mut Model, _tx: &mut Tx, name: &str) -> bool {
    if model.config.profiles.len() <= 1 || !model.config.profiles.contains_key(name) {
        return false;
    }
    let was_default = model.config.default_profile_name() == name;
    model.config.profiles.remove(name);
    if was_default {
        let next = model
            .config
            .profiles
            .keys()
            .next()
            .cloned()
            .unwrap_or_else(|| crate::settings::DEFAULT_PROFILE.to_owned());
        model.config.settings.default_profile = next;
        let resolved = model.config.resolve();
        model.font_size = resolved.font_size;
        model.cell_points = crate::metrics::cell_points_for(resolved.font_size);
        model.palette = model.config.resolve_palette();
        model.terminal.invalidate_theme();
    }
    model.config.normalize();
    if let Err(err) = model.config.save() {
        eprintln!("rune-term: failed to save settings: {err}");
        return false;
    }
    true
}

pub fn edit_profile_named(model: &mut Model, _tx: &mut Tx, name: &str) {
    let Some(profile) = model.config.profile(name).cloned() else {
        return;
    };
    model.ui.settings_mode =
        crate::settings_ui::SettingsMode::Open(Box::new(crate::settings_ui::SettingsUi {
            tab: crate::settings_ui::SettingsTab::Profiles,
            theme_editor: None,
            profile_editor: Some(crate::settings_ui::ProfileEditor::from_profile(
                name, &profile,
            )),
            font_family_picker_open: false,
            font_family_before_pick: None,
            font_search: rune::TextEdit::new(""),
            font_picker_active: 0,
        }));
}

pub fn cancel_profile_edit(model: &mut Model, _tx: &mut Tx) {
    if let crate::settings_ui::SettingsMode::Open(ui) = &mut model.ui.settings_mode {
        ui.tab = crate::settings_ui::SettingsTab::Profiles;
        ui.profile_editor = None;
    }
}

pub fn save_profile_edit(model: &mut Model, tx: &mut Tx) {
    let Some((editor, original)) = (match &model.ui.settings_mode {
        crate::settings_ui::SettingsMode::Open(ui) => ui
            .profile_editor
            .as_ref()
            .map(|e| (e.clone(), e.original_name.clone())),
        _ => None,
    }) else {
        return;
    };
    let Some((new_name, profile)) = editor.to_profile() else {
        return;
    };
    if !model.config.profiles.contains_key(&original) {
        return;
    }
    if new_name != original && model.config.profiles.contains_key(&new_name) {
        return;
    }
    if let Some(p) = model.config.profiles.get_mut(&original) {
        *p = profile;
    }
    if new_name != original {
        if !rename_profile(model, tx, &original, &new_name) {
            return;
        }
    } else {
        model.config.normalize();
        crate::tabs::apply_active_profile(model);
        if let Err(err) = model.config.save() {
            eprintln!("rune-term: failed to save settings: {err}");
        }
    }
    if let crate::settings_ui::SettingsMode::Open(ui) = &mut model.ui.settings_mode {
        ui.tab = crate::settings_ui::SettingsTab::Profiles;
        ui.profile_editor = None;
    }
}

#[cfg(test)]
mod tests {
    use rune::Tx;

    use super::rename_profile;

    #[test]
    fn rename_profile_updates_tabs() {
        let dir = std::env::temp_dir().join(format!(
            "rune-term-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        std::env::set_var("XDG_CONFIG_HOME", &dir);

        let mut model = crate::model::init();
        model.config.ensure_profile("Work");
        crate::tabs::open_with_profile(&mut model, "Work");
        assert_eq!(model.tabs[model.active].profile, "Work");

        let mut tx = Tx::default();
        assert!(rename_profile(&mut model, &mut tx, "Work", "Office"));
        assert!(model.tabs.iter().any(|t| t.profile == "Office"));
        assert!(!model.tabs.iter().any(|t| t.profile == "Work"));
        assert!(model.config.profile("Office").is_some());
        assert_eq!(
            model.config.default_profile_name(),
            crate::settings::DEFAULT_PROFILE
        );

        std::env::remove_var("XDG_CONFIG_HOME");
        let _ = std::fs::remove_dir_all(dir);
    }
}
