//! One tab strip. The panes are [`crate::chrome_view`]. The cell tree is [`crate::split`].

use rune::tok::{radius, space};
use rune::{col, el, row, text, text_input, Color, Cx, El, ScrollSpec, TextStyle};

use crate::chrome_view;
use crate::model::Model;
use crate::theme::{self, Palette};

pub const PANE_MARK: u32 = 1;
pub const LEAF: LeafId = LeafId(1);

pub use crate::split::{Leaf, LeafId, Node};

/// The window titlebar height, and the strip that lives in it.
pub const TITLEBAR_H: f32 = 38.0;
/// One hit target. A short title and a long one use this same box.
const TAB_W: f32 = 132.0;
const TAB_PAD_X: f32 = 14.0;
const TAB_RADIUS: f32 = 7.0;

/// Tab strip. Dark chrome above the black pane.
pub fn tab(p: &Palette) -> rune::Color {
    theme::linear(p.tab_bg)
}

/// Cell background. Transparent cells show this.
pub fn slot(p: &Palette) -> rune::Color {
    theme::linear(p.bg)
}

pub fn text_color(p: &Palette) -> rune::Color {
    theme::linear(p.fg)
}

pub fn slot_premul(bg: u32, srgb_blend: bool) -> [f32; 4] {
    let slot = theme::linear(bg);
    if srgb_blend {
        slot.premul_srgb(1.0)
    } else {
        slot.premul(1.0)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tab {
    pub title: String,
    pub profile: String,
    pub root: Node,
    pub focused: LeafId,
}

pub fn view(model: &Model, cx: &mut Cx<Model>) -> El {
    let p = model.palette.as_ref();
    let tab = tab_strip(model, cx);
    let pane = chrome_view::area(model, cx);
    // Rune delivers Cmd+V to the root handler, not to the focused pane.
    let mut root = col()
        .w_full()
        .h_full()
        .fill(theme::linear(p.tab_bg))
        .on_paste(crate::selection::on_paste)
        .child(tab)
        .child(pane);
    if let Some(uri) = &model.pending_link_open {
        root = root.child(confirm_open_link(uri, p));
    }
    if model.settings_mode.is_open() {
        root = root.child(settings_panel(model));
    }
    root
}

fn close_settings(model: &mut Model, tx: &mut rune::Tx) {
    model.settings_mode.close();
    model.palette = model.config.resolve_palette();
    model.grid_draw.invalidate_theme();
    crate::tabs::focus_active(model, tx);
}

fn settings_panel(model: &Model) -> El {
    let p = model.palette.as_ref();
    let active_tab = match &model.settings_mode {
        crate::model::SettingsMode::Open(ui) => ui.tab,
        crate::model::SettingsMode::Closed => crate::model::SettingsTab::Font,
    };
    let header = row()
        .w_full()
        .items_center()
        .justify_between()
        .child(text("Settings").color(text_color(p)))
        .child(settings_chip("Close", "close-settings", p, |model, tx| {
            close_settings(model, tx);
        }));
    let tabs = row()
        .w_full()
        .gap(space::S2)
        .child(settings_tab_chip(
            "Font",
            crate::model::SettingsTab::Font,
            active_tab,
            p,
        ))
        .child(settings_tab_chip(
            "Theme",
            crate::model::SettingsTab::Theme,
            active_tab,
            p,
        ))
        .child(settings_tab_chip(
            "Profiles",
            crate::model::SettingsTab::Profiles,
            active_tab,
            p,
        ));
    let body = match active_tab {
        crate::model::SettingsTab::Font => font_settings(model, p),
        crate::model::SettingsTab::Theme => theme_settings(model, p),
        crate::model::SettingsTab::Profiles => profiles_settings(model, p),
    };
    let body = col()
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .scroll(ScrollSpec {
            stick_bottom: false,
        })
        .child(body);
    let card = col()
        .key("settings")
        .w(560.0)
        .h_full()
        .fill(theme::linear(p.tab_bg))
        .border(1.0, theme::linear(p.active_border))
        .radius(radius::LG)
        .gap(space::S3)
        .pl(space::S6)
        .pr(space::S6)
        .pt(space::S3)
        .pb(space::S3)
        .modal_tx(true, |model: &mut Model, tx: &mut rune::Tx| {
            close_settings(model, tx);
        })
        .child(header)
        .child(tabs)
        .child(body);
    rune::el()
        .key("settings-overlay")
        .absolute(0.0, 0.0)
        .w_full()
        .h_full()
        .items_center()
        .justify_center()
        .pl(20.0)
        .pr(20.0)
        .pt(20.0)
        .pb(20.0)
        .child(card)
}

fn settings_tab_chip(
    label: &str,
    tab: crate::model::SettingsTab,
    active: crate::model::SettingsTab,
    p: &Palette,
) -> El {
    let selected = tab == active;
    el().key(format!("settings-tab-{label}"))
        .pl(space::S3)
        .pr(space::S3)
        .pt(space::S1)
        .pb(space::S1)
        .radius(radius::MD)
        .border(
            if selected { 1.5 } else { 1.0 },
            theme::linear(if selected { p.accent } else { p.active_border }),
        )
        .fill(theme::linear(if selected { p.bg } else { p.tab_bg }))
        .focusable(true)
        .on_press_tx(move |model: &mut Model, _tx| {
            if let crate::model::SettingsMode::Open(ui) = &mut model.settings_mode {
                ui.tab = tab;
            }
        })
        .child(text(label).color(if selected {
            text_color(p)
        } else {
            theme::linear(p.muted)
        }))
}

fn settings_card(p: &Palette, content: El) -> El {
    col()
        .w_full()
        .padding(space::S3)
        .gap(space::S2)
        .fill(theme::linear(p.bg))
        .radius(radius::LG)
        .border(1.0, theme::linear(p.active_border))
        .child(content)
}

fn font_settings(model: &Model, p: &Palette) -> El {
    let size = model.font_size;
    let resolved = model.config.resolve();
    let family = if resolved.font_family.trim().is_empty() {
        "Cascadia Code NF".to_owned()
    } else {
        resolved.font_family
    };
    let picker_open = matches!(
        &model.settings_mode,
        crate::model::SettingsMode::Open(ui) if ui.font_family_picker_open
    );

    let family_entry = {
        let family_label = family.clone();
        let before = family.clone();
        row()
            .key("font-family-entry")
            .grow(1.0)
            .h(36.0)
            .items_center()
            .justify_between()
            .pl(10.0)
            .pr(10.0)
            .radius(radius::MD)
            .fill(theme::linear(p.bg))
            .border(1.0, theme::linear(p.active_border))
            .hover(hover_overlay(p))
            .on_press_tx(move |model: &mut Model, _tx| {
                if let crate::model::SettingsMode::Open(ui) = &mut model.settings_mode {
                    if ui.font_family_picker_open {
                        ui.font_family_picker_open = false;
                    } else {
                        ui.font_family_before_pick = Some(before.clone());
                        ui.font_family_picker_open = true;
                    }
                }
            })
            .child(
                text(format!("{size:.0} pt · {family_label}"))
                    .color(text_color(p))
                    .truncate(),
            )
            .child(text(if picker_open { "▴" } else { "▾" }).color(theme::linear(p.muted)))
    };

    let mut card_content = col()
        .w_full()
        .gap(space::S3)
        .child(
            row()
                .w_full()
                .h(44.0)
                .gap(space::S2)
                .items_center()
                .child(settings_step("−", "font-dec", p, -1.0))
                .child(family_entry)
                .child(settings_step("+", "font-inc", p, 1.0)),
        )
        .child(
            row()
                .w_full()
                .gap(space::S2)
                .child(font_preset(11.0, p))
                .child(font_preset(12.0, p))
                .child(font_preset(14.0, p))
                .child(font_preset(16.0, p))
                .child(font_preset(18.0, p))
                .child(font_preset(20.0, p)),
        );

    if picker_open {
        let catalog = font_picker::FontCatalog::system();
        let can_revert = matches!(
            &model.settings_mode,
            crate::model::SettingsMode::Open(ui)
                if ui.font_family_before_pick.as_deref().is_some_and(|b| b != family)
        );
        card_content = card_content.child(
            col()
                .w_full()
                .gap(6.0)
                .child(
                    row()
                        .w_full()
                        .gap(8.0)
                        .items_center()
                        .child(text("Pick a font (↑/↓ to navigate)").color(theme::linear(p.muted)))
                        .child(el().grow(1.0))
                        .child(if can_revert {
                            settings_chip("Revert", "font-family-revert", p, |model, _tx| {
                                let prev = match &model.settings_mode {
                                    crate::model::SettingsMode::Open(ui) => {
                                        ui.font_family_before_pick.clone()
                                    }
                                    _ => None,
                                };
                                if let Some(prev) = prev {
                                    crate::app::apply_font_family(
                                        model,
                                        &mut rune::Tx::default(),
                                        &prev,
                                    );
                                }
                                if let crate::model::SettingsMode::Open(ui) =
                                    &mut model.settings_mode
                                {
                                    ui.font_family_picker_open = false;
                                    ui.font_family_before_pick = None;
                                }
                            })
                        } else {
                            el()
                        }),
                )
                .child(
                    el().w_full()
                        .h(220.0)
                        .radius(6.0)
                        .border(1.0, theme::linear(p.active_border))
                        .fill(theme::linear(p.bg))
                        .child(font_picker::family_list::<Model, _>(
                            &catalog.families,
                            &family,
                            |model: &mut Model, selected: &str| {
                                crate::app::apply_font_family(
                                    model,
                                    &mut rune::Tx::default(),
                                    selected,
                                );
                                if let crate::model::SettingsMode::Open(ui) =
                                    &mut model.settings_mode
                                {
                                    ui.font_family_picker_open = false;
                                    ui.font_family_before_pick = None;
                                }
                            },
                        )),
                ),
        );
    }

    let card = settings_card(p, card_content);
    col()
        .w_full()
        .grow(1.0)
        .gap(space::S3)
        .pb(space::S2)
        .child(card)
}

fn theme_settings(model: &Model, p: &Palette) -> El {
    if matches!(
        &model.settings_mode,
        crate::model::SettingsMode::Open(ui) if ui.theme_editor.is_some()
    ) {
        return theme_editor_view(model);
    }
    let mut themes = col().w_full().gap(4.0);
    let mut customs: Vec<_> = model.config.custom_themes.iter().collect();
    customs.sort_by_key(|x| std::cmp::Reverse(x.1.created_at));
    for (name, custom) in customs {
        let tp = custom.to_arc_palette(name);
        themes = themes.child(theme_choice(&tp, p));
    }
    for t in theme::builtins() {
        themes = themes.child(theme_choice(t, p));
    }
    let themes = themes.pb(4.0);
    let theme_list = col()
        .w_full()
        .grow(1.0)
        .min_h(220.0)
        .scroll(ScrollSpec {
            stick_bottom: false,
        })
        .radius(radius::MD)
        .fill(crate::theme::linear(p.bg))
        .border(1.0, crate::theme::linear(p.active_border))
        .pt(space::S2)
        .pb(space::S1)
        .pl(space::S2)
        .pr(space::S2)
        .child(themes);
    col()
        .w_full()
        .grow(1.0)
        .gap(space::S3)
        .pb(space::S2)
        .child(theme_list)
}

fn theme_editor_view(model: &Model) -> El {
    let p = model.palette.as_ref();
    let editor = match &model.settings_mode {
        crate::model::SettingsMode::Open(ui) => match ui.theme_editor.as_ref() {
            Some(editor) => editor,
            None => return col(),
        },
        _ => return col(),
    };
    let mut grid = col().w_full().gap(6.0);
    #[allow(clippy::type_complexity)]
    let text_rows: [(
        &str,
        &str,
        &rune::TextEdit,
        fn(&mut Model) -> &mut rune::TextEdit,
    ); 3] = [
        ("Name", "te-name", &editor.name, |m| {
            match &mut m.settings_mode {
                crate::model::SettingsMode::Open(ui) => &mut ui.theme_editor.as_mut().unwrap().name,
                _ => unreachable!(),
            }
        }),
        ("Author", "te-author", &editor.author, |m| {
            match &mut m.settings_mode {
                crate::model::SettingsMode::Open(ui) => {
                    &mut ui.theme_editor.as_mut().unwrap().author
                }
                _ => unreachable!(),
            }
        }),
        ("URL", "te-url", &editor.url, |m| {
            match &mut m.settings_mode {
                crate::model::SettingsMode::Open(ui) => &mut ui.theme_editor.as_mut().unwrap().url,
                _ => unreachable!(),
            }
        }),
    ];
    for (label, key, te, on_edit) in text_rows {
        grid = grid.child(editor_text_row(label, key, te, p, on_edit));
    }
    let color_rows = [
        ("Background", "te-bg", crate::model::ThemeColorField::Bg),
        ("Foreground", "te-fg", crate::model::ThemeColorField::Fg),
        (
            "Bright FG",
            "te-fgb",
            crate::model::ThemeColorField::FgBright,
        ),
        ("Muted", "te-muted", crate::model::ThemeColorField::Muted),
        ("Tab BG", "te-tab", crate::model::ThemeColorField::TabBg),
        (
            "Border",
            "te-border",
            crate::model::ThemeColorField::ActiveBorder,
        ),
        ("Accent", "te-accent", crate::model::ThemeColorField::Accent),
    ];
    for (label, key, field) in color_rows {
        grid = grid.child(editor_color_row(label, key, field, editor, p));
    }
    let picker = if let Some(field) = editor.picking {
        let rgb = crate::model::parse_hex(&editor.field_edit(field).text).unwrap_or(0);
        let hsv = color_picker::Hsv::from_rgb_u32(rgb);
        theme_color_picker_panel(field, hsv, p)
    } else {
        col().w_full()
    };
    col()
        .w_full()
        .grow(1.0)
        .gap(10.0)
        .child(
            row()
                .w_full()
                .items_center()
                .justify_between()
                .child(text("Edit custom theme").color(text_color(p)))
                .child(text(&editor.original_name).color(theme::linear(p.muted))),
        )
        .child(
            col()
                .w_full()
                .grow(1.0)
                .scroll(ScrollSpec {
                    stick_bottom: false,
                })
                .gap(6.0)
                .child(grid)
                .child(picker),
        )
        .child(
            row()
                .w_full()
                .gap(8.0)
                .justify_center()
                .child(settings_chip("Cancel", "te-cancel", p, |model, tx| {
                    crate::app::cancel_theme_edit(model, tx);
                }))
                .child(settings_chip("Save", "te-save", p, |model, tx| {
                    crate::app::save_theme_edit(model, tx);
                })),
        )
}

fn theme_color_picker_panel(
    field: crate::model::ThemeColorField,
    hsv: color_picker::Hsv,
    p: &Palette,
) -> El {
    let rgb = hsv.to_rgb_u32();
    col()
        .w_full()
        .gap(6.0)
        .pt(8.0)
        .pb(4.0)
        .child(
            row()
                .w_full()
                .gap(8.0)
                .items_center()
                .child(text("Color picker").color(theme::linear(p.muted)))
                .child(el().grow(1.0))
                .child(
                    el().w(24.0)
                        .h(24.0)
                        .radius(4.0)
                        .fill(color_picker::linear_rgb(rgb))
                        .border(1.0, theme::linear(p.active_border)),
                )
                .child(text(color_picker::hex6(rgb)).color(text_color(p))),
        )
        .child(color_picker::channel_slider::<Model, _>(
            &format!("cp-{field:?}-h"),
            color_picker::Channel::Hue,
            hsv,
            move |model: &mut Model, hsv| {
                let rgb = hsv.to_rgb_u32();
                if let crate::model::SettingsMode::Open(ui) = &mut model.settings_mode {
                    if let Some(editor) = ui.theme_editor.as_mut() {
                        editor.set_field_hex(field, rgb);
                    }
                }
                // preview will run on next interaction? call helper-like inline
                if let crate::model::SettingsMode::Open(ui) = &model.settings_mode {
                    if let Some(editor) = ui.theme_editor.as_ref() {
                        let fallback = model
                            .config
                            .custom_themes
                            .get(&editor.original_name)
                            .cloned()
                            .unwrap_or_else(|| {
                                crate::settings::CustomTheme::from_palette(&model.palette)
                            });
                        model.palette = editor.preview_palette(&fallback);
                        model.grid_draw.invalidate_theme();
                    }
                }
            },
        ))
        .child(color_picker::channel_slider::<Model, _>(
            &format!("cp-{field:?}-s"),
            color_picker::Channel::Saturation,
            hsv,
            move |model: &mut Model, hsv| {
                let rgb = hsv.to_rgb_u32();
                if let crate::model::SettingsMode::Open(ui) = &mut model.settings_mode {
                    if let Some(editor) = ui.theme_editor.as_mut() {
                        editor.set_field_hex(field, rgb);
                    }
                }
                if let crate::model::SettingsMode::Open(ui) = &model.settings_mode {
                    if let Some(editor) = ui.theme_editor.as_ref() {
                        let fallback = model
                            .config
                            .custom_themes
                            .get(&editor.original_name)
                            .cloned()
                            .unwrap_or_else(|| {
                                crate::settings::CustomTheme::from_palette(&model.palette)
                            });
                        model.palette = editor.preview_palette(&fallback);
                        model.grid_draw.invalidate_theme();
                    }
                }
            },
        ))
        .child(color_picker::channel_slider::<Model, _>(
            &format!("cp-{field:?}-v"),
            color_picker::Channel::Value,
            hsv,
            move |model: &mut Model, hsv| {
                let rgb = hsv.to_rgb_u32();
                if let crate::model::SettingsMode::Open(ui) = &mut model.settings_mode {
                    if let Some(editor) = ui.theme_editor.as_mut() {
                        editor.set_field_hex(field, rgb);
                    }
                }
                if let crate::model::SettingsMode::Open(ui) = &model.settings_mode {
                    if let Some(editor) = ui.theme_editor.as_ref() {
                        let fallback = model
                            .config
                            .custom_themes
                            .get(&editor.original_name)
                            .cloned()
                            .unwrap_or_else(|| {
                                crate::settings::CustomTheme::from_palette(&model.palette)
                            });
                        model.palette = editor.preview_palette(&fallback);
                        model.grid_draw.invalidate_theme();
                    }
                }
            },
        ))
}

fn editor_text_row(
    label: &str,
    key: &'static str,
    edit: &rune::TextEdit,
    p: &Palette,
    on_edit: fn(&mut Model) -> &mut rune::TextEdit,
) -> El {
    let input = text_input(edit, TextStyle::new(13.0, 400.0))
        .key(key)
        .single_line()
        .grow(1.0)
        .h(30.0)
        .pl(10.0)
        .pr(10.0)
        .pt(7.0)
        .pb(7.0)
        .radius(6.0)
        .items_center()
        .fill(crate::theme::linear(p.tab_bg))
        .border(1.0, crate::theme::linear(p.active_border))
        .color(text_color(p))
        .on_edit(on_edit)
        .on_input_change(|model: &mut Model, tx: &mut rune::Tx| {
            crate::app::preview_theme_edit(model, tx);
        });
    row()
        .w_full()
        .h(32.0)
        .gap(10.0)
        .items_center()
        .child(
            el().w(88.0)
                .h(32.0)
                .items_center()
                .child(text(label).color(theme::linear(p.muted))),
        )
        .child(input)
}

fn editor_color_row(
    label: &str,
    key: &'static str,
    field: crate::model::ThemeColorField,
    editor: &crate::model::ThemeEditor,
    p: &Palette,
) -> El {
    let edit = editor.field_edit(field);
    let swatch = crate::model::parse_hex(&edit.text);
    let selected = editor.picking == Some(field);
    let mut input_row = row().grow(1.0).min_w(0.0).h(32.0).gap(8.0).items_center();
    if let Some(color) = swatch {
        input_row = input_row.child(
            el().key(format!("{key}-swatch"))
                .w(20.0)
                .h(20.0)
                .radius(4.0)
                .fill(crate::theme::linear(color))
                .border(
                    if selected { 2.0 } else { 1.0 },
                    crate::theme::linear(if selected { p.accent } else { p.active_border }),
                )
                .on_press(move |model: &mut Model| {
                    crate::app::select_theme_color_field(model, field);
                }),
        );
    }
    let on_edit: fn(&mut Model) -> &mut rune::TextEdit = match field {
        crate::model::ThemeColorField::Bg => |m| match &mut m.settings_mode {
            crate::model::SettingsMode::Open(ui) => &mut ui.theme_editor.as_mut().unwrap().bg,
            _ => unreachable!(),
        },
        crate::model::ThemeColorField::Fg => |m| match &mut m.settings_mode {
            crate::model::SettingsMode::Open(ui) => &mut ui.theme_editor.as_mut().unwrap().fg,
            _ => unreachable!(),
        },
        crate::model::ThemeColorField::FgBright => |m| match &mut m.settings_mode {
            crate::model::SettingsMode::Open(ui) => {
                &mut ui.theme_editor.as_mut().unwrap().fg_bright
            }
            _ => unreachable!(),
        },
        crate::model::ThemeColorField::Muted => |m| match &mut m.settings_mode {
            crate::model::SettingsMode::Open(ui) => &mut ui.theme_editor.as_mut().unwrap().muted,
            _ => unreachable!(),
        },
        crate::model::ThemeColorField::TabBg => |m| match &mut m.settings_mode {
            crate::model::SettingsMode::Open(ui) => &mut ui.theme_editor.as_mut().unwrap().tab_bg,
            _ => unreachable!(),
        },
        crate::model::ThemeColorField::ActiveBorder => |m| match &mut m.settings_mode {
            crate::model::SettingsMode::Open(ui) => {
                &mut ui.theme_editor.as_mut().unwrap().active_border
            }
            _ => unreachable!(),
        },
        crate::model::ThemeColorField::Accent => |m| match &mut m.settings_mode {
            crate::model::SettingsMode::Open(ui) => &mut ui.theme_editor.as_mut().unwrap().accent,
            _ => unreachable!(),
        },
    };
    input_row = input_row.child(
        text_input(edit, TextStyle::new(13.0, 400.0))
            .key(key)
            .single_line()
            .grow(1.0)
            .h(30.0)
            .pl(10.0)
            .pr(10.0)
            .pt(7.0)
            .pb(7.0)
            .radius(6.0)
            .items_center()
            .fill(crate::theme::linear(p.tab_bg))
            .border(1.0, crate::theme::linear(p.active_border))
            .color(text_color(p))
            .on_edit(on_edit)
            .on_input_change(|model: &mut Model, tx: &mut rune::Tx| {
                crate::app::preview_theme_edit(model, tx);
            }),
    );
    row()
        .w_full()
        .h(32.0)
        .gap(10.0)
        .items_center()
        .child(
            el().w(88.0)
                .h(32.0)
                .items_center()
                .child(text(label).color(theme::linear(p.muted))),
        )
        .child(input_row)
}

fn theme_choice(choice: &Palette, current: &Palette) -> El {
    let active = choice.name == current.name;
    let border = if active {
        choice.accent
    } else {
        current.active_border
    };
    let fill = if active { current.tab_bg } else { current.bg };
    let name = choice.name.clone();
    let name_label = name.clone();
    let name_color = if active {
        text_color(current)
    } else {
        crate::theme::linear(current.fg)
    };
    let meta_color = crate::theme::linear(current.muted);
    let is_builtin = crate::theme::resolve_builtin(&choice.name).is_some();
    let mut right = row().gap(8.0).items_center();
    if !choice.author.is_empty() {
        right = right.child(text(choice.author.as_str()).color(meta_color));
    }
    if !choice.url.is_empty() {
        let url = choice.url.clone();
        let url_key = name.clone();
        right = right.child(
            el().key(format!("theme-url-{url_key}"))
                .w(22.0)
                .h(22.0)
                .center()
                .radius(4.0)
                .hover(hover_overlay(current))
                .on_press_tx(move |_model: &mut Model, _tx| {
                    crate::hyperlink::open_uri(&url);
                })
                .child(text("↗").color(meta_color)),
        );
    }
    let edit_name = name.clone();
    let del_name = name.clone();
    let del_enabled = !is_builtin;
    let actions = row()
        .gap(2.0)
        .items_center()
        .child(row_action(
            "✎",
            &format!("theme-edit-{name}"),
            current,
            true,
            move |model, tx| {
                crate::app::edit_theme_named(model, tx, &edit_name);
            },
        ))
        .child(row_action(
            "✕",
            &format!("theme-del-{name}"),
            current,
            del_enabled,
            move |model, tx| {
                crate::app::delete_theme_named(model, tx, &del_name);
            },
        ));
    right = right.child(actions);
    row()
        .key(format!("theme-{name}"))
        .w_full()
        .h(34.0)
        .pl(10.0)
        .pr(6.0)
        .gap(10.0)
        .items_center()
        .justify_between()
        .radius(6.0)
        .border(if active { 1.5 } else { 1.0 }, crate::theme::linear(border))
        .fill(crate::theme::linear(fill))
        .focusable(true)
        .on_press_tx(move |model: &mut Model, tx: &mut rune::Tx| {
            crate::app::apply_theme(model, tx, &name);
        })
        .child(
            text(name_label)
                .grow(1.0)
                .min_w(0.0)
                .truncate()
                .color(name_color),
        )
        .child(right)
}

fn row_action(
    label: &str,
    key: &str,
    p: &Palette,
    enabled: bool,
    on_press: impl Fn(&mut Model, &mut rune::Tx) + 'static,
) -> El {
    let color = if enabled {
        crate::theme::linear(p.fg)
    } else {
        crate::theme::linear(p.muted)
    };
    let mut e = el().key(key).w(22.0).h(22.0).center().radius(4.0);
    if enabled {
        e = e.hover(hover_overlay(p)).on_press_tx(on_press);
    }
    e.child(text(label).color(color))
}

fn profiles_settings(model: &Model, p: &Palette) -> El {
    if matches!(
        &model.settings_mode,
        crate::model::SettingsMode::Open(ui) if ui.profile_editor.is_some()
    ) {
        return profile_editor_view(model);
    }
    let default_name = model.config.default_profile_name().to_owned();
    let mut names: Vec<String> = model.config.profile_names().map(str::to_owned).collect();
    names.sort();
    let can_delete_any = names.len() > 1;

    let mut list = col().w_full().gap(4.0);
    for name in &names {
        let is_default = name == &default_name;
        let resolved = model.config.resolve_profile(name);
        let mut meta = format!(
            "{} pt · {} · {}",
            resolved.font_size as i32, resolved.theme, resolved.backend
        );
        if let Some(cwd) = &resolved.cwd {
            meta.push_str(" · ");
            meta.push_str(cwd);
        }
        let border = if is_default {
            theme::linear(p.accent)
        } else {
            theme::linear(p.active_border)
        };
        let fill = if is_default {
            theme::linear(p.tab_bg)
        } else {
            theme::linear(p.bg)
        };
        let name_for_set = name.clone();
        let name_for_dup = name.clone();
        let name_for_del = name.clone();
        let name_for_edit = name.clone();
        let name_label = name.clone();
        let label_color = if is_default {
            text_color(p)
        } else {
            theme::linear(p.fg)
        };

        let mut actions = row().gap(2.0).items_center();
        actions = actions.child(row_action(
            "★",
            &format!("profile-default-{name}"),
            p,
            !is_default,
            move |model, tx| {
                crate::app::set_default_profile(model, tx, &name_for_set);
            },
        ));
        actions = actions.child(row_action(
            "✎",
            &format!("profile-edit-{name}"),
            p,
            true,
            move |model, tx| {
                crate::app::edit_profile_named(model, tx, &name_for_edit);
            },
        ));
        actions = actions.child(row_action(
            "⎘",
            &format!("profile-dup-{name}"),
            p,
            true,
            move |model, tx| {
                if let Some(new_name) = crate::app::duplicate_profile(model, tx, &name_for_dup) {
                    crate::app::edit_profile_named(model, tx, &new_name);
                }
            },
        ));
        actions = actions.child(row_action(
            "✕",
            &format!("profile-del-{name}"),
            p,
            can_delete_any,
            move |model, tx| {
                crate::app::delete_profile(model, tx, &name_for_del);
            },
        ));

        let info = col()
            .grow(1.0)
            .min_w(0.0)
            .gap(2.0)
            .child(
                text(if is_default {
                    format!("{name_label} (default)")
                } else {
                    name_label
                })
                .truncate()
                .color(label_color),
            )
            .child(text(meta).truncate().color(theme::linear(p.muted)));

        let row_el = row()
            .key(format!("profile-{name}"))
            .w_full()
            .min_h(40.0)
            .pl(10.0)
            .pr(6.0)
            .pt(6.0)
            .pb(6.0)
            .gap(10.0)
            .items_center()
            .justify_between()
            .radius(6.0)
            .border(if is_default { 1.5 } else { 1.0 }, border)
            .fill(fill)
            .child(info)
            .child(actions);
        list = list.child(row_el);
    }

    let list = col()
        .w_full()
        .grow(1.0)
        .min_h(220.0)
        .scroll(ScrollSpec {
            stick_bottom: false,
        })
        .radius(radius::MD)
        .fill(theme::linear(p.bg))
        .border(1.0, theme::linear(p.active_border))
        .pt(space::S2)
        .pb(space::S1)
        .pl(space::S2)
        .pr(space::S2)
        .child(list);

    let actions = row().w_full().gap(space::S2).child(settings_chip(
        "New Profile",
        "profile-new",
        p,
        |model, tx| {
            let src = model.config.default_profile_name().to_owned();
            if let Some(name) = crate::app::duplicate_profile(model, tx, &src) {
                crate::app::edit_profile_named(model, tx, &name);
            }
        },
    ));

    col()
        .w_full()
        .grow(1.0)
        .gap(space::S3)
        .pb(space::S2)
        .child(actions)
        .child(list)
}

fn profile_editor_view(model: &Model) -> El {
    let p = model.palette.as_ref();
    let editor = match &model.settings_mode {
        crate::model::SettingsMode::Open(ui) => match ui.profile_editor.as_ref() {
            Some(editor) => editor,
            None => return col(),
        },
        _ => return col(),
    };
    let mut grid = col().w_full().gap(6.0);
    #[allow(clippy::type_complexity)]
    let rows: [(
        &str,
        &str,
        &rune::TextEdit,
        fn(&mut Model) -> &mut rune::TextEdit,
    ); 7] = [
        ("Name", "pe-name", &editor.name, |m| {
            match &mut m.settings_mode {
                crate::model::SettingsMode::Open(ui) => {
                    &mut ui.profile_editor.as_mut().unwrap().name
                }
                _ => unreachable!(),
            }
        }),
        (
            "Font size",
            "pe-font-size",
            &editor.font_size,
            |m| match &mut m.settings_mode {
                crate::model::SettingsMode::Open(ui) => {
                    &mut ui.profile_editor.as_mut().unwrap().font_size
                }
                _ => unreachable!(),
            },
        ),
        ("Theme", "pe-theme", &editor.theme, |m| {
            match &mut m.settings_mode {
                crate::model::SettingsMode::Open(ui) => {
                    &mut ui.profile_editor.as_mut().unwrap().theme
                }
                _ => unreachable!(),
            }
        }),
        ("Backend", "pe-backend", &editor.backend, |m| {
            match &mut m.settings_mode {
                crate::model::SettingsMode::Open(ui) => {
                    &mut ui.profile_editor.as_mut().unwrap().backend
                }
                _ => unreachable!(),
            }
        }),
        (
            "Shell program",
            "pe-shell-program",
            &editor.shell_program,
            |m| match &mut m.settings_mode {
                crate::model::SettingsMode::Open(ui) => {
                    &mut ui.profile_editor.as_mut().unwrap().shell_program
                }
                _ => unreachable!(),
            },
        ),
        (
            "Shell args",
            "pe-shell-args",
            &editor.shell_args,
            |m| match &mut m.settings_mode {
                crate::model::SettingsMode::Open(ui) => {
                    &mut ui.profile_editor.as_mut().unwrap().shell_args
                }
                _ => unreachable!(),
            },
        ),
        ("CWD", "pe-cwd", &editor.cwd, |m| {
            match &mut m.settings_mode {
                crate::model::SettingsMode::Open(ui) => {
                    &mut ui.profile_editor.as_mut().unwrap().cwd
                }
                _ => unreachable!(),
            }
        }),
    ];
    for (label, key, te, on_edit) in rows {
        grid = grid.child(profile_editor_row(label, key, te, p, on_edit));
    }
    let current_family = editor.font_family.text.trim().to_owned();
    let family_label = if current_family.is_empty() {
        "Cascadia Code NF".to_owned()
    } else {
        current_family.clone()
    };
    let picker_open = editor.font_picker_open;
    let font_entry = row()
        .key("pe-font-family-entry")
        .w_full()
        .h(36.0)
        .items_center()
        .justify_between()
        .pl(10.0)
        .pr(10.0)
        .radius(6.0)
        .fill(theme::linear(p.tab_bg))
        .border(1.0, theme::linear(p.active_border))
        .hover(hover_overlay(p))
        .on_press_tx(|model: &mut Model, _tx| {
            if let crate::model::SettingsMode::Open(ui) = &mut model.settings_mode {
                if let Some(editor) = ui.profile_editor.as_mut() {
                    editor.font_picker_open = !editor.font_picker_open;
                }
            }
        })
        .child(text(family_label).color(text_color(p)).truncate())
        .child(text(if picker_open { "▴" } else { "▾" }).color(theme::linear(p.muted)));

    let mut font_picker = col()
        .w_full()
        .gap(6.0)
        .pt(6.0)
        .child(text("Font family").color(theme::linear(p.muted)))
        .child(font_entry);
    if picker_open {
        let catalog = font_picker::FontCatalog::system();
        font_picker = font_picker.child(
            el().w_full()
                .h(180.0)
                .radius(6.0)
                .border(1.0, theme::linear(p.active_border))
                .fill(theme::linear(p.bg))
                .child(font_picker::family_list::<Model, _>(
                    &catalog.families,
                    &current_family,
                    |model: &mut Model, family: &str| {
                        if let crate::model::SettingsMode::Open(ui) = &mut model.settings_mode {
                            if let Some(editor) = ui.profile_editor.as_mut() {
                                editor.font_family = rune::TextEdit::new(family);
                                editor.font_picker_open = false;
                            }
                        }
                    },
                )),
        );
    }
    col()
        .w_full()
        .grow(1.0)
        .gap(10.0)
        .child(
            row()
                .w_full()
                .items_center()
                .justify_between()
                .child(text("Edit profile").color(text_color(p)))
                .child(text(&editor.original_name).color(theme::linear(p.muted))),
        )
        .child(
            col()
                .w_full()
                .grow(1.0)
                .scroll(ScrollSpec {
                    stick_bottom: false,
                })
                .gap(6.0)
                .child(grid)
                .child(font_picker),
        )
        .child(
            row()
                .w_full()
                .gap(8.0)
                .justify_center()
                .child(settings_chip("Cancel", "pe-cancel", p, |model, tx| {
                    crate::app::cancel_profile_edit(model, tx);
                }))
                .child(settings_chip("Save", "pe-save", p, |model, tx| {
                    crate::app::save_profile_edit(model, tx);
                })),
        )
}

fn profile_editor_row(
    label: &str,
    key: &'static str,
    edit: &rune::TextEdit,
    p: &Palette,
    on_edit: fn(&mut Model) -> &mut rune::TextEdit,
) -> El {
    let input = text_input(edit, TextStyle::new(13.0, 400.0))
        .key(key)
        .single_line()
        .grow(1.0)
        .h(30.0)
        .pl(10.0)
        .pr(10.0)
        .pt(7.0)
        .pb(7.0)
        .radius(6.0)
        .items_center()
        .fill(theme::linear(p.tab_bg))
        .border(1.0, theme::linear(p.active_border))
        .color(text_color(p))
        .on_edit(on_edit);
    row()
        .w_full()
        .h(32.0)
        .gap(10.0)
        .items_center()
        .child(
            el().w(96.0)
                .h(32.0)
                .items_center()
                .child(text(label).color(theme::linear(p.muted))),
        )
        .child(input)
}

fn settings_chip(
    label: &str,
    key: &str,
    p: &Palette,
    on_press: fn(&mut Model, &mut rune::Tx),
) -> El {
    el().key(key)
        .pl(10.0)
        .pr(10.0)
        .pt(5.0)
        .pb(5.0)
        .radius(7.0)
        .border(1.0, theme::linear(p.active_border))
        .fill(theme::linear(p.tab_bg))
        .focusable(true)
        .on_press_tx(on_press)
        .child(text(label).color(text_color(p)))
}

fn settings_step(label: &str, key: &str, p: &Palette, delta: f32) -> El {
    el().key(key)
        .w(34.0)
        .h(34.0)
        .center()
        .radius(8.0)
        .border(1.0, theme::linear(p.active_border))
        .fill(theme::linear(p.tab_bg))
        .focusable(true)
        .on_press_tx(move |model: &mut Model, tx: &mut rune::Tx| {
            crate::app::apply_font_size(model, tx, model.font_size + delta);
        })
        .child(text(label).color(text_color(p)))
}

fn font_preset(size: f32, p: &Palette) -> El {
    let key = format!("font-{size:.0}");
    let label = format!("{size:.0}");
    el().key(key)
        .grow(1.0)
        .h(28.0)
        .center()
        .radius(7.0)
        .border(1.0, theme::linear(p.active_border))
        .fill(theme::linear(p.tab_bg))
        .focusable(true)
        .on_press_tx(move |model: &mut Model, tx: &mut rune::Tx| {
            crate::app::apply_font_size(model, tx, size);
        })
        .child(text(label).color(theme::linear(p.muted)))
}

fn confirm_open_link(uri: &str, p: &Palette) -> El {
    let shown = if uri.len() > 80 {
        format!("{}…", &uri[..uri.floor_char_boundary(79)])
    } else {
        uri.to_string()
    };
    let to_open = uri.to_string();
    let card = col()
        .key("confirm-open-link")
        .w(420.0)
        .fill(theme::linear(p.tab_bg))
        .border(1.0, theme::linear(p.active_border))
        .radius(8.0)
        .gap(12.0)
        .pl(16.0)
        .pr(16.0)
        .pt(14.0)
        .pb(14.0)
        .modal(true, |model: &mut Model| {
            model.pending_link_open = None;
        })
        .child(text("Open external link?").color(text_color(p)))
        .child(text(shown).color(theme::linear(p.muted)))
        .child(
            row()
                .w_full()
                .gap(8.0)
                .justify_center()
                .child(
                    el().key("cancel-open-link")
                        .pl(12.0)
                        .pr(12.0)
                        .pt(6.0)
                        .pb(6.0)
                        .radius(6.0)
                        .border(1.0, theme::linear(p.active_border))
                        .focusable(true)
                        .on_press_tx(|model: &mut Model, _tx| {
                            model.pending_link_open = None;
                        })
                        .child(text("Cancel").color(theme::linear(p.muted))),
                )
                .child(
                    el().key("open-link")
                        .pl(12.0)
                        .pr(12.0)
                        .pt(6.0)
                        .pb(6.0)
                        .radius(6.0)
                        .fill(theme::linear(p.accent))
                        .focusable(true)
                        .on_press_tx(move |model: &mut Model, _tx| {
                            let _ = crate::hyperlink::open_uri_with(&to_open, |_| true);
                            model.pending_link_open = None;
                        })
                        .child(text("Open").color(text_color(p))),
                ),
        );
    rune::el()
        .key("confirm-open-link-overlay")
        .absolute(0.0, 0.0)
        .w_full()
        .h_full()
        .center()
        .child(card)
}

fn tab_strip(model: &Model, cx: &mut Cx<Model>) -> El {
    let p = model.palette.as_ref();
    let insets = cx.chrome_insets();
    let h = if insets.height > 0.0 {
        insets.height
    } else {
        TITLEBAR_H
    };
    // Leave room for native window controls (macOS traffic lights / Windows
    // caption buttons) that sit over the custom titlebar.
    let leading = insets.leading.max(cx.titlebar_inset());
    let trailing = insets.trailing;
    let tab_h = (h - 10.0).max(22.0);
    let mut strip = row()
        .key("tabs")
        .w_full()
        .h(h)
        .fill(theme::linear(p.tab_bg))
        .center_y()
        .pl(leading + 10.0)
        .pr(trailing + 10.0)
        .gap(6.0)
        .window_drag()
        .scroll_x();
    if model.tabs.is_empty() {
        return strip.child(new_tab_button(tab_h, p));
    }
    for index in 0..model.tabs.len() {
        let active = index == model.active;
        let color = if active {
            text_color(p)
        } else {
            theme::linear(p.muted)
        };
        let mut tab = row()
            .key(format!("tab-{index}"))
            .h(tab_h)
            .w(TAB_W)
            .pl(TAB_PAD_X)
            .pr(4.0)
            .gap(6.0)
            .center_y()
            .radius(TAB_RADIUS)
            .preserve_focus_on_press(true)
            .on_press_tx(move |model: &mut Model, tx| {
                if crate::tabs::select(model, index) {
                    crate::tabs::focus_active(model, tx);
                } else {
                    tx.live_only();
                }
            });
        if active {
            tab = tab
                .fill(theme::linear(p.bg))
                .border(1.0, theme::linear(p.active_border));
        }
        let label = text(tab_caption(model, index))
            .grow(1.0)
            .min_w(0.0)
            .color(color)
            .truncate();
        let close = row()
            .key(format!("tab-close-{index}"))
            .w(tab_h - 8.0)
            .h(tab_h - 8.0)
            .center()
            .radius(TAB_RADIUS)
            .hover(hover_overlay(p))
            .preserve_focus_on_press(true)
            .on_press_tx(move |model: &mut Model, tx| {
                crate::tabs::close(model, index, tx);
                crate::tabs::focus_active(model, tx);
            })
            .child(text("×").color(theme::linear(p.muted)));
        strip = strip.child(tab.child(label).child(close));
    }
    strip.child(new_tab_button(tab_h, p))
}

fn new_tab_button(tab_h: f32, p: &Palette) -> El {
    let size = tab_h - 8.0;
    row()
        .key("tab-new")
        .w(size)
        .h(size)
        .center()
        .radius(TAB_RADIUS)
        .hover(hover_overlay(p))
        .preserve_focus_on_press(true)
        .on_press_tx(move |model: &mut Model, tx| {
            crate::tabs::open(model);
            crate::tabs::reveal(model, tx);
        })
        .child(text("+").color(theme::linear(p.muted)))
}

fn hover_overlay(p: &Palette) -> Color {
    let mut c = theme::linear(p.fg);
    c.a = 0.10;
    c
}

fn tab_caption(model: &Model, index: usize) -> String {
    let Some(tab) = model.tabs.get(index) else {
        return "rune-term".to_string();
    };
    let bell = crate::tabs::surface_of(tab)
        .and_then(|id| model.surfaces.get(id))
        .is_some_and(|host| host.bell);
    if bell {
        format!("{} •", tab.title)
    } else {
        tab.title.clone()
    }
}

#[cfg(all(test, not(feature = "pty")))]
mod tests {
    use super::*;
    use rune::{Env, Runtime};

    fn tab_h() -> f32 {
        (TITLEBAR_H - 10.0).max(22.0)
    }

    fn host() -> Model {
        let mut model = crate::model::init();
        crate::delegate::setup(&mut model, std::sync::Arc::new(|| {}));
        model
    }

    fn rect(rt: &Runtime<Model>, key: &str) -> [f32; 4] {
        rt.presented(key)
            .unwrap_or_else(|| panic!("missing {key}"))
            .0
    }

    #[test]
    fn a_tab_stays_wide_enough_to_hit() {
        let mut model = host();
        model.tabs[0].title = "x".into();
        crate::tabs::open(&mut model);
        model.tabs[1].title = "a-very-long-shell-title-that-would-otherwise-eat-the-strip".into();
        for _ in 0..8 {
            crate::tabs::open(&mut model);
        }
        let mut rt = Runtime::new(model, view, (800.0, 500.0), Env::new(true, 1.0));
        rt.frame(0.0);
        for index in 0..10 {
            let tab = rect(&rt, &format!("tab-{index}"));
            assert!((tab[2] - TAB_W).abs() < 0.5, "tab {index} width {}", tab[2]);
            assert!(
                (tab[3] - tab_h()).abs() < 0.5,
                "tab {index} height {}",
                tab[3]
            );
        }
    }

    #[test]
    fn the_edge_of_a_tab_selects_it() {
        let mut model = host();
        crate::tabs::open(&mut model);
        let mut rt = Runtime::new(model, view, (800.0, 500.0), Env::new(true, 1.0));
        rt.frame(0.0);
        assert_eq!(rt.state.active, 1);
        let tab = rect(&rt, "tab-0");
        let x = tab[0] + 2.0;
        let y = tab[1] + tab[3] - 2.0;
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
        rt.frame(0.0);
        assert_eq!(rt.state.active, 0);
    }
}
