//! One tab strip. The panes are [`crate::chrome_view`]. The cell tree is [`crate::split`].

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
    if model.settings_open {
        root = root.child(settings_panel(model));
    }
    root
}

fn settings_panel(model: &Model) -> El {
    let p = model.palette.as_ref();
    let header = row()
        .w_full()
        .items_center()
        .justify_between()
        .child(text("Settings").color(text_color(p)))
        .child(settings_chip("Close", "close-settings", p, |model, _tx| {
            model.settings_open = false;
            model.theme_editor = None;
        }));
    let tabs = row()
        .w_full()
        .gap(6.0)
        .child(settings_tab_chip(
            "Font",
            crate::model::SettingsTab::Font,
            model.settings_tab,
            p,
        ))
        .child(settings_tab_chip(
            "Theme",
            crate::model::SettingsTab::Theme,
            model.settings_tab,
            p,
        ));
    let body = match model.settings_tab {
        crate::model::SettingsTab::Font => font_settings(model, p),
        crate::model::SettingsTab::Theme => theme_settings(model, p),
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
        .w(520.0)
        .h_full()
        .fill(theme::linear(p.tab_bg))
        .border(1.0, theme::linear(p.active_border))
        .radius(12.0)
        .gap(12.0)
        .pl(18.0)
        .pr(18.0)
        .pt(16.0)
        .pb(16.0)
        .modal(true, |model: &mut Model| {
            model.settings_open = false;
            model.theme_editor = None;
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
        .pl(12.0)
        .pr(12.0)
        .pt(6.0)
        .pb(6.0)
        .radius(7.0)
        .border(
            if selected { 1.5 } else { 1.0 },
            theme::linear(if selected { p.accent } else { p.active_border }),
        )
        .fill(theme::linear(if selected { p.bg } else { p.tab_bg }))
        .focusable(true)
        .on_press_tx(move |model: &mut Model, _tx| {
            model.settings_tab = tab;
        })
        .child(text(label).color(if selected {
            text_color(p)
        } else {
            theme::linear(p.muted)
        }))
}

fn font_settings(model: &Model, p: &Palette) -> El {
    let (cw, ch) = model.cell_points;
    let size = model.font_size;
    let _ = (cw, ch);
    col()
        .w_full()
        .grow(1.0)
        .gap(10.0)
        .pb(8.0)
        .child(
            row()
                .w_full()
                .gap(8.0)
                .items_center()
                .child(settings_step("−", "font-dec", p, -1.0))
                .child(
                    el().grow(1.0)
                        .h(34.0)
                        .center()
                        .radius(8.0)
                        .fill(theme::linear(p.bg))
                        .border(1.0, theme::linear(p.active_border))
                        .child(
                            text(format!("{size:.0} pt · Cascadia Code NF")).color(text_color(p)),
                        ),
                )
                .child(settings_step("+", "font-inc", p, 1.0)),
        )
        .child(
            row()
                .w_full()
                .gap(6.0)
                .child(font_preset(11.0, p))
                .child(font_preset(12.0, p))
                .child(font_preset(14.0, p))
                .child(font_preset(16.0, p))
                .child(font_preset(18.0, p))
                .child(font_preset(20.0, p)),
        )
}

fn theme_settings(model: &Model, p: &Palette) -> El {
    if model.theme_editor.is_some() {
        return theme_editor_view(model);
    }
    let mut themes = col().w_full().gap(4.0);
    for t in theme::builtins() {
        themes = themes.child(theme_choice(t, p));
    }
    for (name, custom) in &model.config.custom_themes {
        let tp = custom.to_arc_palette(name);
        themes = themes.child(theme_choice(&tp, p));
    }
    let themes = themes.pb(4.0);
    let theme_list = col()
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .scroll(ScrollSpec {
            stick_bottom: false,
        })
        .radius(8.0)
        .fill(crate::theme::linear(p.bg))
        .border(1.0, crate::theme::linear(p.active_border))
        .pt(8.0)
        .pb(4.0)
        .pl(8.0)
        .pr(8.0)
        .child(themes);
    let is_custom = model.config.is_custom_theme(&model.palette.name);
    let mut theme_actions = row().w_full().gap(6.0).items_center().justify_end();
    theme_actions = theme_actions.child(settings_chip("Copy", "theme-copy", p, |model, tx| {
        crate::app::copy_theme(model, tx);
    }));
    if is_custom {
        theme_actions = theme_actions.child(settings_chip("Edit", "theme-edit", p, |model, tx| {
            crate::app::edit_theme(model, tx);
        }));
        theme_actions =
            theme_actions.child(settings_chip("Delete", "theme-delete", p, |model, tx| {
                crate::app::delete_theme(model, tx);
            }));
    }
    col()
        .w_full()
        .grow(1.0)
        .gap(8.0)
        .child(theme_actions)
        .child(theme_list)
}

fn theme_editor_view(model: &Model) -> El {
    let p = model.palette.as_ref();
    let Some(editor) = &model.theme_editor else {
        return col();
    };
    let mut grid = col().w_full().gap(6.0);
    let rows: [(
        &str,
        &str,
        &rune::TextEdit,
        fn(&mut Model) -> &mut rune::TextEdit,
    ); 10] = [
        ("Name", "te-name", &editor.name, |m| {
            &mut m.theme_editor.as_mut().unwrap().name
        }),
        ("Author", "te-author", &editor.author, |m| {
            &mut m.theme_editor.as_mut().unwrap().author
        }),
        ("URL", "te-url", &editor.url, |m| {
            &mut m.theme_editor.as_mut().unwrap().url
        }),
        ("Background", "te-bg", &editor.bg, |m| {
            &mut m.theme_editor.as_mut().unwrap().bg
        }),
        ("Foreground", "te-fg", &editor.fg, |m| {
            &mut m.theme_editor.as_mut().unwrap().fg
        }),
        ("Bright FG", "te-fgb", &editor.fg_bright, |m| {
            &mut m.theme_editor.as_mut().unwrap().fg_bright
        }),
        ("Muted", "te-muted", &editor.muted, |m| {
            &mut m.theme_editor.as_mut().unwrap().muted
        }),
        ("Tab BG", "te-tab", &editor.tab_bg, |m| {
            &mut m.theme_editor.as_mut().unwrap().tab_bg
        }),
        ("Border", "te-border", &editor.active_border, |m| {
            &mut m.theme_editor.as_mut().unwrap().active_border
        }),
        ("Accent", "te-accent", &editor.accent, |m| {
            &mut m.theme_editor.as_mut().unwrap().accent
        }),
    ];
    for (label, key, te, on_edit) in rows {
        grid = grid.child(editor_row(label, key, te, p, on_edit));
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
                .child(grid),
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

fn editor_row(
    label: &str,
    key: &'static str,
    edit: &rune::TextEdit,
    p: &Palette,
    on_edit: fn(&mut Model) -> &mut rune::TextEdit,
) -> El {
    let swatch = crate::model::parse_hex(&edit.text);
    let mut input_row = row().grow(1.0).min_w(0.0).h(32.0).gap(8.0).items_center();
    if let Some(color) = swatch {
        input_row = input_row.child(
            el().w(20.0)
                .h(20.0)
                .radius(4.0)
                .fill(crate::theme::linear(color))
                .border(1.0, crate::theme::linear(p.active_border)),
        );
    }
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
    let mut right = row().gap(8.0).items_center();
    if !choice.author.is_empty() {
        right = right.child(text(choice.author.as_str()).color(meta_color));
    }
    if !choice.url.is_empty() {
        right = right.child(text("↗").color(meta_color));
    }
    row()
        .key(format!("theme-{name}"))
        .w_full()
        .h(32.0)
        .pl(10.0)
        .pr(10.0)
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
    let leading = if insets.height > 0.0 {
        insets.leading
    } else {
        cx.titlebar_inset()
    };
    let tab_h = (h - 10.0).max(22.0);
    let mut strip = row()
        .key("tabs")
        .w_full()
        .h(h)
        .fill(theme::linear(p.tab_bg))
        .center_y()
        .pl(leading + 10.0)
        .pr(insets.trailing + 10.0)
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
                if !crate::tabs::select(model, index) {
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
