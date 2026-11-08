//! One tab strip. The panes are [`crate::chrome_view`]. The cell tree is [`crate::split`].

use rune::{col, row, text, Cx, El};

use crate::chrome_view;
use crate::model::Model;
use crate::theme;

pub const PANE_MARK: u32 = 1;
pub const LEAF: LeafId = LeafId(1);

pub use crate::split::{Leaf, LeafId, Node};
const TAB_H: f32 = 28.0;

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
        .w_full()
        .h(TAB_H)
        .fill(tab())
        .center_y()
        // Past the traffic lights. The inset is their trailing edge, and 0 when
        // they are not drawn over the content.
        .pl(cx.titlebar_inset() + 12.0)
        .gap(16.0);
    if model.tabs.is_empty() {
        return strip.child(text("rune-term").color(text_color()));
    }
    for index in 0..model.tabs.len() {
        let color = if index == model.active {
            text_color()
        } else {
            theme::linear(theme::MUTED)
        };
        strip = strip.child(
            text(tab_caption(model, index))
                .color(color)
                .key(format!("tab-{index}"))
                .preserve_focus_on_press(true)
                .on_press_tx(move |model: &mut Model, tx| {
                    if !crate::tabs::select(model, index) {
                        tx.live_only();
                    }
                }),
        );
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
