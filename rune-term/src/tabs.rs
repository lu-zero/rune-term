//! Tabs in one window. Each tab is one shell. The strip selects one,
//! and that shell is the pane keys and the grid follow.

use std::ops::Range;

use librio::SurfaceId;
use rune::Tx;

use crate::chrome::{Leaf, LeafId, Node, Tab, TabGroupId};
use crate::closing::{self, Close};
use crate::delegate;
use crate::model::Model;

/// A new shell, selected. The leaf fills the window.
pub fn open(model: &mut Model) -> bool {
    let profile = model.config.default_profile_name().to_owned();
    open_with_profile(model, &profile)
}

/// A new shell using `profile`, selected. The leaf fills the window.
/// Unknown profiles, or profiles whose backend is unavailable, fall back
/// to the default profile.
pub fn open_with_profile(model: &mut Model, profile: &str) -> bool {
    let mut profile = if model.config.profile(profile).is_some() {
        profile.to_owned()
    } else {
        model.config.default_profile_name().to_owned()
    };
    if let Err(err) = model.config.validate_backend(&profile) {
        eprintln!("rune-term: profile '{profile}': {err}; using default profile");
        profile = model.config.default_profile_name().to_owned();
    }
    apply_profile_for_new_tab(model, &profile);
    if crate::mux::open_profile(model, &profile) {
        return true;
    }
    let (leaf_n, mark) = next_ids(model);
    let id = LeafId(leaf_n);
    let tab_profile = profile.clone();
    model.tabs.push(Tab {
        title: "rune-term".to_string(),
        profile: tab_profile,
        root: Node::Leaf(Leaf {
            id,
            surface: 0,
            mark,
            cols: 80,
            rows: 24,
        }),
        focused: id,
        group: None,
        window: None,
        fit: None,
    });
    model.active = model.tabs.len() - 1;
    let surface = delegate::open_host(model, mark, 0, 0, Some(&profile));
    if let Some(leaf) = find_leaf_mut(&mut model.tabs[model.active].root, id) {
        leaf.surface = surface;
    }
    model.terminal.show(surface);
    true
}

fn apply_profile_for_new_tab(model: &mut Model, profile: &str) {
    let resolved = model.config.resolve_profile(profile);
    crate::metrics::apply_font(model, &resolved);
    if resolved.theme != model.palette.name {
        model.palette = model.config.resolve_palette_for(profile);
        model.terminal.invalidate_theme();
    }
}

/// Select `index`. False when it is missing or already selected.
pub fn select(model: &mut Model, index: usize) -> bool {
    if index >= model.tabs.len() || index == model.active {
        return false;
    }
    model.active = index;
    apply_active_profile(model);
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

/// A session's pane splits in the session; any other in this tab.
pub fn split(model: &mut Model, axis: crate::split::Axis, tx: &mut Tx) {
    if crate::mux::split(model, axis) {
        return;
    }
    if crate::chrome_view::split_focused(model, axis) {
        show_active(model);
        focus_active(model, tx);
    }
}

/// Whether the selected tab holds more than one pane of its own; a session's
/// panes are the session's.
pub fn has_panes(model: &Model) -> bool {
    model
        .tabs
        .get(model.active)
        .is_some_and(|tab| tab.group.is_none() && matches!(tab.root, Node::Split { .. }))
}

/// Close the focused pane, or the tab when it has no other.
pub fn close_pane(model: &mut Model, tx: &mut Tx) {
    let index = model.active;
    let close = if crate::mux::kills(model, index) {
        crate::mux::pane_close(model)
    } else if has_panes(model) {
        focused_surface(&model.tabs[index]).map(Close::Pane)
    } else {
        None
    };
    if let Some(close) = close.or_else(|| closing::tab(model, index)) {
        closing::request(model, close, tx);
    }
}

/// The tab's close button: a session's window is killed unless its profile
/// keeps it, hidden; any other tab closes.
pub fn close_tab(model: &mut Model, index: usize, tx: &mut Tx) {
    let close = crate::mux::kills(model, index)
        .then(|| crate::mux::window_close(model, index))
        .flatten();
    if let Some(close) = close.or_else(|| closing::tab(model, index)) {
        closing::request(model, close, tx);
    }
}

/// The tab menu's Close Window: a session's window ends, whatever its profile says.
pub fn kill_window(model: &mut Model, index: usize, tx: &mut Tx) {
    if let Some(close) = crate::mux::window_close(model, index) {
        closing::request(model, close, tx);
    }
}

/// Close `index`. The last one closes the window.
pub fn close(model: &mut Model, index: usize, tx: &mut Tx) {
    let Some(tab) = model.tabs.get(index) else {
        return;
    };
    let group = tab.group;
    crate::mux::hide(model, index);
    let ids = surfaces_of(&model.tabs[index].root);
    for id in ids {
        model.shared.forget(id);
        model.terminal.remove(id);
    }
    model.tabs.remove(index);
    if let Some(gid) = group {
        prune_group(model, gid);
    }
    if model.tabs.is_empty() {
        model.active = 0;
        model.tab_groups.clear();
        tx.close_window();
        return;
    }
    if index < model.active {
        model.active -= 1;
    } else if model.active >= model.tabs.len() {
        model.active = model.tabs.len() - 1;
    }
    show_active(model);
    focus_active(model, tx);
}

/// Remove a group entry when no tabs belong to it. A session's group stays
/// while every window is hidden.
pub fn prune_group(model: &mut Model, group: crate::chrome::TabGroupId) {
    let attached = model.muxes.iter().any(|mux| mux.group == group);
    if !attached && !model.tabs.iter().any(|t| t.group == Some(group)) {
        model.tab_groups.retain(|g| g.id != group);
    }
}

/// A drag along the strip. `group` is the group whose tabs it reorders, or `None` for the
/// strip's blocks; `from` and `to` count within that.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct StripDrag {
    pub group: Option<TabGroupId>,
    pub from: usize,
    pub to: usize,
    pub dx: f32,
}

/// What the strip moves as one: a group with its tabs, or a tab in no group.
pub(crate) fn blocks(tabs: &[Tab]) -> Vec<Range<usize>> {
    let mut blocks: Vec<Range<usize>> = Vec::new();
    for (index, tab) in tabs.iter().enumerate() {
        match blocks.last_mut() {
            Some(last) if tab.group.is_some() && tabs[last.start].group == tab.group => {
                last.end = index + 1;
            }
            _ => blocks.push(index..index + 1),
        }
    }
    blocks
}

/// Where an item dragged `dx` points from slot `from` lands: past a neighbour once it has
/// covered half of it.
pub(crate) fn landing(widths: &[f32], from: usize, dx: f32) -> usize {
    let (mut to, mut travel) = (from, dx);
    while to + 1 < widths.len() && travel > widths[to + 1] / 2.0 {
        travel -= widths[to + 1];
        to += 1;
    }
    while to > 0 && travel < -widths[to - 1] / 2.0 {
        travel += widths[to - 1];
        to -= 1;
    }
    to
}

/// Move block `from` to `to`. The selected tab stays selected.
pub(crate) fn move_block(model: &mut Model, from: usize, to: usize) {
    let mut blocks = blocks(&model.tabs);
    if from >= blocks.len() || to >= blocks.len() {
        return;
    }
    let moved = blocks.remove(from);
    blocks.insert(to, moved);
    reorder(model, blocks.into_iter().flatten().collect());
}

/// Move the tab at `from` among `group`'s tabs to `to`.
pub(crate) fn move_in_group(model: &mut Model, group: TabGroupId, from: usize, to: usize) {
    let Some(range) = blocks(&model.tabs)
        .into_iter()
        .find(|range| model.tabs[range.start].group == Some(group))
    else {
        return;
    };
    let mut inside: Vec<usize> = range.clone().collect();
    if from >= inside.len() || to >= inside.len() {
        return;
    }
    let moved = inside.remove(from);
    inside.insert(to, moved);
    let order = (0..range.start)
        .chain(inside)
        .chain(range.end..model.tabs.len())
        .collect();
    reorder(model, order);
}

/// `order` lists the old index of each tab in its new place.
fn reorder(model: &mut Model, order: Vec<usize>) {
    let mut old: Vec<Option<Tab>> = std::mem::take(&mut model.tabs)
        .into_iter()
        .map(Some)
        .collect();
    model.tabs = order
        .iter()
        .map(|index| old[*index].take().expect("each tab once"))
        .collect();
    model.active = order
        .iter()
        .position(|index| *index == model.active)
        .unwrap_or(0);
}

/// Bring the selected tab back into the strip and focus its pane. Opening one
/// past the window's edge would leave its box off to the side.
pub fn reveal(model: &Model, tx: &mut Tx) {
    if model.tabs.is_empty() {
        return;
    }
    tx.scroll_into_view_in("tabs", format!("tab-{}", model.active));
    focus_active(model, tx);
}

/// Point the pane at the selected tab. A shell exit calls this too.
pub fn show_active(model: &mut Model) {
    apply_active_profile(model);
    let Some(id) = model.tabs.get(model.active).and_then(surface_of) else {
        return;
    };
    model.terminal.show(id);
}

pub(crate) fn apply_active_profile(model: &mut Model) {
    let Some(profile) = model.tabs.get(model.active).map(|t| t.profile.clone()) else {
        return;
    };
    let resolved = model.config.resolve_profile(&profile);
    crate::metrics::apply_font(model, &resolved);
    if resolved.theme != model.palette.name {
        model.palette = model.config.resolve_palette_for(&profile);
        model.terminal.invalidate_theme();
    }
}

/// Focus the active tab's focused pane element, so keyboard input lands there.
pub fn focus_active(model: &Model, tx: &mut Tx) {
    let Some(tab) = model.tabs.get(model.active) else {
        return;
    };
    if let Node::Leaf(_) = tab.root {
        tx.focus(crate::chrome_view::SINGLE_PANE, false);
        return;
    }
    let Some(mark) = mark_at(&tab.root, tab.focused).or_else(|| first_mark(&tab.root)) else {
        return;
    };
    tx.focus(format!("pane-{mark}"), false);
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

pub(crate) fn next_ids(model: &Model) -> (u32, u32) {
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

fn mark_at(node: &Node, id: LeafId) -> Option<u32> {
    match node {
        Node::Leaf(leaf) if leaf.id == id => Some(leaf.mark),
        Node::Leaf(_) => None,
        Node::Split { left, right, .. } => mark_at(left, id).or_else(|| mark_at(right, id)),
    }
}

fn first_mark(node: &Node) -> Option<u32> {
    match node {
        Node::Leaf(leaf) => Some(leaf.mark),
        Node::Split { left, .. } => first_mark(left),
    }
}

fn contains(node: &Node, surface: SurfaceId) -> bool {
    match node {
        Node::Leaf(leaf) => leaf.surface == surface,
        Node::Split { left, right, .. } => contains(left, surface) || contains(right, surface),
    }
}

pub(crate) fn surfaces_of(node: &Node) -> Vec<SurfaceId> {
    let mut ids = Vec::new();
    walk(node, &mut |leaf| {
        if leaf.surface != 0 {
            ids.push(leaf.surface);
        }
    });
    ids
}

pub(crate) fn walk(node: &Node, f: &mut dyn FnMut(&Leaf)) {
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
        model.terminal.shown_id().expect("shown surface")
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
        assert_eq!(model.terminal.len(), 2);
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
        let active = model.active;
        close(&mut model, active, &mut Tx::default());
        assert_eq!(model.tabs.len(), 1);
        assert_eq!(model.active, 0);
    }

    #[test]
    fn open_with_profile_records_profile_and_falls_back_for_unknown() {
        let mut model = host();
        model.config.ensure_profile("Work").font_size = Some(16.0);
        assert!(open_with_profile(&mut model, "Work"));
        assert_eq!(model.tabs[model.active].profile, "Work");

        assert!(open_with_profile(&mut model, "Missing"));
        assert_eq!(
            model.tabs[model.active].profile,
            model.config.default_profile_name()
        );
    }

    #[test]
    fn open_with_unavailable_backend_falls_back_to_default() {
        let mut model = host();
        model.config.ensure_profile("Remote").backend = "ssh".to_owned();
        assert!(open_with_profile(&mut model, "Remote"));
        assert_eq!(
            model.tabs[model.active].profile,
            model.config.default_profile_name()
        );
    }

    #[test]
    fn show_active_resyncs_shown_surface_to_focused_tab() {
        let mut model = host();
        open(&mut model);
        let s0 = surface_of(&model.tabs[0]).unwrap();
        model.active = 0;
        model.terminal.show(surface_of(&model.tabs[1]).unwrap());
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
