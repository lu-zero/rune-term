//! One tab strip. The panes are [`crate::chrome_view`]. The cell tree is [`crate::split`].

use rune::{col, el, row, text, Color, Cx, El, ScrollSpec};

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
    let p = model.palette;
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
    let p = model.palette;
    let (cw, ch) = model.cell_points;
    let size = model.font_size;
    let mut themes = col().w_full().gap(4.0);
    for t in theme::BUILTINS {
        themes = themes.child(theme_choice(t, p));
    }
    let theme_list = col()
        .w_full()
        .h(110.0)
        .scroll(ScrollSpec {
            stick_bottom: false,
        })
        .radius(8.0)
        .fill(crate::theme::linear(p.bg))
        .border(1.0, crate::theme::linear(p.active_border))
        .pt(4.0)
        .pb(4.0)
        .pl(4.0)
        .pr(4.0)
        .child(themes);
    let card = col()
        .key("settings")
        .w(440.0)
        .fill(theme::linear(p.tab_bg))
        .border(1.0, theme::linear(p.active_border))
        .radius(12.0)
        .gap(14.0)
        .pl(18.0)
        .pr(18.0)
        .pt(16.0)
        .pb(16.0)
        .modal(true, |model: &mut Model| {
            model.settings_open = false;
        })
        .child(
            row()
                .w_full()
                .items_center()
                .justify_between()
                .child(text("Settings").color(text_color(p)))
                .child(settings_chip("Close", "close-settings", p, |model, _tx| {
                    model.settings_open = false;
                })),
        )
        .child(
            col()
                .w_full()
                .gap(8.0)
                .child(text("Font size").color(text_color(p)))
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
                                    text(format!("{size:.0} pt · Cascadia Code NF"))
                                        .color(text_color(p)),
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
                .child(
                    text(format!("Cell {cw:.1}×{ch:.1} pt · saved to config"))
                        .color(theme::linear(p.muted)),
                ),
        )
        .child(
            col()
                .w_full()
                .gap(8.0)
                .child(text("Theme").color(text_color(p)))
                .child(theme_list)
                .child(text(p.name).color(theme::linear(p.muted))),
        );
    rune::el()
        .key("settings-overlay")
        .absolute(0.0, 0.0)
        .w_full()
        .h_full()
        .center()
        .child(card)
}

fn theme_choice(choice: &'static Palette, current: &'static Palette) -> El {
    let active = choice.name == current.name;
    let border = if active {
        choice.accent
    } else {
        current.active_border
    };
    let fill = if active { current.tab_bg } else { current.bg };
    el().key(format!("theme-{}", choice.name))
        .w_full()
        .h(32.0)
        .pl(10.0)
        .pr(10.0)
        .items_center()
        .radius(6.0)
        .border(if active { 1.5 } else { 1.0 }, crate::theme::linear(border))
        .fill(crate::theme::linear(fill))
        .focusable(true)
        .on_press_tx(move |model: &mut Model, tx: &mut rune::Tx| {
            crate::app::apply_theme(model, tx, choice.name);
        })
        .child(text(choice.name).color(text_color(current)))
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

fn confirm_open_link(uri: &str, p: &'static Palette) -> El {
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
    let p = model.palette;
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
