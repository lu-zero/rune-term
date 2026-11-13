//! One effect and one surface per leaf. A single leaf fills the window.
//! A split gives each leaf an explicit size from the cell tree.

use std::mem;

use rune::{col, effect, row, Cx, El, Tx};

use crate::app;
use crate::chrome::{self, Leaf, LeafId, Node, PANE_MARK};
use crate::delegate;
use crate::ime;
use crate::model::Model;
use crate::pane_input;
use crate::resize::{self, GridMetrics, ResizeRequest};
use crate::scrollback;
use crate::search;
use crate::split::Axis;

pub fn area(model: &Model, cx: &mut Cx<Model>) -> El {
    let scale = cx.env().scale;
    let root = model.tabs.get(model.active).map(|tab| &tab.root);
    let pane = match root {
        Some(Node::Split { .. }) => split_area(model, scale),
        _ => single(model, scale),
    };
    search::mount(model, pane)
}

/// The surface created at startup belongs to the one leaf.
pub fn attach_initial(model: &mut Model, id: librio::SurfaceId) {
    let Some(tab) = model.tabs.get_mut(model.active) else {
        return;
    };
    if let Node::Leaf(leaf) = &mut tab.root {
        leaf.surface = id;
    }
}

/// Split the focused leaf. The new leaf is the right or bottom one.
/// No chord calls this yet. The non-pty test does.
#[cfg_attr(any(not(test), feature = "pty"), allow(dead_code))]
pub fn split_focused(model: &mut Model, axis: Axis) -> bool {
    let Some(active) = model.tabs.get(model.active).map(|_| model.active) else {
        return false;
    };
    let focused = model.tabs[active].focused;
    adopt_host_size(model, focused);
    let new_id = LeafId(max_leaf(&model.tabs[active].root).saturating_add(1));
    let new_mark = max_mark(&model.tabs[active].root).saturating_add(1);
    let other = Leaf {
        id: new_id,
        surface: 0,
        mark: new_mark,
        cols: 1,
        rows: 1,
    };
    let root = mem::replace(&mut model.tabs[active].root, lone(new_id));
    let root = match split_at(root, focused, axis, other) {
        Ok(root) => root,
        Err(root) => {
            model.tabs[active].root = root;
            return false;
        }
    };
    model.tabs[active].root = root;
    let Some((cols, rows, mark)) = leaf_shape(&model.tabs[active].root, new_id) else {
        return false;
    };
    let profile = model.tabs.get(active).map(|t| t.profile.clone());
    let surface = delegate::open_host(model, mark, cols, rows, profile.as_deref());
    if let Some(leaf) = find_leaf_mut(&mut model.tabs[active].root, new_id) {
        leaf.surface = surface;
    }
    true
}

/// Returns whether focus moved. Also focuses the pane element in Rune.
pub fn focus_leaf(model: &mut Model, id: LeafId, tx: &mut Tx) -> bool {
    let Some(active) = model.tabs.get(model.active).map(|_| model.active) else {
        return false;
    };
    let Some(mark) = leaf_mark(&model.tabs[active].root, id) else {
        return false;
    };
    let (moved, surface) = {
        let tab = &mut model.tabs[active];
        if !contains(&tab.root, id) {
            return false;
        }
        let moved = tab.focused != id;
        tab.focused = id;
        (moved, leaf_surface(&tab.root, id))
    };
    if let Some(surface) = surface.filter(|sid| *sid != 0) {
        model.terminal.surfaces.show(surface);
    }
    tx.focus(format!("pane-{mark}"), false);
    moved
}

fn leaf_mark(node: &Node, id: LeafId) -> Option<u32> {
    match node {
        Node::Leaf(leaf) if leaf.id == id => Some(leaf.mark),
        Node::Leaf(_) => None,
        Node::Split { left, right, .. } => leaf_mark(left, id).or_else(|| leaf_mark(right, id)),
    }
}

fn single(model: &Model, scale: f32) -> El {
    let hover_active = model.terminal.primary().is_some_and(|host| {
        host.hover_link.is_some() && crate::hyperlink::open_mods_rune(model.terminal.held_mods)
    });
    let mut pane = effect(PANE_MARK)
        .key("pane")
        .grow(1.0)
        .min_w(0.0)
        .h_full()
        .focusable(true)
        .on_layout_tx(move |model: &mut Model, rect, tx: &mut Tx| {
            app::on_pane_layout(model, (rect[2], rect[3]), scale, tx);
        });
    if hover_active {
        pane = pane.cursor(rune::Cursor::Pointer);
    }
    let cell_px = model
        .terminal
        .surfaces
        .primary()
        .map(|host| host.cell_px)
        .unwrap_or((0, 0));
    col()
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .min_w(0.0)
        .child(ime::mount_caret(
            wire(pane, PANE_MARK, scale),
            model.terminal.cursor_cell,
            cell_px,
            scale,
        ))
        .child(scrollback::column(scrollback::SHOWN_PANE).pin(
            None,
            Some(0.0),
            Some(0.0),
            Some(0.0),
        ))
}

fn split_area(model: &Model, scale: f32) -> El {
    let Some(root) = model.tabs.get(model.active).map(|tab| &tab.root) else {
        return single(model, scale);
    };
    let cell = model.cell_points;
    col()
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .on_layout_tx(move |model: &mut Model, rect, tx: &mut Tx| {
            layout_tree(model, (rect[2], rect[3]), scale, tx);
        })
        .child(nodes(root, model, scale, cell))
}

fn nodes(node: &Node, model: &Model, scale: f32, cell: (f32, f32)) -> El {
    match node {
        Node::Leaf(leaf) => leaf_el(leaf, model, scale, cell),
        Node::Split {
            axis: Axis::Horizontal,
            left,
            right,
        } => row()
            .w(span(node_cells(node).0, cell.0, scale))
            .h_full()
            .child(nodes(left, model, scale, cell))
            .child(divider(true, scale, cell, model.palette.as_ref()))
            .child(nodes(right, model, scale, cell)),
        Node::Split {
            axis: Axis::Vertical,
            left,
            right,
        } => col()
            .h(span(node_cells(node).1, cell.1, scale))
            .w_full()
            .child(nodes(left, model, scale, cell))
            .child(divider(false, scale, cell, model.palette.as_ref()))
            .child(nodes(right, model, scale, cell)),
    }
}

fn leaf_el(leaf: &Leaf, model: &Model, scale: f32, cell: (f32, f32)) -> El {
    let surface = leaf.surface;
    let id = leaf.id;
    let mark = leaf.mark;
    let hover_active = model.terminal.by_mark(mark).is_some_and(|host| {
        host.hover_link.is_some() && crate::hyperlink::open_mods_rune(model.terminal.held_mods)
    });
    let mut pane = effect(mark)
        .key(format!("pane-{mark}"))
        .grow(1.0)
        .min_w(0.0)
        .h_full()
        .focusable(true)
        .on_layout_tx(move |model: &mut Model, rect, tx: &mut Tx| {
            layout_leaf(model, surface, (rect[2], rect[3]), scale, tx);
        })
        .on_press_tx(move |model: &mut Model, tx: &mut Tx| {
            if !focus_leaf(model, id, tx) {
                tx.live_only();
            }
        });
    if hover_active {
        pane = pane.cursor(rune::Cursor::Pointer);
    }
    let focused = model
        .tabs
        .get(model.active)
        .is_some_and(|tab| tab.focused == id);
    let cursor_cell = if focused {
        model.terminal.cursor_cell
    } else {
        (0, 0)
    };
    let cell_px = model
        .terminal
        .surfaces
        .get(surface)
        .map(|host| host.cell_px)
        .unwrap_or((0, 0));
    let pane = col()
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .min_w(0.0)
        .child(ime::mount_caret(
            wire(pane, mark, scale),
            cursor_cell,
            cell_px,
            scale,
        ))
        .child(scrollback::column(mark).pin(None, Some(0.0), Some(0.0), Some(0.0)));
    row()
        .w(span(leaf.cols, cell.0, scale))
        .h(span(leaf.rows, cell.1, scale))
        .child(pane)
}

fn wire(pane: El, mark: u32, scale: f32) -> El {
    pane.on_physical_input(move |model, event, tx| {
        pane_input::on_physical_mark(model, mark, event, tx)
    })
    .on_wheel(move |model, event, tx| pane_input::on_wheel_mark(model, mark, event, scale, tx))
    .pointer_buttons([rune::PointerButton::Middle])
    .on_drag(move |model, drag, tx| pane_input::on_drag_mark(model, mark, drag, scale, tx))
    .on_secondary_press_local(move |model, point, tx| {
        pane_input::on_secondary_mark(model, mark, point, scale, tx)
    })
    .on_pointer_move_local(move |model, point, tx| {
        pane_input::on_pointer_move_mark(model, mark, point, scale, tx)
    })
}

fn divider(horizontal: bool, scale: f32, cell: (f32, f32), palette: &crate::theme::Palette) -> El {
    let thickness = resize::content_points(1, if horizontal { cell.0 } else { cell.1 }, scale);
    let gap = rune::el().fill(chrome::tab(palette));
    if horizontal {
        gap.w(thickness).h_full()
    } else {
        gap.h(thickness).w_full()
    }
}

/// `apply` subtracts the content margin, so the element holds the grid plus
/// that margin. The next floor then returns this leaf's cell count.
fn span(cells: u16, cell_points: f32, scale: f32) -> f32 {
    resize::content_points(cells, cell_points, scale) + resize::CONTENT_MARGIN_PT * 2.0
}

fn layout_tree(model: &mut Model, slot: (f32, f32), scale: f32, tx: &mut Tx) {
    if !(slot.0 > 0.0 && slot.1 > 0.0 && scale.is_finite() && scale > 0.0) {
        tx.live_only();
        return;
    }
    let cell_points = model.cell_points;
    let Some(tab) = model.tabs.get_mut(model.active) else {
        tx.live_only();
        return;
    };
    let (cols, rows) = node_cells(&tab.root);
    let effect = resize::apply(ResizeRequest::Layout {
        slot_points: slot,
        scale,
        cell_points,
        current: GridMetrics {
            cols,
            rows,
            cell_px: (0, 0),
        },
    });
    let next = crate::split::distribute(effect.cols, effect.rows, tab.root.clone());
    let changed = next != tab.root;
    tab.root = next;
    if !changed {
        tx.live_only();
    }
}

fn layout_leaf(
    model: &mut Model,
    surface: librio::SurfaceId,
    slot: (f32, f32),
    scale: f32,
    tx: &mut Tx,
) {
    let cell_points = model.cell_points;
    let Some(host) = model.terminal.surfaces.get_mut(surface) else {
        tx.live_only();
        return;
    };
    let effect = resize::apply(ResizeRequest::Layout {
        slot_points: slot,
        scale,
        cell_points,
        current: GridMetrics {
            cols: host.cols,
            rows: host.rows,
            cell_px: host.cell_px,
        },
    });
    if effect.resize_surface {
        host.cols = effect.cols;
        host.rows = effect.rows;
        host.cell_px = effect.cell_px;
    }
    app::perform_resize(tx, host, effect);
}

#[cfg_attr(any(not(test), feature = "pty"), allow(dead_code))]
fn adopt_host_size(model: &mut Model, id: LeafId) {
    let active = model.active;
    let Some(surface) = model
        .tabs
        .get(active)
        .and_then(|tab| leaf_surface(&tab.root, id))
    else {
        return;
    };
    let Some(host) = model.terminal.surfaces.get(surface) else {
        return;
    };
    if host.cols == 0 || host.rows == 0 {
        return;
    }
    let (cols, rows) = (host.cols, host.rows);
    let Some(tab) = model.tabs.get_mut(active) else {
        return;
    };
    if let Some(leaf) = find_leaf_mut(&mut tab.root, id) {
        leaf.cols = cols;
        leaf.rows = rows;
    }
}

/// Same sum as the split tree: a divider costs one cell.
fn node_cells(node: &Node) -> (u16, u16) {
    match node {
        Node::Leaf(leaf) => (leaf.cols, leaf.rows),
        Node::Split {
            axis: Axis::Horizontal,
            left,
            right,
        } => {
            let (left_cols, left_rows) = node_cells(left);
            let (right_cols, right_rows) = node_cells(right);
            (
                left_cols.saturating_add(1).saturating_add(right_cols),
                left_rows.max(right_rows),
            )
        }
        Node::Split {
            axis: Axis::Vertical,
            left,
            right,
        } => {
            let (left_cols, left_rows) = node_cells(left);
            let (right_cols, right_rows) = node_cells(right);
            (
                left_cols.max(right_cols),
                left_rows.saturating_add(1).saturating_add(right_rows),
            )
        }
    }
}

#[cfg_attr(any(not(test), feature = "pty"), allow(dead_code))]
fn split_at(node: Node, id: LeafId, axis: Axis, other: Leaf) -> Result<Node, Node> {
    match node {
        Node::Leaf(leaf) if leaf.id == id => {
            let fits = match axis {
                Axis::Horizontal => leaf.cols >= 3,
                Axis::Vertical => leaf.rows >= 3,
            };
            if !fits {
                return Err(Node::Leaf(leaf));
            }
            Ok(Node::Leaf(leaf)
                .split(axis, other)
                .expect("the leaf has room for a divider"))
        }
        Node::Leaf(leaf) => Err(Node::Leaf(leaf)),
        Node::Split {
            axis: parent,
            left,
            right,
        } => {
            if contains(&left, id) {
                match split_at(*left, id, axis, other) {
                    Ok(left) => Ok(Node::Split {
                        axis: parent,
                        left: Box::new(left),
                        right,
                    }),
                    Err(left) => Err(Node::Split {
                        axis: parent,
                        left: Box::new(left),
                        right,
                    }),
                }
            } else if contains(&right, id) {
                match split_at(*right, id, axis, other) {
                    Ok(right) => Ok(Node::Split {
                        axis: parent,
                        left,
                        right: Box::new(right),
                    }),
                    Err(right) => Err(Node::Split {
                        axis: parent,
                        left,
                        right: Box::new(right),
                    }),
                }
            } else {
                Err(Node::Split {
                    axis: parent,
                    left,
                    right,
                })
            }
        }
    }
}

fn contains(node: &Node, id: LeafId) -> bool {
    match node {
        Node::Leaf(leaf) => leaf.id == id,
        Node::Split { left, right, .. } => contains(left, id) || contains(right, id),
    }
}

#[cfg_attr(any(not(test), feature = "pty"), allow(dead_code))]
fn find_leaf_mut(node: &mut Node, id: LeafId) -> Option<&mut Leaf> {
    match node {
        Node::Leaf(leaf) if leaf.id == id => Some(leaf),
        Node::Leaf(_) => None,
        Node::Split { left, right, .. } => {
            find_leaf_mut(left, id).or_else(|| find_leaf_mut(right, id))
        }
    }
}

#[cfg_attr(any(not(test), feature = "pty"), allow(dead_code))]
fn leaf_shape(node: &Node, id: LeafId) -> Option<(u16, u16, u32)> {
    match node {
        Node::Leaf(leaf) if leaf.id == id => Some((leaf.cols, leaf.rows, leaf.mark)),
        Node::Leaf(_) => None,
        Node::Split { left, right, .. } => leaf_shape(left, id).or_else(|| leaf_shape(right, id)),
    }
}

#[cfg_attr(any(not(test), feature = "pty"), allow(dead_code))]
fn leaf_surface(node: &Node, id: LeafId) -> Option<librio::SurfaceId> {
    match node {
        Node::Leaf(leaf) if leaf.id == id => Some(leaf.surface),
        Node::Leaf(_) => None,
        Node::Split { left, right, .. } => {
            leaf_surface(left, id).or_else(|| leaf_surface(right, id))
        }
    }
}

#[cfg_attr(any(not(test), feature = "pty"), allow(dead_code))]
fn max_leaf(node: &Node) -> u32 {
    match node {
        Node::Leaf(leaf) => leaf.id.0,
        Node::Split { left, right, .. } => max_leaf(left).max(max_leaf(right)),
    }
}

#[cfg_attr(any(not(test), feature = "pty"), allow(dead_code))]
fn max_mark(node: &Node) -> u32 {
    match node {
        Node::Leaf(leaf) => leaf.mark,
        Node::Split { left, right, .. } => max_mark(left).max(max_mark(right)),
    }
}

#[cfg_attr(any(not(test), feature = "pty"), allow(dead_code))]
fn lone(id: LeafId) -> Node {
    Node::Leaf(Leaf {
        id,
        surface: 0,
        mark: 0,
        cols: 1,
        rows: 1,
    })
}

#[cfg(all(test, not(feature = "pty")))]
mod tests {
    use super::*;
    use crate::delegate;
    use crate::model;
    use std::sync::atomic::Ordering;

    fn host() -> Model {
        let mut model = model::init();
        delegate::setup(&mut model, std::sync::Arc::new(|| {}));
        model
    }

    fn leaves(node: &Node) -> Vec<&Leaf> {
        match node {
            Node::Leaf(leaf) => vec![leaf],
            Node::Split { left, right, .. } => {
                let mut all = leaves(left);
                all.extend(leaves(right));
                all
            }
        }
    }

    #[test]
    fn a_split_keeps_one_cell_for_the_divider_and_isolates_damage() {
        let mut model = host();
        assert_eq!(model.terminal.surfaces.len(), 1);
        let first = model.tabs[0].root.clone();
        let Node::Leaf(original) = first else {
            panic!("the first tab is one leaf");
        };
        assert_ne!(original.surface, 0);

        assert!(split_focused(&mut model, Axis::Horizontal));
        assert_eq!(model.terminal.surfaces.len(), 2);
        let (
            left_cols,
            right_cols,
            left_rows,
            right_rows,
            total,
            left_surface,
            right_surface,
            left_mark,
            right_mark,
            right_id,
        ) = {
            let tab = &model.tabs[0];
            let Node::Split {
                axis: Axis::Horizontal,
                left,
                right,
            } = &tab.root
            else {
                panic!("the tab is a horizontal split");
            };
            let left = leaves(left);
            let right = leaves(right);
            assert_eq!(left.len(), 1);
            assert_eq!(right.len(), 1);
            (
                left[0].cols,
                right[0].cols,
                left[0].rows,
                right[0].rows,
                node_cells(&tab.root),
                left[0].surface,
                right[0].surface,
                left[0].mark,
                right[0].mark,
                right[0].id,
            )
        };
        assert_eq!(left_cols, 40);
        assert_eq!(right_cols, 39);
        assert_eq!(left_rows, 24);
        assert_eq!(right_rows, 24);
        assert_eq!(total, (80, 24));
        assert_ne!(left_surface, right_surface);
        assert_ne!(left_mark, right_mark);

        assert!(focus_leaf(&mut model, right_id, &mut rune::Tx::default()));
        assert_eq!(model.tabs[0].focused, right_id);
        assert!(!focus_leaf(&mut model, right_id, &mut rune::Tx::default()));
        {
            let host = model.terminal.surfaces.get_mut(left_surface).unwrap();
            host.surface.inject_output(b"only-a\r\n");
            host.frame_flag.store(true, Ordering::Release);
        }
        assert!(model
            .terminal
            .surfaces
            .get(left_surface)
            .unwrap()
            .frame_flag
            .load(Ordering::Acquire));
        assert!(!model
            .terminal
            .surfaces
            .get(right_surface)
            .unwrap()
            .frame_flag
            .load(Ordering::Acquire));
    }
}
