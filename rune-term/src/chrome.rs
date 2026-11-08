//! One tab strip. The panes are [`crate::chrome_view`]. The cell tree is [`crate::split`].

use rune::{col, row, text, Cx, El};

use crate::chrome_view;
use crate::model::Model;
use crate::theme;

pub const PANE_MARK: u32 = 1;
pub const LEAF: LeafId = LeafId(1);

pub use crate::split::{Leaf, LeafId, Node};
const TAB_H: f32 = 28.0;

/// Tab strip. A step darker than the cell background.
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
    let label = tab_label(model);
    let tab_bg = tab();
    let tab = row()
        .w_full()
        .h(TAB_H)
        .fill(tab_bg)
        .center_y()
        // Past the traffic lights. The inset is their trailing edge, and 0 when
        // they are not drawn over the content.
        .pl(cx.titlebar_inset() + 12.0)
        .child(text(label).color(text_color()));
    let pane = chrome_view::area(model, cx);
    col().w_full().h_full().fill(tab_bg).child(tab).child(pane)
}

fn tab_label(model: &Model) -> String {
    let title = model
        .tabs
        .get(model.active)
        .map(|tab| tab.title.as_str())
        .unwrap_or("rune-term");
    let bell = model.surfaces.primary().is_some_and(|host| host.bell);
    if bell {
        format!("{title} •")
    } else {
        title.to_string()
    }
}
