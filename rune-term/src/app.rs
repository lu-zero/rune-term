//! Window shell. `perform_resize` is frozen; the winsize change does not edit it.

use std::sync::atomic::Ordering;

use rune::compose::DeviceKey;
use rune::render::{Instance, KIND_EFFECT};
use rune::{App, AppSpec, Compositor, Frame, MenuItem, Titlebar, Tx};
use term_grid::images::ImageBucket;
use term_grid::{DrawSlot, GridRenderer, SolidSlot};

use crate::chrome;
use crate::delegate;
use crate::grid_pass;
use crate::images::Images;
use crate::ime;
use crate::model::{self, Model};
use crate::pane_input;
use crate::resize::{self, GridMetrics, ResizeEffect, ResizeRequest};
use crate::scrollback::{self, Bars};

pub fn run() {
    app().run();
}

/// Run with explicit arguments forwarded to Rune's tooling layer, so Rune does
/// not parse the process argv.
pub fn run_with_rune_args(args: impl IntoIterator<Item = String>) {
    app().args(args).run();
}

pub fn app() -> App<Model> {
    let mut settings = MenuItem::new("Settings…", |model: &mut Model, _tx: &mut Tx| {
        model.settings_mode =
            crate::model::SettingsMode::Open(Box::new(crate::model::SettingsUi {
                tab: crate::model::SettingsTab::Font,
                theme_editor: None,
                profile_editor: None,
                font_family_picker_open: false,
                font_family_before_pick: None,
                font_search: rune::TextEdit::new(""),
                font_picker_active: 0,
            }));
    });
    #[cfg(target_os = "macos")]
    if let Ok(sc) = "cmd+comma".parse::<rune::Shortcut>() {
        settings = settings.key_equivalent(sc);
    }
    App::new(spec())
        .compositor(GridCompositor::default())
        .on_ime(ime::on_ime)
        .on_modifiers(|model, mods, tx| {
            let changed = model.held_mods != mods;
            model.held_mods = mods;
            if changed
                && model
                    .surfaces
                    .iter_mut()
                    .any(|host| host.hover_link.is_some())
            {
                tx.wake_after(std::time::Duration::ZERO);
            }
        })
        .titlebar(Titlebar::new(chrome::TITLEBAR_H))
        .menu_item(settings)
}

fn spec() -> AppSpec<Model> {
    AppSpec {
        setup: Some(delegate::setup),
        poll: Some(delegate::poll),
        on_key: Some(pane_input::on_key),
        ..AppSpec::new("rune-term", (800.0, 500.0), model::init, chrome::view)
    }
}

/// Applies `window_points`, `Surface::resize`, `resized`, and `live_only`.
/// The caller stores `cols`, `rows`, and `cell_px` on the host.
pub fn perform_resize(tx: &mut Tx, host: &crate::surfaces::HostSurface, effect: ResizeEffect) {
    if let Some(size) = effect.window_points {
        tx.resize_window_instant(size);
    }
    if effect.resize_surface {
        host.surface.resize(
            effect.cols,
            effect.rows,
            effect.pixel_width,
            effect.pixel_height,
        );
        host.resized.store(true, Ordering::Release);
    }
    if effect.live_only {
        tx.live_only();
    }
}

pub fn on_pane_layout(model: &mut Model, slot: (f32, f32), scale: f32, tx: &mut Tx) {
    if scale.is_finite() && scale > 0.0 {
        model.scale = scale;
    }
    let Some(effect) = fit_layout(model, slot, scale) else {
        return;
    };
    let Some(host) = model.surfaces.primary() else {
        return;
    };
    // `fit_layout` already resized the surface. `perform_resize` still owns
    // the window snap and `live_only`.
    let mut delivered = effect;
    delivered.resize_surface = false;
    perform_resize(tx, host, delivered);
}

/// Floors the pane and resizes the surface when the cell count changed.
/// The layout callback reports the rect on the following frame, so the
/// paint path calls this with the rect it is about to draw.
pub fn fit_layout(model: &mut Model, slot: (f32, f32), scale: f32) -> Option<ResizeEffect> {
    if scale.is_finite() && scale > 0.0 {
        model.scale = scale;
    }
    let cell_points = model.cell_points;
    let host = model.surfaces.primary_mut()?;
    let effect = resize::apply(ResizeRequest::Layout {
        slot_points: slot,
        scale,
        cell_points,
        current: GridMetrics {
            cols: host.cols,
            rows: host.rows,
            cell_px: host.cell_px,
        },
    });
    if effect.resize_surface {
        host.cols = effect.cols;
        host.rows = effect.rows;
        host.cell_px = effect.cell_px;
        host.surface.resize(
            effect.cols,
            effect.rows,
            effect.pixel_width,
            effect.pixel_height,
        );
        host.resized.store(true, Ordering::Release);
    }
    Some(effect)
}

pub fn apply_font_size(model: &mut Model, tx: &mut Tx, size: f32) {
    let size = crate::settings::clamp_font_size(size);
    if (size - model.font_size).abs() < f32::EPSILON {
        return;
    }
    model.font_size = size;
    model.config.set_font_size(size);
    model.cell_points = crate::metrics::cell_points_for(size);
    let scale = if model.scale.is_finite() && model.scale > 0.0 {
        model.scale
    } else {
        1.0
    };
    let Some(host) = model.surfaces.primary() else {
        let _ = model.config.save();
        return;
    };
    let effect = resize::apply(ResizeRequest::Font {
        chrome_points: (0.0, crate::chrome::TITLEBAR_H),
        cols: host.cols.max(1),
        rows: host.rows.max(1),
        cell_points: model.cell_points,
        scale,
    });
    perform_resize(tx, host, effect);
    if let Err(err) = model.config.save() {
        eprintln!("rune-term: failed to save settings: {err}");
    }
}

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
        model.settings_mode =
            crate::model::SettingsMode::Open(Box::new(crate::model::SettingsUi {
                tab: crate::model::SettingsTab::Theme,
                theme_editor: Some(crate::model::ThemeEditor::from_custom(&name, &custom)),
                profile_editor: None,
                font_family_picker_open: false,
                font_family_before_pick: None,
                font_search: rune::TextEdit::new(""),
                font_picker_active: 0,
            }));
    } else if let crate::model::SettingsMode::Open(ui) = &mut model.settings_mode {
        ui.tab = crate::model::SettingsTab::Theme;
        ui.theme_editor = None;
    } else {
        model.settings_mode =
            crate::model::SettingsMode::Open(Box::new(crate::model::SettingsUi {
                tab: crate::model::SettingsTab::Theme,
                theme_editor: None,
                profile_editor: None,
                font_family_picker_open: false,
                font_family_before_pick: None,
                font_search: rune::TextEdit::new(""),
                font_picker_active: 0,
            }));
    }
    model.grid_draw.invalidate_theme();
    if let Err(err) = model.config.save() {
        eprintln!("rune-term: failed to save settings: {err}");
    }
}

pub fn edit_theme_named(model: &mut Model, tx: &mut Tx, name: &str) {
    if let Some(custom) = model.config.custom_themes.get(name).cloned() {
        model.config.set_theme(name.to_owned());
        model.palette = model.config.resolve_palette();
        model.settings_mode =
            crate::model::SettingsMode::Open(Box::new(crate::model::SettingsUi {
                tab: crate::model::SettingsTab::Theme,
                theme_editor: Some(crate::model::ThemeEditor::from_custom(name, &custom)),
                profile_editor: None,
                font_family_picker_open: false,
                font_family_before_pick: None,
                font_search: rune::TextEdit::new(""),
                font_picker_active: 0,
            }));
        model.grid_draw.invalidate_theme();
    } else if crate::theme::resolve_builtin(name).is_some() {
        copy_theme_named(model, tx, name);
    } else if let crate::model::SettingsMode::Open(ui) = &mut model.settings_mode {
        ui.tab = crate::model::SettingsTab::Theme;
        ui.theme_editor = None;
    }
}

pub fn cancel_theme_edit(model: &mut Model, _tx: &mut Tx) {
    if let crate::model::SettingsMode::Open(ui) = &mut model.settings_mode {
        ui.tab = crate::model::SettingsTab::Theme;
        ui.theme_editor = None;
    }
    model.palette = model.config.resolve_palette();
    model.grid_draw.invalidate_theme();
}

pub fn preview_theme_edit(model: &mut Model, _tx: &mut Tx) {
    let crate::model::SettingsMode::Open(ui) = &model.settings_mode else {
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
    model.grid_draw.invalidate_theme();
}

pub fn select_theme_color_field(model: &mut Model, field: crate::model::ThemeColorField) {
    if let crate::model::SettingsMode::Open(ui) = &mut model.settings_mode {
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
    let editor = match &mut model.settings_mode {
        crate::model::SettingsMode::Open(ui) => {
            ui.tab = crate::model::SettingsTab::Theme;
            ui.theme_editor.take()
        }
        _ => return,
    };
    let Some(editor) = editor else {
        return;
    };
    let Some((name, custom)) = editor.to_custom() else {
        if let crate::model::SettingsMode::Open(ui) = &mut model.settings_mode {
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
        if let crate::model::SettingsMode::Open(ui) = &mut model.settings_mode {
            ui.theme_editor = Some(crate::model::ThemeEditor::from_custom(&original, &custom));
        }
        model.grid_draw.invalidate_theme();
        return;
    }
    model.config.custom_themes.insert(name.clone(), custom);
    model.config.set_theme(name);
    model.palette = model.config.resolve_palette();
    model.grid_draw.invalidate_theme();
    if let Err(err) = model.config.save() {
        eprintln!("rune-term: failed to save settings: {err}");
    }
}

pub fn delete_theme_named(model: &mut Model, _tx: &mut Tx, name: &str) {
    if !model.config.is_custom_theme(name) {
        return;
    }
    model.config.custom_themes.remove(name);
    if let crate::model::SettingsMode::Open(ui) = &mut model.settings_mode {
        if ui
            .theme_editor
            .as_ref()
            .is_some_and(|e| e.original_name == name)
        {
            ui.tab = crate::model::SettingsTab::Theme;
            ui.theme_editor = None;
        }
    }
    if model.palette.name == name {
        model.config.set_theme("Flexoki Black".to_owned());
        model.palette = model.config.resolve_palette();
        model.grid_draw.invalidate_theme();
    }
    if let Err(err) = model.config.save() {
        eprintln!("rune-term: failed to save settings: {err}");
    }
}

#[derive(Default)]
struct GridCompositor {
    slot: Option<SolidSlot>,
    grid: Option<GridRenderer>,
    images: Option<Images>,
    bars: Option<Bars>,
    key: Option<DeviceKey>,
    format: Option<wgpu::TextureFormat>,
}

impl GridCompositor {
    fn ensure(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        key: DeviceKey,
    ) {
        let same = self.key == Some(key)
            && self.format == Some(format)
            && self.slot.is_some()
            && self.grid.is_some()
            && self.images.is_some()
            && self.bars.is_some();
        if same {
            return;
        }
        self.slot = Some(SolidSlot::new(device, format));
        self.grid = Some(GridRenderer::new(device, queue, format));
        self.images = Some(Images::new(device, format));
        self.bars = Some(Bars::new(device, format));
        self.key = Some(key);
        self.format = Some(format);
    }
}

impl Compositor<Model> for GridCompositor {
    fn render(&mut self, f: &mut Frame<Model>) {
        self.ensure(f.device, f.queue, f.format, f.device_key());
        let srgb_blend = f.srgb_blend();
        let color = chrome::slot_premul(f.rt.state.palette.bg, srgb_blend);
        let blinking = f.rt.state.surfaces.primary().is_some_and(|host| host.blink);
        let cursor_on = f.rt.state.blink_phase_on || !blinking;
        let pane = pane_instance(&f.rt.instances);
        if let Some((rect, scale)) = pane {
            if scale.is_finite() && scale > 0.0 {
                fit_layout(&mut f.rt.state, (rect[2] / scale, rect[3] / scale), scale);
            }
        }
        grid_pass::sync(
            &mut f.rt.state,
            self.grid.as_mut().expect("grid"),
            self.images.as_mut().expect("images"),
            f.device,
            f.queue,
        );
        let clear = f.rt.clear_color();
        f.ui.prepare(
            f.device,
            f.queue,
            &mut f.rt.text.atlas,
            &f.rt.instances,
            f.size,
        );
        let mut pane_origin = [0.0, 0.0];
        if let Some((rect, scale)) = pane {
            let margin = if scale.is_finite() && scale > 0.0 {
                f32::from(resize::margin_px(scale))
            } else {
                0.0
            };
            pane_origin = [rect[0] + margin, rect[1] + margin];
            self.grid.as_mut().expect("grid").publish(
                f.queue,
                DrawSlot {
                    origin: pane_origin,
                    size: [
                        (rect[2] - margin * 2.0).max(0.0),
                        (rect[3] - margin * 2.0).max(0.0),
                    ],
                    target: [f.size.0, f.size.1],
                    pass_origin: [0.0, 0.0],
                },
                srgb_blend,
                cursor_on,
            );
        }
        let slot = self.slot.as_ref().expect("solid slot");
        slot.write_color(f.queue, color);
        let bars = self.bars.as_ref().expect("scrollbar");
        let grid = self.grid.as_ref().expect("grid");
        let images = self.images.as_mut().expect("images");
        let mark = chrome::PANE_MARK;
        let queue = f.queue;
        let device = f.device;
        f.ui.draw_with(f.encoder, f.target, clear, &mut |pass, site| {
            if scrollback::is_scroll(site.mark) {
                bars.draw(queue, srgb_blend, pass, site, &f.rt.state);
                return;
            }
            if site.mark != mark {
                return;
            }
            // Rune's scissor is the ancestor clip. With no scroll view that is
            // the window, so a fullscreen triangle would cover the tab strip.
            let Some((x, y, w, h)) = pane_scissor(site) else {
                return;
            };
            pass.set_scissor_rect(x, y, w, h);
            slot.draw(pass);
            grid.draw_bg(pass);
            images.draw_bucket(
                device,
                queue,
                pass,
                ImageBucket::UnderText,
                pane_origin,
                site.origin,
                site.size,
                srgb_blend,
            );
            grid.draw_text(pass);
            images.draw_bucket(
                device,
                queue,
                pass,
                ImageBucket::OverText,
                pane_origin,
                site.origin,
                site.size,
                srgb_blend,
            );
        });
    }
}

fn pane_instance(instances: &[Instance]) -> Option<([f32; 4], f32)> {
    instances.iter().find_map(|inst| {
        if inst.params[1] != KIND_EFFECT {
            return None;
        }
        let mark = (inst.border[0] as u32 & 0xFFFF) | ((inst.border[1] as u32 & 0xFFFF) << 16);
        if mark == chrome::PANE_MARK {
            Some((inst.rect, inst.params[3]))
        } else {
            None
        }
    })
}

/// Device-pixel scissor of the pane rect cut by its clip, in this pass's target.
fn pane_scissor(site: &rune::render::EffectSite<'_>) -> Option<(u32, u32, u32, u32)> {
    let rect = site.instance.rect;
    let clip = site.instance.clip;
    let x0 = rect[0].max(clip[0]);
    let y0 = rect[1].max(clip[1]);
    let x1 = (rect[0] + rect[2]).min(clip[2]);
    let y1 = (rect[1] + rect[3]).min(clip[3]);
    let edge = |v: f32, origin: f32, max: u32| (v - origin).clamp(0.0, max as f32) as u32;
    let (ox, oy) = (site.origin[0], site.origin[1]);
    let (sx0, sy0) = (
        edge(x0.floor(), ox, site.size[0]),
        edge(y0.floor(), oy, site.size[1]),
    );
    let (sx1, sy1) = (
        edge(x1.ceil(), ox, site.size[0]),
        edge(y1.ceil(), oy, site.size[1]),
    );
    if sx1 <= sx0 || sy1 <= sy0 {
        return None;
    }
    Some((sx0, sy0, sx1 - sx0, sy1 - sy0))
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
            model.grid_draw.invalidate_theme();
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
        model.grid_draw.invalidate_theme();
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
    model.settings_mode = crate::model::SettingsMode::Open(Box::new(crate::model::SettingsUi {
        tab: crate::model::SettingsTab::Profiles,
        theme_editor: None,
        profile_editor: Some(crate::model::ProfileEditor::from_profile(name, &profile)),
        font_family_picker_open: false,
        font_family_before_pick: None,
        font_search: rune::TextEdit::new(""),
        font_picker_active: 0,
    }));
}

pub fn cancel_profile_edit(model: &mut Model, _tx: &mut Tx) {
    if let crate::model::SettingsMode::Open(ui) = &mut model.settings_mode {
        ui.tab = crate::model::SettingsTab::Profiles;
        ui.profile_editor = None;
    }
}

pub fn save_profile_edit(model: &mut Model, tx: &mut Tx) {
    let Some((editor, original)) = (match &model.settings_mode {
        crate::model::SettingsMode::Open(ui) => ui
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
    if let crate::model::SettingsMode::Open(ui) = &mut model.settings_mode {
        ui.tab = crate::model::SettingsTab::Profiles;
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
