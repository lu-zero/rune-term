//! One tab strip. The panes are [`crate::chrome_view`]. The cell tree is [`crate::split`].

use rune::{col, el, row, text, Color, Cx, El};

use crate::chrome_view;
use crate::model::Model;
use crate::theme;

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
pub fn tab() -> rune::Color {
    theme::linear(theme::TAB_BG)
}

/// Cell background. Transparent cells show this.
pub fn slot() -> rune::Color {
    theme::linear(theme::BG)
}

pub fn text_color() -> rune::Color {
    theme::linear(theme::FG)
}

pub fn slot_premul(srgb_blend: bool) -> [f32; 4] {
    let slot = slot();
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
    let tab_bg = tab();
    let tab = tab_strip(model, cx);
    let pane = chrome_view::area(model, cx);
    // Rune delivers Cmd+V to the root handler, not to the focused pane.
    let mut root = col()
        .w_full()
        .h_full()
        .fill(tab_bg)
        .on_paste(crate::selection::on_paste)
        .child(tab)
        .child(pane);
    if let Some(uri) = &model.pending_link_open {
        root = root.child(confirm_open_link(uri));
    }
    root
}

fn confirm_open_link(uri: &str) -> El {
    let shown = if uri.len() > 80 {
        format!("{}…", &uri[..uri.floor_char_boundary(79)])
    } else {
        uri.to_string()
    };
    let to_open = uri.to_string();
    let card = col()
        .key("confirm-open-link")
        .w(420.0)
        .fill(theme::linear(theme::TAB_BG))
        .border(1.0, theme::linear(theme::ACTIVE_BORDER))
        .radius(8.0)
        .gap(12.0)
        .pl(16.0)
        .pr(16.0)
        .pt(14.0)
        .pb(14.0)
        .modal(true, |model: &mut Model| {
            model.pending_link_open = None;
        })
        .child(text("Open external link?").color(text_color()))
        .child(text(shown).color(theme::linear(theme::MUTED)))
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
                        .border(1.0, theme::linear(theme::ACTIVE_BORDER))
                        .focusable(true)
                        .on_press_tx(|model: &mut Model, _tx| {
                            model.pending_link_open = None;
                        })
                        .child(text("Cancel").color(theme::linear(theme::MUTED))),
                )
                .child(
                    el().key("open-link")
                        .pl(12.0)
                        .pr(12.0)
                        .pt(6.0)
                        .pb(6.0)
                        .radius(6.0)
                        .fill(theme::linear(0x205E_A6))
                        .focusable(true)
                        .on_press_tx(move |model: &mut Model, _tx| {
                            let _ = crate::hyperlink::open_uri_with(&to_open, |_| true);
                            model.pending_link_open = None;
                        })
                        .child(text("Open").color(text_color())),
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
        .fill(tab())
        .center_y()
        .pl(leading + 10.0)
        .pr(insets.trailing + 10.0)
        .gap(6.0)
        .window_drag()
        .scroll_x();
    if model.tabs.is_empty() {
        return strip.child(new_tab_button(tab_h));
    }
    for index in 0..model.tabs.len() {
        let active = index == model.active;
        let color = if active {
            text_color()
        } else {
            theme::linear(theme::MUTED)
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
                .fill(slot())
                .border(1.0, theme::linear(theme::ACTIVE_BORDER));
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
            .hover(hover_overlay())
            .preserve_focus_on_press(true)
            .on_press_tx(move |model: &mut Model, tx| {
                crate::tabs::close(model, index, tx);
            })
            .child(text("×").color(theme::linear(theme::MUTED)));
        strip = strip.child(tab.child(label).child(close));
    }
    strip.child(new_tab_button(tab_h))
}

fn new_tab_button(tab_h: f32) -> El {
    let size = tab_h - 8.0;
    row()
        .key("tab-new")
        .w(size)
        .h(size)
        .center()
        .radius(TAB_RADIUS)
        .hover(hover_overlay())
        .preserve_focus_on_press(true)
        .on_press_tx(move |model: &mut Model, tx| {
            crate::tabs::open(model);
            crate::tabs::reveal(model, tx);
        })
        .child(text("+").color(theme::linear(theme::MUTED)))
}

fn hover_overlay() -> Color {
    let mut c = theme::linear(theme::FG);
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
