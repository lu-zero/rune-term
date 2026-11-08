//! One tab strip. The panes are [`crate::chrome_view`]. The cell tree is [`crate::split`].

use rune::{col, row, text, Cx, El};

use crate::chrome_view;
use crate::model::Model;
use crate::theme;

pub const PANE_MARK: u32 = 1;
pub const LEAF: LeafId = LeafId(1);

pub use crate::split::{Leaf, LeafId, Node};
const TAB_H: f32 = 28.0;
/// One hit target. A short title and a long one use this same box.
const TAB_W: f32 = 120.0;

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
    col()
        .w_full()
        .h_full()
        .fill(tab_bg)
        .on_paste(crate::selection::on_paste)
        .child(tab)
        .child(pane)
}

fn tab_strip(model: &Model, cx: &mut Cx<Model>) -> El {
    let mut strip = row()
        .key("tabs")
        .w_full()
        .h(TAB_H)
        .fill(tab())
        .center_y()
        // Past the traffic lights. The inset is their trailing edge, and 0 when
        // they are not drawn over the content.
        .pl(cx.titlebar_inset() + 12.0)
        .pr(12.0)
        .gap(4.0)
        .scroll_x();
    if model.tabs.is_empty() {
        return strip.child(text("rune-term").color(text_color()));
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
            .h(TAB_H)
            .w(TAB_W)
            .px(12.0)
            .center_y()
            .preserve_focus_on_press(true)
            .on_press_tx(move |model: &mut Model, tx| {
                if !crate::tabs::select(model, index) {
                    tx.live_only();
                }
            });
        if active {
            tab = tab.fill(slot());
        }
        strip = strip.child(tab.child(text(tab_caption(model, index)).color(color).truncate()));
    }
    strip
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
                (tab[3] - TAB_H).abs() < 0.5,
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
