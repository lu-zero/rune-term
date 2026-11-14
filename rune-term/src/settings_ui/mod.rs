//! The settings window: a profiles list and a themes list, with CRUD.
//!
//! The window is its own Rune app with its own state. It edits a copy of the
//! config, writes it back, and pushes an applied change into the terminal
//! window, which owns the running grid.

pub mod profiles;
pub mod themes;

use std::sync::Arc;

use rune::{
    col, el, icon, row, text, AppHandle, AppSpec, Color, Cx, El, Key, KeyEvent, ScrollSpec, Tx,
    WindowOptions, WindowSpec,
};
use rune_kit::Kit;

use crate::model::Model;
use crate::settings::{self, ConfigFile};
use crate::theme::{self, Palette};

/// The window's size, and the smallest it may be resized to.
pub const SIZE: (f32, f32) = (880.0, 560.0);
pub const MIN_SIZE: (f32, f32) = (620.0, 400.0);

/// The width of the list column.
const SIDEBAR_W: f32 = 300.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Tab {
    #[default]
    Profiles,
    Themes,
}

/// The settings window's state.
pub struct SettingsModel {
    pub config: ConfigFile,
    pub tab: Tab,
    /// The row the editor shows, as an index into the current tab's list.
    pub selected: usize,
    pub profile: profiles::ProfileDraft,
    pub theme: themes::ThemeDraft,
    pub status: Option<String>,
    /// The palette the terminal and this window are showing, while the editor
    /// previews one that is not saved yet.
    pub previewed: Option<Arc<Palette>>,
    /// The terminal window. The spec's own initializer has no terminal to
    /// talk to; the window the terminal opens always has one.
    pub terminal: Option<AppHandle<Model>>,
}

impl SettingsModel {
    pub fn new(terminal: AppHandle<Model>) -> Self {
        Self::with_config(ConfigFile::load(), Some(terminal))
    }

    /// A model over a config the caller holds. The terminal window is absent
    /// when there is none to talk to.
    pub fn with_config(config: ConfigFile, terminal: Option<AppHandle<Model>>) -> Self {
        Self {
            profile: profile_draft(&config),
            theme: theme_draft(&config),
            config,
            tab: Tab::default(),
            selected: 0,
            status: None,
            previewed: None,
            terminal,
        }
    }
}

/// The window's spec. The terminal opens it; it holds the terminal's handle so
/// an applied change reaches the running grid.
pub fn window(terminal: AppHandle<Model>) -> WindowSpec<SettingsModel> {
    let app = rune::App::new(AppSpec {
        min_size: Some(MIN_SIZE),
        on_key: Some(on_key),
        ..AppSpec::new("Settings", SIZE, init, view)
    })
    .on_close(save_on_close);
    WindowSpec::from_app("settings", app)
        .state(SettingsModel::new(terminal))
        .options(WindowOptions {
            visible: false,
            ..WindowOptions::default()
        })
}

/// The window closed: what the editor was showing is written, so the user
/// keeps what they saw.
fn save_on_close(model: &mut SettingsModel, _tx: &mut Tx) {
    let changed = match model.tab {
        Tab::Profiles => profiles::commit(model),
        Tab::Themes => themes::commit(model),
    };
    if changed && write_config(model) {
        model.previewed = None;
        push(model);
    }
}

/// The spec's initializer. The window's state carries the terminal's handle,
/// which the spec cannot build, so the window sets the state instead.
fn init() -> SettingsModel {
    SettingsModel::with_config(ConfigFile::load(), None)
}

/// The palette the window draws with: the one it is previewing, else the one
/// the config holds.
fn previewed_palette(model: &SettingsModel) -> Arc<Palette> {
    match &model.previewed {
        Some(palette) => palette.clone(),
        None => model.config.resolve_palette(),
    }
}

/// The draft for the first profile in the config.
fn profile_draft(config: &ConfigFile) -> profiles::ProfileDraft {
    let name = config
        .profiles
        .keys()
        .next()
        .cloned()
        .unwrap_or_else(|| settings::DEFAULT_PROFILE.to_owned());
    let profile = config.profile(&name).cloned().unwrap_or_default();
    profiles::ProfileDraft::from_profile(&name, &profile)
}

/// The draft for the theme in use, or the first one in the list.
fn theme_draft(config: &ConfigFile) -> themes::ThemeDraft {
    let names = themes::names(config);
    let name = names
        .iter()
        .find(|name| *name == &config.settings.theme)
        .or_else(|| names.first())
        .cloned()
        .unwrap_or_else(|| settings::DEFAULT_THEME.to_owned());
    themes::ThemeDraft::from_colors(&name, config)
}

/// The terminal's chord. Opens the settings window.
pub fn open(model: &mut Model) {
    if let Some(handle) = &model.settings {
        handle.open();
    }
}

/// Keys the window handles itself: its close chord, and the list's arrows. A
/// focused field takes the arrows first.
fn on_key(model: &mut SettingsModel, event: &KeyEvent, tx: &mut Tx) -> bool {
    if matches!(event.key, Key::Command('w')) {
        tx.close_window();
        return true;
    }
    let delta = match event.key {
        Key::Down => 1,
        Key::Up => -1,
        _ => 0,
    };
    if delta == 0 {
        return false;
    }
    let len = list_len(model);
    if len == 0 {
        return true;
    }
    let next = (model.selected as i32 + delta).rem_euclid(len as i32) as usize;
    select(model, next, tx);
    true
}

/// How many rows the current tab's list has.
fn list_len(model: &SettingsModel) -> usize {
    match model.tab {
        Tab::Profiles => model.config.profiles.len(),
        Tab::Themes => themes::names(&model.config).len(),
    }
}

/// The key of the row at `index`, so the focus can follow the selection.
fn row_key(model: &SettingsModel, index: usize) -> String {
    match model.tab {
        Tab::Profiles => format!("profile-{index}"),
        Tab::Themes => format!("theme-{index}"),
    }
}

/// Moves the selection, and the focus with it.
fn select(model: &mut SettingsModel, index: usize, tx: &mut Tx) {
    let len = list_len(model);
    if len == 0 {
        return;
    }
    let index = index.min(len - 1);
    if index == model.selected {
        return;
    }
    model.selected = index;
    model.status = None;
    reload_draft(model);
    tx.focus(row_key(model, index), false);
}

/// Puts the list's row into the editor again, and shows it in the terminal.
fn reload_draft(model: &mut SettingsModel) {
    match model.tab {
        Tab::Profiles => model.profile = profiles::ProfileDraft::from_selected(model),
        Tab::Themes => model.theme = themes::ThemeDraft::from_selected(model),
    }
    preview_selected(model);
}

/// The terminal shows the theme of the row the list has selected.
fn preview_selected(model: &mut SettingsModel) {
    match model.tab {
        Tab::Profiles => profiles::preview_theme(model),
        Tab::Themes => themes::preview_selected(model),
    }
}

/// Shows a tab, with the row its list had.
pub fn show_tab(model: &mut SettingsModel, tab: Tab) {
    model.tab = tab;
    model.selected = match tab {
        Tab::Profiles => 0,
        Tab::Themes => themes::index_of(&model.config, &model.config.settings.theme),
    };
    model.status = None;
    reload_draft(model);
}

pub fn view(model: &SettingsModel, cx: &mut Cx<SettingsModel>) -> El {
    let kit = kit(&previewed_palette(model));
    let mut overlays = Vec::new();
    // The window's own controls (the macOS traffic lights) sit over the
    // content's top-left, so the bar keeps clear of them.
    let insets = cx.chrome_insets();
    let leading = insets.leading.max(cx.titlebar_inset());
    let body = col()
        .w_full()
        .h_full()
        .fill(kit.c.page)
        .child(toolbar(&kit, model, leading, insets.trailing))
        .child(
            row()
                .w_full()
                .grow(1.0)
                .min_h(0.0)
                .child(sidebar(&kit, model))
                .child(el().w(1.0).h_full().fill(kit.c.border_muted))
                .child(editor(&kit, model, &mut overlays)),
        );
    rune_kit::root(body, overlays)
}

fn toolbar(kit: &Kit, model: &SettingsModel, leading: f32, trailing: f32) -> El {
    let selected = match model.tab {
        Tab::Profiles => Some(0),
        Tab::Themes => Some(1),
    };
    let status = model.status.clone();
    let mut bar = row()
        .w_full()
        .h(52.0)
        .pl(leading + 16.0)
        .pr(trailing + 16.0)
        .gap(12.0)
        .center_y()
        .fill(kit.c.bg_base)
        .child(
            text("Settings")
                .font(kit.ty.ui_medium)
                .color(kit.c.text_base),
        )
        .child(
            rune_kit::SegmentedControl::new(&["Profiles", "Themes"], selected)
                .fit()
                .label("Section")
                .on_pick(|m: &mut SettingsModel, index: usize, _tx| {
                    show_tab(
                        m,
                        if index == 1 {
                            Tab::Themes
                        } else {
                            Tab::Profiles
                        },
                    );
                })
                .build(kit),
        )
        .child(el().grow(1.0));
    if let Some(status) = status {
        bar = bar.child(
            text(status)
                .font(kit.ty.small)
                .color(kit.c.danger.fg)
                .shrink(),
        );
    }
    bar
}

fn sidebar(kit: &Kit, model: &SettingsModel) -> El {
    col()
        .w(SIDEBAR_W)
        .h_full()
        .py(8.0)
        .px(8.0)
        .gap(2.0)
        .child(list(kit, model))
        .child(add_row(kit, model))
}

/// The button that adds a row to the list.
fn add_row(kit: &Kit, _model: &SettingsModel) -> El {
    row()
        .key("add-row")
        .w_full()
        .h(30.0)
        .px(8.0)
        .gap(8.0)
        .center_y()
        .radius(kit.radius.md)
        .hover(kit.c.hover)
        .focusable(true)
        .on_press(move |m: &mut SettingsModel| match m.tab {
            Tab::Profiles => profiles::create(m),
            Tab::Themes => themes::create_theme(m),
        })
        .child(text("+").font(kit.ty.ui_medium).color(kit.c.text_muted))
}

fn list(kit: &Kit, model: &SettingsModel) -> El {
    let mut list = col()
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .gap(2.0)
        .role("listbox")
        .scroll(ScrollSpec {
            stick_bottom: false,
        });
    match model.tab {
        Tab::Profiles => {
            for (index, name) in model.config.profiles.keys().enumerate() {
                let default = name == model.config.default_profile_name();
                list = list.child(profile_row(kit, model, index, name, default));
            }
        }
        Tab::Themes => {
            for (index, name) in themes::names(&model.config).iter().enumerate() {
                list = list.child(theme_row(kit, model, index, name));
            }
        }
    }
    list
}

fn profile_row(kit: &Kit, model: &SettingsModel, index: usize, name: &str, default: bool) -> El {
    let selected = model.selected == index;
    let profile = name.to_owned();
    row()
        .key(format!("profile-{index}"))
        .role("option")
        .w_full()
        .h(30.0)
        .px(8.0)
        .gap(8.0)
        .center_y()
        .radius(kit.radius.md)
        .focusable(true)
        .focus_selected(selected)
        .focus_highlight(kit.c.surface_raised_hover)
        .on_press(move |m: &mut SettingsModel| {
            if m.selected != index {
                if profiles::commit(m) {
                    write_config(m);
                }
                m.selected = index;
                m.status = None;
                m.profile = profiles::ProfileDraft::from_selected(m);
                preview_selected(m);
            }
        })
        .child(marker(kit, selected))
        .child(
            text(name.to_owned())
                .font(kit.ty.ui)
                .color(kit.c.text_base)
                .grow(1.0)
                .min_w(0.0)
                .truncate(),
        )
        .child(make_default(kit, profile, default))
}

/// The button that makes a profile the one new sessions use.
fn make_default(kit: &Kit, name: String, default: bool) -> El {
    el().key(format!("profile-default-{name}"))
        .w(22.0)
        .h(22.0)
        .center()
        .radius(4.0)
        .hover(kit.c.hover)
        .on_press(move |m: &mut SettingsModel| profiles::set_default(m, &name))
        .child(icon(kit.glyph.check_small, 16.0).color(if default {
            kit.c.text_accent
        } else {
            kit.c.text_faint
        }))
}

fn theme_row(kit: &Kit, model: &SettingsModel, index: usize, name: &str) -> El {
    let selected = model.selected == index;
    let palette = themes::palette(&model.config, name);
    let author = palette.author.clone();
    let url = palette.url.clone();
    row()
        .key(format!("theme-{index}"))
        .role("option")
        .w_full()
        .h(30.0)
        .px(8.0)
        .gap(8.0)
        .center_y()
        .radius(kit.radius.md)
        .focusable(true)
        .focus_selected(selected)
        .focus_highlight(kit.c.surface_raised_hover)
        .on_press(move |m: &mut SettingsModel| {
            if m.selected != index {
                if themes::commit(m) {
                    write_config(m);
                }
                m.selected = index;
                m.status = None;
                m.theme = themes::ThemeDraft::from_selected(m);
                themes::apply_theme(m);
            }
        })
        .child(marker(kit, selected))
        .child(
            text(name.to_owned())
                .font(kit.ty.ui)
                .color(kit.c.text_base)
                .grow(1.0)
                .min_w(0.0)
                .truncate(),
        )
        .child(
            text(author)
                .font(kit.ty.small)
                .color(kit.c.text_muted)
                .max_w(96.0)
                .shrink()
                .truncate(),
        )
        .child(link(kit, url))
}

/// The button that opens where a theme came from.
pub(crate) fn link(kit: &Kit, url: String) -> El {
    let mut button = el()
        .key(format!("theme-url-{url}"))
        .w(22.0)
        .h(22.0)
        .center()
        .radius(4.0)
        .hover(kit.c.hover)
        .child(text("↗").font(kit.ty.small).color(kit.c.text_muted));
    if !url.is_empty() {
        button = button.on_press(move |_m: &mut SettingsModel| {
            let _ = crate::hyperlink::open_uri(&url);
        });
    }
    button
}

/// The tick on the selected row.
fn marker(kit: &Kit, selected: bool) -> El {
    if selected {
        icon(kit.glyph.check_small, 16.0).color(kit.c.text_accent)
    } else {
        el().w(16.0).h(16.0)
    }
}

/// The editor column.
fn editor(kit: &Kit, model: &SettingsModel, overlays: &mut Vec<El>) -> El {
    match model.tab {
        Tab::Profiles => profiles::editor(kit, model, overlays),
        Tab::Themes => themes::editor(kit, model),
    }
}

/// The kit the window draws with: the terminal's own palette, so the settings
/// window looks like the terminal it configures.
pub fn kit(palette: &Palette) -> Kit {
    let dark = luminance(palette.bg) < 0.5;
    // The neutral tokens the palette does not override (the contrast button,
    // the danger colour) follow the appearance the palette is.
    let mut kit = Kit::neutral(dark);
    let bg = theme::linear(palette.bg);
    let fg = theme::linear(palette.fg);
    let muted = theme::linear(palette.muted);
    let accent = theme::linear(palette.accent);
    let surface = theme::linear(palette.tab_bg);
    let border = theme::linear(palette.active_border);
    kit.dark = dark;
    kit.c.page = bg;
    kit.c.bg_base = bg;
    kit.c.bg_deep = bg;
    kit.c.layer_01 = surface;
    kit.c.layer_02 = surface;
    kit.c.layer_03 = border;
    kit.c.layer_04 = border;
    kit.c.surface_base = surface;
    kit.c.surface_weak = surface;
    kit.c.surface_raised = surface;
    kit.c.surface_raised_base = surface;
    kit.c.surface_float = surface;
    kit.c.text_base = fg;
    kit.c.text_strong = fg;
    kit.c.text_muted = muted;
    kit.c.text_faint = muted;
    kit.c.text_weak = muted;
    kit.c.icon_base = fg;
    kit.c.icon_muted = muted;
    kit.c.icon_faint = muted;
    kit.c.border_muted = border;
    kit.c.border_base = border;
    kit.c.border_strong = border;
    kit.c.border_hover = border;
    kit.c.border_focus = accent;
    kit.c.border_active = accent;
    kit.c.text_accent = accent;
    kit.c.icon_accent = accent;
    kit.c.bg_accent = accent;
    kit.c.hover = fg.alpha(0.08);
    kit.c.pressed = fg.alpha(0.14);
    kit.c.scrim = Color::BLACK.alpha(0.5);
    kit
}

/// The perceived brightness of a color, 0 (black) to 1 (white).
fn luminance(rgb: u32) -> f32 {
    let [r, g, b] = theme::hex(rgb);
    (0.2126 * f32::from(r) + 0.7152 * f32::from(g) + 0.0722 * f32::from(b)) / 255.0
}

/// Writes the config back. `false` when it could not be written.
pub fn write_config(model: &mut SettingsModel) -> bool {
    match model.config.save() {
        Ok(()) => {
            model.status = None;
            true
        }
        Err(err) => {
            model.status = Some(format!("Could not save: {err}"));
            false
        }
    }
}

/// Shows a palette in the terminal and in this window, without saving it.
pub fn preview(model: &mut SettingsModel, palette: Arc<Palette>) {
    model.previewed = Some(palette.clone());
    let Some(terminal) = model.terminal.clone() else {
        return;
    };
    terminal.update(move |model: &mut Model, _tx: &mut Tx| {
        model.palette = palette.clone();
        model.terminal.invalidate_theme();
    });
}

/// Hands the config to the terminal window, with the preview on top of it.
fn push(model: &mut SettingsModel) {
    let Some(terminal) = model.terminal.clone() else {
        return;
    };
    let config = model.config.clone();
    let previewed = model.previewed.clone();
    terminal.update(move |model: &mut Model, tx: &mut Tx| {
        apply(model, &config, tx);
        if let Some(palette) = &previewed {
            model.palette = palette.clone();
            model.terminal.invalidate_theme();
        }
    });
}

/// Puts the terminal back to what the config holds, after a preview.
pub fn restore(model: &mut SettingsModel) {
    model.previewed = None;
    push(model);
}

/// Puts a saved config into the terminal window: the grid's palette, the cell
/// pitch, and the window size that keeps the grid's cell count.
fn apply(model: &mut Model, config: &ConfigFile, tx: &mut Tx) {
    let profile = model
        .tabs
        .get(model.active)
        .map(|tab| tab.profile.clone())
        .unwrap_or_default();
    let resolved = config.resolve_profile(&profile);
    let resized = (resolved.font_size - model.font_size).abs() > f32::EPSILON;
    if resized {
        model.font_size = resolved.font_size;
        model.cell_points = crate::metrics::cell_points_for(resolved.font_size);
    }
    model.palette = config.resolve_palette_for(&profile);
    model.config = config.clone();
    model.terminal.invalidate_theme();
    if resized {
        let effect = model.terminal.primary().map(|host| {
            rune_term_core::resize::apply(rune_term_core::resize::ResizeRequest::Font {
                chrome_points: (0.0, crate::chrome::TITLEBAR_H),
                cols: host.cols,
                rows: host.rows,
                cell_points: model.cell_points,
                scale: model.scale,
            })
        });
        if let (Some(host), Some(effect)) = (model.terminal.primary(), effect) {
            crate::app::perform_resize(tx, host, effect);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rune::{Env, Runtime};

    /// A model over a config of its own, so a test never writes the user's.
    fn model(name: &str) -> SettingsModel {
        let dir = std::env::temp_dir().join("rune-term-settings-ui-test");
        SettingsModel::with_config(
            ConfigFile::default().at(dir.join(format!("{name}.toml"))),
            None,
        )
    }

    fn runtime(model: SettingsModel) -> Runtime<SettingsModel> {
        Runtime::new(model, view, (SIZE.0, SIZE.1), Env::new(true, 1.0))
    }

    #[test]
    fn the_list_shows_the_profiles() {
        let mut rt = runtime(model("list"));
        rt.frame(0.0);
        assert!(rt.find("profile-0").is_some());
        assert!(rt.find("profile-1").is_none());
    }

    #[test]
    fn a_new_profile_joins_the_config_and_the_list() {
        let mut rt = runtime(model("create"));
        rt.frame(0.0);
        press(&mut rt, "add-row");
        assert!(rt.state.config.profiles.contains_key("Profile"));
        assert!(rt.find("profile-1").is_some());
    }

    #[test]
    fn a_deleted_profile_leaves_the_config() {
        let mut rt = runtime(model("delete"));
        rt.frame(0.0);
        press(&mut rt, "add-row");
        press(&mut rt, "profile-delete");
        assert!(!rt.state.config.profiles.contains_key("Profile"));
        assert!(rt.find("profile-1").is_none());
    }

    #[test]
    fn the_themes_tab_lists_the_builtin_themes() {
        let mut rt = runtime(model("themes"));
        show_tab(&mut rt.state, Tab::Themes);
        rt.frame(0.0);
        assert!(rt.find("theme-0").is_some());
        assert!(rt.state.config.custom_themes.is_empty());
    }

    #[test]
    fn a_duplicated_theme_is_a_custom_one() {
        let mut rt = runtime(model("duplicate"));
        show_tab(&mut rt.state, Tab::Themes);
        rt.frame(0.0);
        let source = rt.state.theme.original.clone();
        press(&mut rt, "theme-duplicate");
        assert_eq!(rt.state.config.custom_themes.len(), 1);
        assert!(rt.state.config.is_custom_theme(&format!("{source} Copy")));
    }

    #[test]
    fn a_renamed_theme_keeps_its_colors() {
        let mut rt = runtime(model("rename"));
        show_tab(&mut rt.state, Tab::Themes);
        rt.frame(0.0);
        let source = rt.state.theme.original.clone();
        press(&mut rt, "theme-duplicate");
        let author = rt.state.theme.author.text.clone();
        rt.state.theme.name.text = "Night".to_owned();
        // The field was written, not typed, so the view has not seen it yet.
        rt.invalidate();
        rt.frame(0.0);
        press(&mut rt, "theme-save");
        let theme = rt.state.config.custom_themes.get("Night").expect("renamed");
        assert_eq!(theme.accent, rt.state.theme.colors.accent);
        assert_eq!(theme.author, author);
        assert!(!rt.state.config.is_custom_theme(&format!("{source} Copy")));
    }

    #[test]
    fn the_theme_list_is_sorted() {
        let mut rt = runtime(model("sorted"));
        show_tab(&mut rt.state, Tab::Themes);
        rt.frame(0.0);
        let names = themes::names(&rt.state.config);
        let mut sorted = names.clone();
        sorted.sort_by_key(|name| name.to_lowercase());
        assert_eq!(names, sorted);
    }

    /// Presses a control and lets the frame that follows rebuild the view.
    fn press(rt: &mut Runtime<SettingsModel>, key: &str) {
        let (x, y) = rt.find(key).unwrap_or_else(|| panic!("no {key}"));
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
        rt.frame(0.0);
    }

    #[test]
    fn a_clean_draft_keeps_revert_disabled() {
        let mut rt = runtime(model("clean"));
        rt.frame(0.0);
        assert!(!rt.state.profile.is_dirty());
        assert!(rt.find("profile-revert").is_some());
    }

    #[test]
    fn a_picked_theme_previews_in_the_window_and_the_terminal() {
        let mut rt = runtime(model("preview"));
        rt.frame(0.0);
        let other = themes::names(&rt.state.config)
            .into_iter()
            .find(|name| *name != rt.state.config.settings.theme)
            .expect("another theme");
        rt.state.profile.theme = other.clone();
        profiles::preview_theme(&mut rt.state);
        assert_eq!(
            rt.state.previewed.as_ref().map(|p| p.name.clone()),
            Some(other.clone())
        );
        assert_eq!(previewed_palette(&rt.state).name, other);
    }

    #[test]
    fn reverting_puts_the_draft_and_the_theme_back() {
        let mut rt = runtime(model("revert"));
        rt.frame(0.0);
        let saved = rt.state.config.settings.theme.clone();
        rt.state.profile.theme = "Dracula".to_owned();
        profiles::preview_theme(&mut rt.state);
        assert!(rt.state.profile.is_dirty());
        profiles::revert(&mut rt.state);
        assert!(!rt.state.profile.is_dirty());
        assert!(rt.state.profile.theme.is_empty());
        assert_eq!(rt.state.previewed, None);
        assert_eq!(previewed_palette(&rt.state).name, saved);
    }

    #[test]
    fn saving_a_profile_writes_its_new_name() {
        let mut rt = runtime(model("save"));
        rt.frame(0.0);
        rt.state.profile.name.text = "Renamed".to_owned();
        rt.invalidate();
        rt.frame(0.0);
        profiles::save(&mut rt.state);
        assert!(rt.state.config.profiles.contains_key("Renamed"));
        assert!(!rt.state.config.profiles.contains_key("Default"));
        assert!(!rt.state.profile.is_dirty());
    }

    #[test]
    fn the_arrow_keys_move_the_selection() {
        let mut rt = runtime(model("arrows"));
        rt.frame(0.0);
        press(&mut rt, "add-row");
        assert_eq!(rt.state.selected, 1);
        let down = rune::KeyEvent {
            key: Key::Up,
            mods: rune::Mods::default(),
        };
        assert!(on_key(&mut rt.state, &down, &mut Tx::default()));
        assert_eq!(rt.state.selected, 0);
    }

    #[test]
    fn closing_the_window_saves_the_profile_that_was_edited() {
        let mut rt = runtime(model("close"));
        rt.frame(0.0);
        rt.state.profile.name.text = "Renamed".to_owned();
        rt.invalidate();
        rt.frame(0.0);
        assert!(rt.state.profile.is_dirty());
        save_on_close(&mut rt.state, &mut Tx::default());
        assert!(rt.state.config.profiles.contains_key("Renamed"));
        assert!(!rt.state.profile.is_dirty());
    }

    #[test]
    fn selecting_a_theme_applies_it() {
        let mut rt = runtime(model("themes-preview"));
        show_tab(&mut rt.state, Tab::Themes);
        rt.frame(0.0);
        press(&mut rt, "theme-1");
        assert_eq!(rt.state.config.settings.theme, rt.state.theme.original);
        assert_eq!(
            rt.state
                .previewed
                .as_ref()
                .map(|palette| palette.name.clone()),
            Some(rt.state.theme.original.clone())
        );
    }

    #[test]
    fn a_new_theme_starts_from_the_palette_in_use() {
        let mut rt = runtime(model("themes-new"));
        show_tab(&mut rt.state, Tab::Themes);
        rt.frame(0.0);
        themes::create_theme(&mut rt.state);
        assert_eq!(rt.state.config.custom_themes.len(), 1);
        let name = rt
            .state
            .config
            .custom_themes
            .keys()
            .next()
            .expect("a theme");
        assert_eq!(
            rt.state.config.custom_themes[name].bg,
            rt.state.config.resolve_palette().bg
        );
    }

    #[test]
    fn reverting_a_theme_puts_its_colors_back() {
        let mut rt = runtime(model("themes-revert"));
        show_tab(&mut rt.state, Tab::Themes);
        rt.frame(0.0);
        themes::duplicate_theme(&mut rt.state);
        let before = rt.state.theme.colors;
        rt.state.theme.colors.accent = 0x00FF_00FF;
        rt.invalidate();
        rt.frame(0.0);
        assert!(rt.state.theme.is_dirty());
        themes::revert_theme(&mut rt.state);
        assert!(!rt.state.theme.is_dirty());
        assert_eq!(rt.state.theme.colors, before);
    }

    #[test]
    fn an_applied_config_reaches_the_terminal() {
        let mut model = crate::model::init();
        crate::delegate::setup(&mut model, std::sync::Arc::new(|| {}));
        let mut config = ConfigFile::default();
        config.set_theme("Dracula".to_owned());
        apply(&mut model, &config, &mut Tx::default());
        assert_eq!(model.palette.name, "Dracula");
        assert_eq!(model.config.settings.theme, "Dracula");
    }
}
