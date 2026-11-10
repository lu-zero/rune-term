//! Tabs in one window. Each tab is one shell. The strip selects one,
//! and that shell is the pane keys and the grid follow.

use librio::SurfaceId;
use rune::Tx;

use crate::chrome::{Leaf, LeafId, Node, Tab};
use crate::delegate;
use crate::model::Model;

/// A new shell, selected. The leaf fills the window.
pub fn open(model: &mut Model) -> bool {
    let (leaf_n, mark) = next_ids(model);
    let id = LeafId(leaf_n);
    model.tabs.push(Tab {
        title: "rune-term".to_string(),
        root: Node::Leaf(Leaf {
            id,
            surface: 0,
            mark,
            cols: 80,
            rows: 24,
        }),
        focused: id,
    });
    model.active = model.tabs.len() - 1;
    let surface = delegate::open_host(model, mark, 0, 0);
    if let Some(leaf) = find_leaf_mut(&mut model.tabs[model.active].root, id) {
        leaf.surface = surface;
    }
    model.surfaces.show(surface);
    true
}

/// Select `index`. False when it is missing or already selected.
pub fn select(model: &mut Model, index: usize) -> bool {
    if index >= model.tabs.len() || index == model.active {
        return false;
    }
    model.active = index;
    show_active(model);
    true
}

/// `1` is the first tab.
pub fn select_digit(model: &mut Model, digit: u8) -> bool {
    let Some(index) = usize::from(digit).checked_sub(1) else {
        return false;
    };
    select(model, index)
}

/// Move by `delta` tabs, wrapping. False when the selection does not move.
pub fn cycle(model: &mut Model, delta: i32) -> bool {
    let n = model.tabs.len();
    if n == 0 {
        return false;
    }
    let next = (model.active as i32 + delta).rem_euclid(n as i32) as usize;
    select(model, next)
}

/// Close the selected tab. The last one closes the window.
pub fn close_active(model: &mut Model, tx: &mut Tx) {
    close(model, model.active, tx);
}

/// Close `index`. The last one closes the window.
pub fn close(model: &mut Model, index: usize, tx: &mut Tx) {
    let Some(tab) = model.tabs.get(index) else {
        return;
    };
    let ids = surfaces_of(&tab.root);
    for id in ids {
        model.shared.remove_flag(id);
        model.surfaces.remove(id);
    }
    model.tabs.remove(index);
    if model.tabs.is_empty() {
        model.active = 0;
        tx.close_window();
        return;
    }
    if index < model.active {
        model.active -= 1;
    } else if model.active >= model.tabs.len() {
        model.active = model.tabs.len() - 1;
    }
    show_active(model);
}

/// Bring the selected tab back into the strip. Opening one past the
/// window's edge would leave its box off to the side.
pub fn reveal(model: &Model, tx: &mut Tx) {
    if model.tabs.is_empty() {
        return;
    }
    tx.scroll_into_view_in("tabs", format!("tab-{}", model.active));
}

/// Point the pane at the selected tab. A shell exit calls this too.
pub fn show_active(model: &mut Model) {
    let Some(id) = model.tabs.get(model.active).and_then(surface_of) else {
        return;
    };
    model.surfaces.show(id);
}

/// The shell this tab shows.
pub fn surface_of(tab: &Tab) -> Option<SurfaceId> {
    focused_surface(tab).filter(|id| *id != 0)
}

/// OSC title lands on the tab that owns the shell.
pub fn set_title(model: &mut Model, surface: SurfaceId, title: String) {
    for tab in &mut model.tabs {
        if contains(&tab.root, surface) {
            tab.title = title;
            return;
        }
    }
}

fn next_ids(model: &Model) -> (u32, u32) {
    let mut leaf = 0;
    let mut mark = 0;
    for tab in &model.tabs {
        walk(&tab.root, &mut |one| {
            leaf = leaf.max(one.id.0);
            mark = mark.max(one.mark);
        });
    }
    (leaf.saturating_add(1), mark.saturating_add(1))
}

fn focused_surface(tab: &Tab) -> Option<SurfaceId> {
    surface_at(&tab.root, tab.focused).or_else(|| first_surface(&tab.root))
}

fn surface_at(node: &Node, id: LeafId) -> Option<SurfaceId> {
    match node {
        Node::Leaf(leaf) if leaf.id == id => Some(leaf.surface),
        Node::Leaf(_) => None,
        Node::Split { left, right, .. } => surface_at(left, id).or_else(|| surface_at(right, id)),
    }
}

fn first_surface(node: &Node) -> Option<SurfaceId> {
    match node {
        Node::Leaf(leaf) => Some(leaf.surface),
        Node::Split { left, .. } => first_surface(left),
    }
}

fn contains(node: &Node, surface: SurfaceId) -> bool {
    match node {
        Node::Leaf(leaf) => leaf.surface == surface,
        Node::Split { left, right, .. } => contains(left, surface) || contains(right, surface),
    }
}

fn surfaces_of(node: &Node) -> Vec<SurfaceId> {
    let mut ids = Vec::new();
    walk(node, &mut |leaf| {
        if leaf.surface != 0 {
            ids.push(leaf.surface);
        }
    });
    ids
}

fn walk(node: &Node, f: &mut dyn FnMut(&Leaf)) {
    match node {
        Node::Leaf(leaf) => f(leaf),
        Node::Split { left, right, .. } => {
            walk(left, f);
            walk(right, f);
        }
    }
}

fn find_leaf_mut(node: &mut Node, id: LeafId) -> Option<&mut Leaf> {
    match node {
        Node::Leaf(leaf) if leaf.id == id => Some(leaf),
        Node::Leaf(_) => None,
        Node::Split { left, right, .. } => {
            find_leaf_mut(left, id).or_else(|| find_leaf_mut(right, id))
        }
    }
}

#[cfg(all(test, not(feature = "pty")))]
mod tests {
    use super::*;
    use crate::delegate;
    use crate::model;

    fn host() -> Model {
        let mut model = model::init();
        delegate::setup(&mut model, std::sync::Arc::new(|| {}));
        model
    }

    fn shown(model: &Model) -> SurfaceId {
        model.surfaces.shown_id().expect("shown surface")
    }

    fn active_surface(model: &Model) -> SurfaceId {
        surface_of(&model.tabs[model.active]).expect("active tab surface")
    }

    #[test]
    fn open_adds_and_selects_a_new_tab_with_its_surface() {
        let mut model = host();
        assert_eq!(model.tabs.len(), 1);
        let first = shown(&model);
        assert!(open(&mut model));
        assert_eq!(model.tabs.len(), 2);
        assert_eq!(model.active, 1);
        let second = shown(&model);
        assert_ne!(second, first);
        assert_eq!(active_surface(&model), second);
        assert_eq!(model.surfaces.len(), 2);
    }

    #[test]
    fn select_switches_active_tab_and_shown_surface() {
        let mut model = host();
        open(&mut model);
        open(&mut model);
        let s0 = surface_of(&model.tabs[0]).unwrap();
        let s1 = surface_of(&model.tabs[1]).unwrap();
        let s2 = surface_of(&model.tabs[2]).unwrap();
        assert_eq!(shown(&model), s2);
        assert!(!select(&mut model, 2));
        assert!(select(&mut model, 0));
        assert_eq!(model.active, 0);
        assert_eq!(shown(&model), s0);
        assert!(select(&mut model, 1));
        assert_eq!(shown(&model), s1);
        assert!(!select(&mut model, 99));
    }

    #[test]
    fn select_digit_is_one_based_and_cycle_wraps() {
        let mut model = host();
        open(&mut model);
        open(&mut model);
        assert!(!select_digit(&mut model, 0));
        assert!(select_digit(&mut model, 1));
        assert_eq!(model.active, 0);
        assert!(select_digit(&mut model, 3));
        assert_eq!(model.active, 2);
        assert!(cycle(&mut model, 1));
        assert_eq!(model.active, 0);
        assert!(cycle(&mut model, -1));
        assert_eq!(model.active, 2);
        assert_eq!(shown(&model), surface_of(&model.tabs[2]).unwrap());
    }

    #[test]
    fn closing_a_tab_before_active_keeps_selection_and_surface() {
        let mut model = host();
        open(&mut model);
        open(&mut model);
        let s1 = surface_of(&model.tabs[1]).unwrap();
        let s2 = surface_of(&model.tabs[2]).unwrap();
        assert_eq!(model.active, 2);
        close(&mut model, 0, &mut Tx::default());
        assert_eq!(model.tabs.len(), 2);
        assert_eq!(model.active, 1);
        assert_eq!(surface_of(&model.tabs[0]).unwrap(), s1);
        assert_eq!(shown(&model), s2);
    }

    #[test]
    fn closing_active_tab_selects_a_valid_tab_and_shows_it() {
        let mut model = host();
        open(&mut model);
        open(&mut model);
        let s0 = surface_of(&model.tabs[0]).unwrap();
        let s2 = surface_of(&model.tabs[2]).unwrap();
        assert!(select(&mut model, 1));
        close(&mut model, 1, &mut Tx::default());
        assert_eq!(model.tabs.len(), 2);
        assert!(model.active < model.tabs.len());
        assert_eq!(shown(&model), active_surface(&model));
        assert!(shown(&model) == s0 || shown(&model) == s2);
        // closing the last remaining in this scenario keeps a valid shown surface
        close_active(&mut model, &mut Tx::default());
        assert_eq!(model.tabs.len(), 1);
        assert_eq!(model.active, 0);
        assert_eq!(shown(&model), active_surface(&model));
    }

    #[test]
    fn show_active_resyncs_shown_surface_to_focused_tab() {
        let mut model = host();
        open(&mut model);
        let s0 = surface_of(&model.tabs[0]).unwrap();
        model.active = 0;
        model.surfaces.show(surface_of(&model.tabs[1]).unwrap());
        assert_ne!(shown(&model), s0);
        show_active(&mut model);
        assert_eq!(shown(&model), s0);
    }

    #[test]
    fn set_title_updates_the_owning_tab() {
        let mut model = host();
        open(&mut model);
        let s1 = surface_of(&model.tabs[1]).unwrap();
        set_title(&mut model, s1, "other".to_string());
        assert_eq!(model.tabs[0].title, "rune-term");
        assert_eq!(model.tabs[1].title, "other");
    }
}
