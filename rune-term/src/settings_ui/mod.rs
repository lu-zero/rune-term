//! The settings window: a profiles list and a themes list, with CRUD.
//!
//! The window is its own Rune app with its own state. It edits a copy of the
//! config, writes it back, and pushes an applied change into the terminal
//! window, which owns the running grid.

pub mod general;
pub mod profiles;
pub mod themes;

use std::sync::Arc;

use rune::{
    col, el, icon, row, text, AppHandle, AppSpec, Color, Cx, El, Icon, Key, KeyEvent, ScrollSpec,
    Titlebar, Tx, WindowOptions, WindowSpec,
};
use rune_kit::{Kit, TextInput};

use crate::model::Model;
use crate::settings::{self, ConfigFile};
use crate::theme::{self, Palette};

/// The window's size, and the smallest it may be resized to.
pub const SIZE: (f32, f32) = (880.0, 560.0);
pub const MIN_SIZE: (f32, f32) = (620.0, 400.0);

/// The width of the list column.
const SIDEBAR_W: f32 = 300.0;

/// The arrow that points back, for reverting.
pub(crate) static REVERT: Icon = Icon {
    name: "rune-term-revert",
    view: 16.0,
    body: r#"<path d="M9.5 3.5L4.5 8L9.5 12.5" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>"#,
};

/// The arrow that goes down onto a line, for saving.
pub(crate) static SAVE: Icon = Icon {
    name: "rune-term-save",
    view: 16.0,
    body: r#"<path d="M8 2.5V9M4.5 6L8 9.5L11.5 6M3 12.5H13" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>"#,
};

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
        button = button.on_press_tx(move |_m: &mut SettingsModel, tx| {
            if let Err(err) = tx.open_url(&url) {
                eprintln!("not opening {url}: {err}");
            }
        });
    }
    button
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Tab {
    #[default]
    General,
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
        .titlebar(Titlebar::new(crate::chrome::TITLEBAR_H))
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
        Tab::General => false,
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
    themes::ThemeDraft::from_name(&name, config)
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
        Tab::General => 0,
    }
}

/// The key of the row at `index`, so the focus can follow the selection.
fn row_key(model: &SettingsModel, index: usize) -> String {
    match model.tab {
        Tab::Profiles => format!("profile-{index}"),
        Tab::Themes => format!("theme-{index}"),
        Tab::General => String::new(),
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
        Tab::General => {}
    }
    preview_selected(model);
}

/// The terminal shows the theme of the row the list has selected.
fn preview_selected(model: &mut SettingsModel) {
    match model.tab {
        Tab::Profiles => profiles::preview_theme(model),
        Tab::Themes => themes::preview_selected(model),
        Tab::General => restore(model),
    }
}

/// Shows a tab, with the row its list had.
pub fn show_tab(model: &mut SettingsModel, tab: Tab) {
    model.tab = tab;
    model.selected = match tab {
        Tab::Profiles => 0,
        Tab::Themes => themes::index_of(&model.config, &model.config.settings.theme),
        Tab::General => 0,
    };
    model.status = None;
    reload_draft(model);
}

pub fn view(model: &SettingsModel, cx: &mut Cx<SettingsModel>) -> El {
    let kit = kit(&previewed_palette(model));
    let mut overlays = Vec::new();
    let mut main = row().w_full().grow(1.0).min_h(0.0);
    if model.tab != Tab::General {
        main = main
            .child(sidebar(&kit, model))
            .child(el().w(1.0).h_full().fill(kit.c.border_muted));
    }
    let body = col()
        .w_full()
        .h_full()
        .fill(kit.c.page)
        .child(toolbar(&kit, model, cx))
        .child(main.child(editor(&kit, model, &mut overlays, cx)));
    rune_kit::root(body, overlays)
}

fn toolbar(kit: &Kit, model: &SettingsModel, cx: &mut Cx<SettingsModel>) -> El {
    let insets = cx.chrome_insets();
    let band = crate::chrome::band_height(insets.height);
    let tab_h = crate::chrome::tab_box_height(band);
    let leading = insets.leading.max(cx.titlebar_inset());
    let mut bar = crate::chrome::title_strip(band, leading, insets.trailing, kit.c.surface_base)
        .key("titlebar")
        .child(section(
            kit,
            "General",
            "section-general",
            model.tab == Tab::General,
            tab_h,
            Tab::General,
        ))
        .child(section(
            kit,
            "Profiles",
            "section-profiles",
            model.tab == Tab::Profiles,
            tab_h,
            Tab::Profiles,
        ))
        .child(section(
            kit,
            "Themes",
            "section-themes",
            model.tab == Tab::Themes,
            tab_h,
            Tab::Themes,
        ))
        .child(el().grow(1.0));
    if let Some(status) = model.status.clone() {
        bar = bar.child(
            text(status)
                .font(kit.ty.small)
                .color(kit.c.danger.fg)
                .shrink(),
        );
    }
    bar
}

/// One section, drawn as a terminal tab. Pressing the one already showing does
/// nothing, so the rest of the draft is not reloaded.
fn section(kit: &Kit, label: &str, key: &str, on: bool, tab_h: f32, tab: Tab) -> El {
    let ink = if on {
        kit.c.text_base
    } else {
        kit.c.text_muted
    };
    let mut chip = row()
        .key(key)
        .h(tab_h)
        .pl(crate::chrome::TAB_PAD_X)
        .pr(crate::chrome::TAB_PAD_X)
        .center_y()
        .radius(crate::chrome::TAB_RADIUS)
        .on_press(move |model: &mut SettingsModel| {
            if model.tab != tab {
                show_tab(model, tab);
            }
        })
        .child(text(label).color(ink));
    if on {
        chip = chip.fill(kit.c.page).border(1.0, kit.c.border_muted);
    }
    chip
}

fn sidebar(kit: &Kit, model: &SettingsModel) -> El {
    col()
        .w(SIDEBAR_W)
        .h_full()
        .py(8.0)
        .px(8.0)
        .gap(2.0)
        .child(list(kit, model))
        .child(foot(kit))
}

/// The list of rows, with a marker on the one the editor is showing.
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
        Tab::General => {}
    }
    list
}

/// The foot of the list: the button that adds a row.
fn foot(kit: &Kit) -> El {
    row().w_full().h(30.0).center_y().child(add_row(kit))
}

/// The button that adds a row to the list.
fn add_row(kit: &Kit) -> El {
    row()
        .key("add-row")
        .w(26.0)
        .h(26.0)
        .center()
        .radius(kit.radius.sm)
        .hover(kit.c.hover)
        .focusable(true)
        .on_press(move |m: &mut SettingsModel| match m.tab {
            Tab::Profiles => profiles::create(m),
            Tab::Themes => themes::create_theme(m),
            Tab::General => {}
        })
        .child(text("+").font(kit.ty.ui_medium).color(kit.c.text_muted))
}

fn profile_row(kit: &Kit, model: &SettingsModel, index: usize, name: &str, default: bool) -> El {
    let selected = model.selected == index;
    let profile = name.to_owned();
    let row = row()
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
        .child(make_default(kit, profile, default));
    row
}

fn theme_row(kit: &Kit, model: &SettingsModel, index: usize, name: &str) -> El {
    let selected = model.selected == index;
    let row = row()
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
        );
    row
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

/// The tick on the selected row.
fn marker(kit: &Kit, selected: bool) -> El {
    if selected {
        icon(kit.glyph.check_small, 16.0).color(kit.c.text_accent)
    } else {
        el().w(16.0).h(16.0)
    }
}

/// Enter and leaving the field both run `done`. A half-typed value stays put.
pub(crate) fn when_finished(input: TextInput, done: fn(&mut SettingsModel)) -> TextInput {
    input
        .on_submit(move |model: &mut SettingsModel, _| done(model))
        .on_blur(move |model: &mut SettingsModel, _| done(model))
}

/// One editor action. Disabled, it keeps its icon and does not press.
pub(crate) fn action(
    kit: &Kit,
    key: &'static str,
    glyph: &'static Icon,
    label: &str,
    enabled: bool,
    press: impl Fn(&mut SettingsModel) + 'static,
) -> El {
    let mut button = el()
        .key(key)
        .w(24.0)
        .h(24.0)
        .center()
        .radius(kit.radius.sm)
        .help(label)
        .child(icon(glyph, 16.0).color(kit.c.text_base));
    if enabled {
        button = button
            .fill(kit.c.surface_raised)
            .hover(kit.c.hover)
            .on_press(press);
    }
    button
}

/// The editor column, with its actions at the top right.
pub(crate) fn with_actions(actions: Vec<El>, body: El) -> El {
    let mut bar = row()
        .w_full()
        .px(16.0)
        .pt(12.0)
        .gap(4.0)
        .center_y()
        .child(el().grow(1.0));
    for action in actions {
        bar = bar.child(action);
    }
    col()
        .w_full()
        .h_full()
        .grow(1.0)
        .min_h(0.0)
        .child(bar)
        .child(body)
}

/// The editor column.
fn editor(kit: &Kit, model: &SettingsModel, overlays: &mut Vec<El>, cx: &Cx<SettingsModel>) -> El {
    match model.tab {
        Tab::Profiles => profiles::editor(kit, model, overlays, cx),
        Tab::Themes => themes::editor(kit, model, overlays),
        Tab::General => general::editor(kit, model),
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
    let resized = crate::metrics::apply_font(model, &resolved);
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
        let dir = std::env::temp_dir()
            .join("rune-term-settings-ui-test")
            .join(name);
        let _ = std::fs::remove_dir_all(&dir);
        let mut model = SettingsModel::with_config(ConfigFile::default().at_dir(dir), None);
        model.tab = Tab::Profiles;
        model
    }

    fn runtime(model: SettingsModel) -> Runtime<SettingsModel> {
        Runtime::new(model, view, (SIZE.0, SIZE.1), Env::new(true, 1.0))
    }

    #[test]
    fn the_theme_menu_matches_its_field() {
        let mut rt = runtime(model("theme-menu"));
        rt.frame(0.0);
        let trigger = width_of(&rt, "profile-theme-trigger");
        press(&mut rt, "profile-theme-trigger");
        assert!(rt.state.profile.theme_open);
        let menu = width_of(&rt, "profile-theme-content");
        assert!(
            (menu - trigger).abs() < 1.0,
            "menu {menu} trigger {trigger}"
        );
    }

    fn width_of(rt: &Runtime<SettingsModel>, key: &str) -> f32 {
        rt.elements()
            .into_iter()
            .find(|el| el.key.as_deref() == Some(key))
            .unwrap_or_else(|| panic!("missing {key}"))
            .rect[2]
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
        // Let the actions' entering flight settle before the click.
        rt.frame(0.3);
        rt.frame(0.3);
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
    fn an_indexed_color_is_stored_on_its_own() {
        let mut rt = runtime(model("indexed-color"));
        show_tab(&mut rt.state, Tab::Themes);
        rt.frame(0.0);
        press(&mut rt, "theme-duplicate");
        let name = rt.state.theme.original.clone();
        let background = rt.state.config.custom_themes[&name].background;
        let foreground = rt.state.config.custom_themes[&name].foreground;
        rt.state.theme.select_slot(crate::scheme::Slot::Indexed(17));
        let [r, g, b] = crate::theme::xterm_index(17);
        assert_eq!(
            rt.state.theme.hex.text,
            color_picker::hex6(u32::from(r) << 16 | u32::from(g) << 8 | u32::from(b))
        );
        rt.state.theme.select_slot(crate::scheme::Slot::Indexed(16));
        rt.state
            .theme
            .hex
            .replace_text(color_picker::hex6(foreground));
        themes::reflect_color(&mut rt.state);
        let theme = &rt.state.config.custom_themes[&name];
        assert_eq!(theme.color(crate::scheme::Slot::Indexed(16)), foreground);
        assert_eq!(theme.background, background);
        assert_eq!(rt.state.theme.slot, crate::scheme::Slot::Indexed(16));

        rt.state.theme.hex.replace_text(color_picker::hex6(0));
        themes::reflect_color(&mut rt.state);
        assert!(!rt.state.config.custom_themes[&name]
            .indexed
            .contains_key(&16));

        rt.state
            .theme
            .hex
            .replace_text(color_picker::hex6(foreground));
        themes::reflect_color(&mut rt.state);
        themes::remove_indexed(&mut rt.state);
        let theme = &rt.state.config.custom_themes[&name];
        assert!(!theme.indexed.contains_key(&16));
        assert_eq!(theme.background, background);
        assert_eq!(rt.state.theme.slot, crate::scheme::Slot::Background);
        assert!(!rt.state.theme.editor_open);
    }

    #[test]
    fn the_theme_editor_wraps_with_the_window() {
        let mut wide = runtime(model("theme-layout-wide"));
        show_tab(&mut wide.state, Tab::Themes);
        wide.frame(0.0);
        let name = text_top(&wide, "Name");
        let author = text_top(&wide, "Author");
        let source = text_top(&wide, "Source");
        assert!((name - author).abs() < 1.0, "name {name} author {author}");
        assert!(
            (author - source).abs() < 1.0,
            "author {author} source {source}"
        );
        let basic = text_top(&wide, "Basic");
        let ui = text_top(&wide, "UI");
        assert!((basic - ui).abs() < 1.0, "basic {basic} ui {ui}");
        assert!(text_top(&wide, "ANSI") > basic);
        assert!(text_top(&wide, "Cube") > text_top(&wide, "ANSI"));
        assert!(text_top(&wide, "Gray") > text_top(&wide, "Cube"));
        let cube = key_top(&wide, "slot-Index 16");
        assert!((cube - key_top(&wide, "slot-Index 46")).abs() < 1.0);
        assert!(key_top(&wide, "slot-Index 52") > cube);

        let mut narrow = Runtime::new(
            model("theme-layout-narrow"),
            view,
            (MIN_SIZE.0, MIN_SIZE.1),
            Env::new(true, 1.0),
        );
        show_tab(&mut narrow.state, Tab::Themes);
        narrow.frame(0.0);
        assert!(text_top(&narrow, "Author") > text_top(&narrow, "Name"));
        assert!(text_top(&narrow, "Source") > text_top(&narrow, "Author"));
        assert!(text_top(&narrow, "UI") > text_top(&narrow, "Basic"));
        let cube = key_top(&narrow, "slot-Index 16");
        assert!((cube - key_top(&narrow, "slot-Index 28")).abs() < 1.0);
        assert!(key_top(&narrow, "slot-Index 34") > cube);
    }

    fn text_top(rt: &Runtime<SettingsModel>, label: &str) -> f32 {
        rt.elements()
            .into_iter()
            .find(|el| el.text.as_deref() == Some(label))
            .unwrap_or_else(|| panic!("missing {label}"))
            .rect[1]
    }

    fn key_top(rt: &Runtime<SettingsModel>, key: &str) -> f32 {
        rt.elements()
            .into_iter()
            .find(|el| el.key.as_deref() == Some(key))
            .unwrap_or_else(|| panic!("missing {key}"))
            .rect[1]
    }

    #[test]
    fn a_swatch_names_itself_and_opens_its_editor() {
        let mut rt = runtime(model("swatch-tip"));
        show_tab(&mut rt.state, Tab::Themes);
        rt.frame(0.0);
        let help = rt
            .elements()
            .into_iter()
            .find(|el| el.key.as_deref() == Some("slot-Background"))
            .and_then(|el| el.help);
        assert_eq!(help.as_deref(), Some("Background"));
        assert!(rt.find("theme-color-editor-content").is_none());
        assert!(rt.find("slot-Index 16").is_some());
        assert!(rt.find("slot-Index 232").is_some());
        assert!(rt.find("slot-Index 255").is_some());
        assert!(rt.find("indexed-add").is_none());
        let index_help = rt
            .elements()
            .into_iter()
            .find(|el| el.key.as_deref() == Some("slot-Index 16"))
            .and_then(|el| el.help);
        assert_eq!(index_help.as_deref(), Some("Index 16"));
        press(&mut rt, "slot-Blue");
        assert!(rt.find("theme-color-editor-content").is_none());

        press(&mut rt, "theme-duplicate");
        press(&mut rt, "slot-Blue");
        assert!(rt.find("theme-color-editor-content").is_some());
        assert_eq!(rt.state.theme.slot, crate::scheme::Slot::Ansi(4));
        assert!(rt.state.theme.editor_open);
        let title = rt
            .elements()
            .into_iter()
            .find(|el| el.key.as_deref() == Some("title"))
            .and_then(|el| el.text);
        assert_eq!(title.as_deref(), Some("Blue"));

        // The scrim fades in over the grid, and fades out after it closes.
        rt.frame(0.5);
        press(&mut rt, "slot-Blue");
        assert!(rt.find("theme-color-editor-content").is_none());
        assert!(!rt.state.theme.editor_open);
        rt.frame(0.5);

        press(&mut rt, "slot-Blue");
        rt.frame(0.5);
        press(&mut rt, "slot-Red");
        assert!(rt.find("theme-color-editor-content").is_none());
        assert_eq!(rt.state.theme.slot, crate::scheme::Slot::Ansi(4));
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
        assert_eq!(theme.ansi, rt.state.theme.scheme.ansi);
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
    fn the_editor_actions_name_themselves() {
        let mut rt = runtime(model("action-tips"));
        rt.frame(0.0);
        assert_eq!(help_text(&rt, "profile-save").as_deref(), Some("Save"));
        assert_eq!(help_text(&rt, "profile-revert").as_deref(), Some("Revert"));
        assert_eq!(
            help_text(&rt, "profile-duplicate").as_deref(),
            Some("Duplicate")
        );
        assert_eq!(help_text(&rt, "profile-delete").as_deref(), Some("Delete"));
        press(&mut rt, "section-themes");
        assert_eq!(help_text(&rt, "theme-save").as_deref(), Some("Save"));
        assert_eq!(help_text(&rt, "theme-revert").as_deref(), Some("Revert"));
        assert_eq!(
            help_text(&rt, "theme-duplicate").as_deref(),
            Some("Duplicate")
        );
        assert_eq!(help_text(&rt, "theme-delete").as_deref(), Some("Delete"));
    }

    fn help_text(rt: &Runtime<SettingsModel>, key: &str) -> Option<String> {
        rt.elements()
            .into_iter()
            .find(|el| el.key.as_deref() == Some(key))
            .and_then(|el| el.help)
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
    fn the_profile_name_lands_in_the_list_when_the_field_is_done() {
        let mut rt = runtime(model("name-live"));
        rt.frame(0.0);
        let (x, y) = rt.find("input").expect("name field");
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
        rt.frame(0.0);
        rt.state.profile.name.text = "Work".to_owned();
        rt.key(rune::KeyEvent {
            key: Key::Enter,
            mods: rune::Mods::default(),
        });
        rt.frame(0.0);
        assert!(rt.state.config.profiles.contains_key("Work"));
        assert!(!rt.state.config.profiles.contains_key("Default"));
        assert_eq!(rt.state.profile.original, "Work");
        assert_eq!(rt.state.selected, 0);

        rt.state.profile.name.text = "Home".to_owned();
        rt.physical_input(rune::PhysicalInput::FocusLost);
        rt.frame(0.0);
        assert!(rt.state.config.profiles.contains_key("Home"));
        assert!(!rt.state.config.profiles.contains_key("Work"));
        assert_eq!(rt.state.profile.original, "Home");

        rt.state.profile.name.text = "Lab".to_owned();
        rt.key(rune::KeyEvent {
            key: Key::Tab,
            mods: rune::Mods::default(),
        });
        rt.frame(0.0);
        assert!(rt.state.config.profiles.contains_key("Lab"));
        assert!(!rt.state.config.profiles.contains_key("Home"));

        rt.state.profile.name.text = "  ".to_owned();
        profiles::reflect_name(&mut rt.state);
        assert_eq!(rt.state.status.as_deref(), Some("A profile needs a name"));
        assert!(rt.state.config.profiles.contains_key("Lab"));

        rt.state.profile.name.text = "Lab".to_owned();
        profiles::reflect_name(&mut rt.state);
        assert_eq!(rt.state.status, None);
        assert!(rt.state.config.profiles.contains_key("Lab"));
    }

    #[test]
    fn a_finished_profile_field_is_stored_without_the_rest_of_the_draft() {
        let mut rt = runtime(model("fields"));
        rt.frame(0.0);
        let (x, y) = rt.find("input").expect("name field");
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
        rt.frame(0.0);
        // Name, then the theme menu, the font family, and the font size.
        for _ in 0..3 {
            rt.key(rune::KeyEvent {
                key: Key::Tab,
                mods: rune::Mods::default(),
            });
            rt.frame(0.0);
        }
        rt.state.profile.font_size.text = "16".to_owned();
        rt.state.profile.shell_program.text = "zsh".to_owned();
        rt.key(rune::KeyEvent {
            key: Key::Enter,
            mods: rune::Mods::default(),
        });
        rt.frame(0.0);
        let profile = rt.state.config.profiles.get("Default").expect("profile");
        assert_eq!(profile.font_size, Some(16.0));
        assert!(profile.shell.is_none());
        assert_eq!(rt.state.profile.shell_program.text, "zsh");
        assert!(rt.state.profile.is_dirty());

        rt.state.profile.font_size.text = "nope".to_owned();
        profiles::reflect_font_size(&mut rt.state);
        assert_eq!(
            rt.state.status.as_deref(),
            Some("'nope' is not a font size")
        );
        assert_eq!(rt.state.config.profiles["Default"].font_size, Some(16.0));

        rt.state.profile.cwd.text = "  /tmp ".to_owned();
        profiles::reflect_cwd(&mut rt.state);
        assert_eq!(
            rt.state.config.profiles["Default"].cwd.as_deref(),
            Some("/tmp")
        );
        assert_eq!(rt.state.profile.cwd.text, "/tmp");
        assert_eq!(rt.state.profile.shell_program.text, "zsh");
    }

    #[test]
    fn a_finished_theme_field_is_stored_without_the_rest_of_the_draft() {
        let mut rt = runtime(model("theme-fields"));
        show_tab(&mut rt.state, Tab::Themes);
        rt.frame(0.0);
        press(&mut rt, "theme-duplicate");
        let author = rt.state.theme.author.text.clone();
        rt.state.theme.select_slot(crate::scheme::Slot::Ansi(15));
        rt.state.theme.hex.text = "#112233".to_owned();
        rt.state.theme.author.text = "Ada".to_owned();
        themes::reflect_color(&mut rt.state);
        let name = rt.state.theme.original.clone();
        let theme = rt.state.config.custom_themes.get(&name).expect("theme");
        assert_eq!(theme.color(crate::scheme::Slot::Ansi(15)), 0x112233);
        assert_eq!(theme.background, 0);
        assert_eq!(theme.author, author);
        assert_eq!(rt.state.theme.author.text, "Ada");

        rt.state.theme.name.text = "Night".to_owned();
        themes::reflect_name(&mut rt.state);
        assert!(rt.state.config.custom_themes.contains_key("Night"));
        assert!(!rt.state.config.custom_themes.contains_key(&name));
        assert_eq!(
            rt.state.config.custom_themes["Night"].color(crate::scheme::Slot::Ansi(15)),
            0x112233
        );
        assert_eq!(rt.state.config.custom_themes["Night"].author, author);
        assert_eq!(rt.state.theme.author.text, "Ada");

        rt.state.theme.name.text = "  ".to_owned();
        themes::reflect_name(&mut rt.state);
        assert_eq!(rt.state.status.as_deref(), Some("A theme needs a name"));
        assert!(rt.state.config.custom_themes.contains_key("Night"));
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
    fn saving_a_session_profile_keeps_the_session_it_attaches() {
        let dir = std::env::temp_dir()
            .join("rune-term-settings-ui-test")
            .join("session-profile");
        let _ = std::fs::remove_dir_all(&dir);
        let mut config = ConfigFile::default().at_dir(dir);
        let profile = config.ensure_profile("Default");
        profile.backend = "tmux".to_owned();
        profile.host = Some("box".to_owned());
        profile.session = Some("work".to_owned());
        let mut rt = runtime(SettingsModel::with_config(config, None));
        rt.frame(0.0);
        rt.state.profile.font_size.text = "15".to_owned();
        profiles::save(&mut rt.state);
        let saved = rt.state.config.profile("Default").unwrap();
        assert_eq!(saved.font_size, Some(15.0));
        assert_eq!(saved.backend, "tmux");
        assert_eq!(saved.host.as_deref(), Some("box"));
        assert_eq!(saved.session.as_deref(), Some("work"));
    }

    fn shows(rt: &Runtime<SettingsModel>, label: &str) -> bool {
        rt.elements()
            .into_iter()
            .any(|el| el.text.as_deref() == Some(label))
    }

    #[test]
    fn a_session_backend_swaps_the_shell_fields_for_its_host() {
        let mut rt = runtime(model("backend"));
        rt.frame(0.0);
        assert!(shows(&rt, "Shell program") && !shows(&rt, "Host"));
        press(&mut rt, "profile-backend-trigger");
        press(&mut rt, "option2");
        assert_eq!(rt.state.config.profile("Default").unwrap().backend, "rmux");
        assert!(shows(&rt, "Host") && shows(&rt, "Session") && !shows(&rt, "Shell program"));
        rt.state.profile.host.text = " thalia ".to_owned();
        profiles::save(&mut rt.state);
        let saved = rt.state.config.profile("Default").unwrap();
        assert_eq!(saved.host.as_deref(), Some("thalia"));
        assert_eq!(saved.session, None);
        assert!(shows(&rt, "On close"));
        assert_eq!(saved.on_close, None);
        press(&mut rt, "segment2");
        let dir = rt.state.config.dir();
        let written = ConfigFile::open(&dir).unwrap();
        assert_eq!(
            written.profile("Default").unwrap().on_close,
            Some(crate::settings::OnClose::Detach)
        );
    }

    #[test]
    fn the_general_tab_saves_what_the_app_does_and_a_profile_overrides_it() {
        use crate::settings::OnClose;
        let mut rt = runtime(model("general"));
        rt.frame(0.0);
        press(&mut rt, "section-general");
        assert_eq!(rt.state.tab, Tab::General);
        assert!(rt.find("add-row").is_none(), "no list on this tab");
        // The page slides in; it takes input where it lands.
        rt.frame(2.0);
        press(&mut rt, "confirm-close");
        press(&mut rt, "segment1");
        let written = ConfigFile::open(&rt.state.config.dir()).unwrap();
        assert!(!written.settings.confirm_close);
        assert_eq!(written.settings.on_close, OnClose::Detach);
        assert_eq!(
            written.on_close("Default"),
            OnClose::Detach,
            "a profile without its own follows"
        );
        let mut config = written;
        config.profiles.get_mut("Default").unwrap().on_close = Some(OnClose::Kill);
        assert_eq!(
            config.on_close("Default"),
            OnClose::Kill,
            "a profile's own wins"
        );
        assert_eq!(config.on_close("missing"), OnClose::Detach);
    }

    #[test]
    fn a_profile_the_terminal_saves_survives_the_next_write_here() {
        let mut rt = runtime(model("terminal-saved"));
        rt.frame(0.0);
        profiles::create(&mut rt.state);
        let selected = rt.state.selected;
        let shown = rt.state.config.profiles.keys().nth(selected).cloned();
        let dir = rt.state.config.dir();
        let mut terminal = ConfigFile::open(&dir).unwrap();
        terminal.ensure_profile("A first");
        terminal.save().unwrap();
        profiles::add(
            &mut rt.state,
            "A first".to_owned(),
            crate::settings::Profile::default(),
        );
        assert_eq!(
            rt.state
                .config
                .profiles
                .keys()
                .nth(rt.state.selected)
                .cloned(),
            shown
        );
        rt.state.profile.font_size.text = "16".to_owned();
        profiles::save(&mut rt.state);
        assert!(ConfigFile::open(&dir).unwrap().profile("A first").is_some());
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
            rt.state.config.custom_themes[name].background,
            rt.state.config.resolve_palette().bg
        );
    }

    #[test]
    fn a_typed_hex_recolors_the_slot_and_a_slider_rewrites_the_field() {
        let mut rt = runtime(model("hex"));
        show_tab(&mut rt.state, Tab::Themes);
        themes::duplicate_theme(&mut rt.state);
        rt.state.theme.select_slot(crate::scheme::Slot::Ansi(4));
        rt.state.theme.hex.text = "00ff00".to_owned();
        themes::apply_typed_color(&mut rt.state);
        assert_eq!(
            rt.state.theme.scheme.color(crate::scheme::Slot::Ansi(4)),
            0x00FF00
        );
        // A fragment is still being typed, so the color stays.
        rt.state.theme.hex.text = "00ff".to_owned();
        themes::apply_typed_color(&mut rt.state);
        assert_eq!(
            rt.state.theme.scheme.color(crate::scheme::Slot::Ansi(4)),
            0x00FF00
        );
        assert!(!themes::hex_unusable("00ff"));
        assert!(themes::hex_unusable("zz"));
        rt.state.theme.set_slot_color(0x11_22_33);
        assert_eq!(
            rt.state.theme.scheme.color(crate::scheme::Slot::Ansi(4)),
            0x112233
        );
        assert_eq!(rt.state.theme.hex.text, "#112233");
    }

    #[test]
    fn reverting_a_theme_puts_its_colors_back() {
        let mut rt = runtime(model("themes-revert"));
        show_tab(&mut rt.state, Tab::Themes);
        rt.frame(0.0);
        themes::duplicate_theme(&mut rt.state);
        let before = rt.state.theme.scheme.clone();
        rt.state
            .theme
            .scheme
            .set_color(crate::scheme::Slot::Ansi(4), 0x00FF_00FF);
        rt.invalidate();
        rt.frame(0.0);
        assert!(rt.state.theme.is_dirty());
        themes::revert_theme(&mut rt.state);
        assert!(!rt.state.theme.is_dirty());
        assert_eq!(rt.state.theme.scheme, before);
    }

    #[test]
    fn the_section_bar_matches_the_terminal_titlebar() {
        let mut rt = runtime(model("titlebar"));
        rt.frame(0.0);
        let bar = rt.presented("titlebar").expect("titlebar").0;
        assert!(
            (bar[3] - crate::chrome::TITLEBAR_H).abs() < 0.5,
            "band {}",
            bar[3]
        );
        let chip = rt.presented("section-profiles").expect("profiles").0;
        let tab_h = crate::chrome::tab_box_height(crate::chrome::TITLEBAR_H);
        assert!((chip[3] - tab_h).abs() < 0.5, "chip {}", chip[3]);
        rt.state.profile.name.text = "Work".to_owned();
        press(&mut rt, "section-profiles");
        assert_eq!(rt.state.profile.name.text, "Work");
        assert!(rt.state.config.profiles.contains_key("Default"));
        press(&mut rt, "section-themes");
        assert_eq!(rt.state.tab, Tab::Themes);
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
