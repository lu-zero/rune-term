//! One effect and one surface per leaf. A single leaf fills the window.
//! A split gives each leaf an explicit size from the cell tree.

use std::mem;

use rune::{col, effect, row, Cx, Drag, El, Phase, Tx};

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
    // A pane that leaves must not fade over the shell that replaced it, and a
    // split's leaves take their new sizes at once.
    let pane = match root {
        Some(Node::Split { .. }) => split_area(model, scale),
        _ => single(model, scale),
    }
    .instant_changes();
    search::mount(model, pane)
}

/// Each pane on screen: its effect mark and the surface it shows. A lone leaf
/// is the one pane at [`PANE_MARK`], showing the selected shell.
pub fn visible(model: &Model) -> Vec<(u32, librio::SurfaceId)> {
    let Some(root @ Node::Split { .. }) = model.tabs.get(model.active).map(|tab| &tab.root) else {
        return model
            .terminal
            .primary()
            .map(|host| vec![(PANE_MARK, host.surface.id())])
            .unwrap_or_default();
    };
    let mut panes = Vec::new();
    crate::tabs::walk(root, &mut |leaf| panes.push((leaf.mark, leaf.surface)));
    panes
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
        model.terminal.show(surface);
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

/// One key for every tab's lone pane. A key per tab would remount the pane on a
/// switch, and the old one would fade out over the new shell on the same slot.
pub const SINGLE_PANE: &str = "pane";

fn single(model: &Model, scale: f32) -> El {
    let hover_active = model.terminal.primary().is_some_and(|host| {
        host.hover_link.is_some() && crate::hyperlink::open_mods_rune(model.terminal.held_mods())
    });
    let mut pane = effect(PANE_MARK)
        .key(SINGLE_PANE)
        .grow(1.0)
        .min_w(0.0)
        .h_full()
        .focusable(true)
        // The cursor shows focus, as in a text field. Rune's default ring
        // would frame the whole pane in the system accent colour.
        .no_focus_ring()
        .on_layout_tx(move |model: &mut Model, rect, tx: &mut Tx| {
            app::on_pane_layout(model, (rect[2], rect[3]), scale, tx);
        });
    if hover_active {
        pane = pane.cursor(rune::Cursor::Pointer);
    }
    let cell_px = model.terminal.primary_cell_px();
    col()
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .min_w(0.0)
        .child(ime::mount_caret(
            wire(pane, scrollback::SHOWN_PANE, scale),
            model.terminal.cursor_cell(),
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
        .child(nodes(root, &mut Vec::new(), model, scale, cell))
}

/// `path` leads from the root to `node`: `true` is a right or bottom child.
fn nodes(node: &Node, path: &mut Vec<bool>, model: &Model, scale: f32, cell: (f32, f32)) -> El {
    let (axis, left, right) = match node {
        Node::Leaf(leaf) => return leaf_el(leaf, model, scale, cell),
        Node::Split { axis, left, right } => (axis, left, right),
    };
    let bar = divider(*axis, path.clone(), scale, cell, model.palette.as_ref());
    path.push(false);
    let first = nodes(left, path, model, scale, cell);
    path.pop();
    path.push(true);
    let second = nodes(right, path, model, scale, cell);
    path.pop();
    let (cols, rows) = node_cells(node);
    // Full width or height would be the parent split's, not this one's.
    let split = match axis {
        Axis::Horizontal => row(),
        Axis::Vertical => col(),
    }
    .w(span(cols, cell.0, scale))
    .h(span(rows, cell.1, scale));
    split.child(first).child(bar).child(second)
}

fn leaf_el(leaf: &Leaf, model: &Model, scale: f32, cell: (f32, f32)) -> El {
    let surface = leaf.surface;
    let id = leaf.id;
    let mark = leaf.mark;
    let hover_active = model.terminal.by_mark(mark).is_some_and(|host| {
        host.hover_link.is_some() && crate::hyperlink::open_mods_rune(model.terminal.held_mods())
    });
    let mut pane = effect(mark)
        .key(format!("pane-{mark}"))
        .grow(1.0)
        .min_w(0.0)
        .h_full()
        .focusable(true)
        // The cursor shows focus, as in a text field.
        .no_focus_ring()
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
        model.terminal.cursor_cell()
    } else {
        (0, 0)
    };
    let cell_px = model
        .terminal
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

/// One cell between two children, drawn as a hairline down its middle. The
/// whole cell takes the drag, which moves the split by whole cells.
fn divider(
    axis: Axis,
    path: Vec<bool>,
    scale: f32,
    cell: (f32, f32),
    palette: &crate::theme::Palette,
) -> El {
    let key: String = path
        .iter()
        .map(|right| if *right { 'r' } else { 'l' })
        .collect();
    let line = rune::el().fill(chrome::tab(palette));
    let gap = rune::el()
        .key(format!("divider-{key}"))
        .fill(chrome::slot(palette))
        .center()
        .on_drag(move |model: &mut Model, drag: &mut Drag, tx: &mut Tx| {
            drag_divider(model, &path, drag, tx);
        });
    match axis {
        Axis::Horizontal => gap
            .w(resize::content_points(1, cell.0, scale))
            .h_full()
            .cursor(rune::Cursor::ResizeColumn)
            .child(line.w(1.0).h_full()),
        Axis::Vertical => gap
            .h(resize::content_points(1, cell.1, scale))
            .w_full()
            .cursor(rune::Cursor::ResizeRow)
            .child(line.h(1.0).w_full()),
    }
}

/// The press records the cells on the divider's left or top. The travel since
/// then, in whole cells, sets them. tmux owns its windows' layout: a tmux
/// pane is resized there, and the tree follows the layout tmux reports.
fn drag_divider(model: &mut Model, path: &[bool], drag: &mut Drag, tx: &mut Tx) {
    let Some(tab) = model.tabs.get(model.active) else {
        return;
    };
    let Some((axis, cells, surface)) = crate::split::divider(&tab.root, path) else {
        return;
    };
    if drag.phase == Phase::Down {
        drag.data = f32::from(cells);
        return;
    }
    let (travel, pitch) = match axis {
        Axis::Horizontal => (drag.dx, model.cell_points.0),
        Axis::Vertical => (drag.dy, model.cell_points.1),
    };
    let target = (drag.data + (travel / pitch).round()).clamp(0.0, f32::from(u16::MAX)) as u16;
    if target == cells {
        tx.live_only();
        return;
    }
    if crate::mux::resize(model, surface, axis, target) {
        tx.live_only();
        return;
    }
    let tab = &mut model.tabs[model.active];
    tab.root = crate::split::move_divider(tab.root.clone(), path, target);
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
    if let Some(group) = tab.group.filter(|_| tab.window.is_some()) {
        crate::mux::fill(model, group, (effect.cols, effect.rows));
        tx.live_only();
        return;
    }
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
    let remote = crate::mux::owns(model, surface);
    let Some(host) = model.terminal.get_mut(surface) else {
        tx.live_only();
        return;
    };
    let mut effect = resize::apply(ResizeRequest::Layout {
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
    effect.resize_surface &= !remote;
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
    let Some(host) = model.terminal.get(surface) else {
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
        assert_eq!(model.terminal.len(), 1);
        let first = model.tabs[0].root.clone();
        let Node::Leaf(original) = first else {
            panic!("the first tab is one leaf");
        };
        assert_ne!(original.surface, 0);

        assert!(split_focused(&mut model, Axis::Horizontal));
        assert_eq!(model.terminal.len(), 2);
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
            let host = model.terminal.get_mut(left_surface).unwrap();
            host.surface.inject_output(b"only-a\r\n");
            host.frame_flag.store(true, Ordering::Release);
        }
        assert!(model
            .terminal
            .get(left_surface)
            .unwrap()
            .frame_flag
            .load(Ordering::Acquire));
        assert!(!model
            .terminal
            .get(right_surface)
            .unwrap()
            .frame_flag
            .load(Ordering::Acquire));
    }

    #[test]
    fn dragging_a_divider_moves_it_by_whole_cells() {
        let mut model = host();
        assert!(split_focused(&mut model, Axis::Horizontal));
        let mut rt = rune::Runtime::new(
            model,
            crate::chrome::view,
            (800.0, 500.0),
            rune::Env::new(true, 1.0),
        );
        rt.frame(0.0);
        rt.frame(0.0);
        let before = leaves(&rt.state.tabs[0].root)[0].cols;
        let total = node_cells(&rt.state.tabs[0].root);
        let bar = rt.presented("divider-").expect("the divider is drawn").0;
        let (x, y) = (bar[0] + bar[2] / 2.0, bar[1] + bar[3] / 2.0);
        let pitch = rt.state.cell_points.0;
        rt.pointer_down(x, y);
        rt.pointer_move(x + pitch * 5.2, y);
        rt.pointer_up(x + pitch * 5.2, y);
        rt.frame(0.0);
        assert_eq!(leaves(&rt.state.tabs[0].root)[0].cols, before + 5);
        assert_eq!(node_cells(&rt.state.tabs[0].root), total);
    }

    #[test]
    fn switching_between_a_split_tab_and_a_lone_pane_leaves_no_pane_behind() {
        let mut model = host();
        assert!(crate::tabs::open(&mut model));
        assert!(split_focused(&mut model, Axis::Horizontal));
        let mut rt = rune::Runtime::new(
            model,
            crate::chrome::view,
            (800.0, 500.0),
            rune::Env::new(true, 1.0),
        );
        rt.frame(0.0);
        for tab in ["tab-0", "tab-1", "tab-0"] {
            let (x, y) = rt.find(tab).expect("tab");
            rt.pointer_down(x, y);
            rt.pointer_up(x, y);
            rt.frame(0.0);
            let leaving: Vec<_> = rt
                .elements()
                .into_iter()
                .filter(|el| el.exiting)
                .filter_map(|el| el.key)
                .filter(|key| key.starts_with("pane") || key.starts_with("divider"))
                .collect();
            assert!(leaving.is_empty(), "after {tab}, still drawn: {leaving:?}");
        }
    }
}
