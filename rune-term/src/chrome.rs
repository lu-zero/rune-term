//! One tab strip and one pane. The cell tree is [`crate::split`].

use rune::{col, effect, row, text, Cx, El, Tx};

use crate::app;
use crate::ime;
use crate::model::Model;
use crate::pane_input;
use crate::selection;
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
    let scale = cx.env().scale;
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
    let pane = effect(PANE_MARK)
        .key("pane")
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .focusable(true)
        .on_layout_tx(move |model: &mut Model, rect, tx: &mut Tx| {
            app::on_pane_layout(model, (rect[2], rect[3]), scale, tx);
        })
        .on_physical_input(pane_input::on_physical)
        .on_paste(selection::on_paste)
        .on_wheel(move |model, event, tx| pane_input::on_wheel(model, event, scale, tx))
        .pointer_buttons([rune::PointerButton::Middle])
        .on_drag(move |model, drag, tx| pane_input::on_drag(model, drag, scale, tx))
        .on_secondary_press_local(move |model, point, tx| {
            pane_input::on_secondary(model, point, scale, tx)
        })
        .on_pointer_move_local(move |model, point, tx| {
            pane_input::on_pointer_move(model, point, scale, tx)
        });
    let pane = ime::mount_caret(pane);
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
