//! A tmux session in control mode as a tab group: a window is a tab and a pane
//! is a leaf. tmux owns the layout. rune-term reports its size in cells and
//! mirrors the layout tmux sends back.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::io::{self, Read, Write};
use std::process::{ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;

use librio::{Surface, SurfaceDesc, SurfaceId};
use rune::{PopupItem, PopupMenu, Tx};
use term_mux::tmux::{self, Update, Window};
use term_mux::Layout;

use crate::chrome::{Leaf, LeafId, Node, Tab, TabGroup, TabGroupId};
use crate::delegate::{self, Shared};
use crate::model::Model;
use crate::split::Axis;

static QUEUED: Mutex<Option<Vec<String>>> = Mutex::new(None);
/// Above any group a test or the strip made by hand.
static NEXT_GROUP: AtomicU32 = AtomicU32::new(1 << 16);

/// Output kept for a pane that has no surface yet. Past this it is dropped:
/// showing a hidden window redraws it from a capture.
const EARLY_CAP: usize = 1 << 20;

pub(crate) struct Mux {
    pub group: TabGroupId,
    input: mpsc::Sender<Vec<u8>>,
    panes: Arc<Mutex<Panes>>,
    windows: Vec<Window>,
    /// The surface of each pane in a shown window.
    surfaces: HashMap<u32, SurfaceId>,
    hidden: BTreeSet<u32>,
    /// The cells a split tab fills. A lone pane's surface has its own size.
    slot: Option<(u16, u16)>,
    reported: Option<(u16, u16)>,
    selected: Option<(u32, u32)>,
    /// The last `resize-pane` sent, so a drag does not repeat it on every move.
    resized: Option<(u32, Axis, u16)>,
}

impl Drop for Mux {
    fn drop(&mut self) {
        let _ = self.input.send(b"detach-client\n".to_vec());
    }
}

/// The reader injects output here, in tmux's order, so a capture and the
/// output after it reach the surface in the order tmux wrote them.
#[derive(Default)]
struct Panes {
    live: HashMap<u32, Arc<Surface>>,
    early: HashMap<u32, Vec<u8>>,
    /// Each live surface's size, which only tmux's layouts change.
    sizes: HashMap<u32, (u16, u16)>,
    cell_px: (u16, u16),
}

/// The arguments `setup` starts tmux with. They include `-C`.
pub(crate) fn queue(args: Vec<String>) {
    *QUEUED.lock().expect("tmux") = Some(args);
}

pub(crate) fn attach_queued(model: &mut Model) {
    let Some(args) = QUEUED.lock().expect("tmux").take() else {
        return;
    };
    if let Err(err) = attach(model, &args) {
        eprintln!("rune-term: tmux: {err}");
    }
}

pub(crate) fn attach(model: &mut Model, args: &[String]) -> io::Result<TabGroupId> {
    let mut child = Command::new("tmux")
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    let mut stdin = child.stdin.take().expect("piped stdin");
    let stdout = child.stdout.take().expect("piped stdout");
    let group = TabGroupId(NEXT_GROUP.fetch_add(1, Ordering::Relaxed));
    let (input, queued) = mpsc::channel::<Vec<u8>>();
    thread::Builder::new()
        .name(format!("tmux-write-{}", group.0))
        .spawn(move || {
            for bytes in queued {
                if stdin.write_all(&bytes).is_err() {
                    break;
                }
            }
        })?;
    let panes = Arc::new(Mutex::new(Panes::default()));
    let reader = (Arc::clone(&panes), Arc::clone(&model.shared), input.clone());
    thread::Builder::new()
        .name(format!("tmux-read-{}", group.0))
        .spawn(move || {
            let (panes, shared, input) = reader;
            read(stdout, &panes, &shared, &input, group);
            let _ = child.wait();
        })?;
    model.tab_groups.push(TabGroup {
        id: group,
        label: "tmux".to_owned(),
    });
    model.muxes.push(Mux {
        group,
        input,
        panes,
        windows: Vec::new(),
        surfaces: HashMap::new(),
        hidden: BTreeSet::new(),
        slot: None,
        reported: None,
        selected: None,
        resized: None,
    });
    Ok(group)
}

fn read(
    mut stdout: ChildStdout,
    panes: &Mutex<Panes>,
    shared: &Shared,
    input: &mpsc::Sender<Vec<u8>>,
    group: TabGroupId,
) {
    let mut client = tmux::Client::default();
    let mut buf = vec![0u8; 64 * 1024];
    let reason = loop {
        let n = match stdout.read(&mut buf) {
            Ok(0) => break None,
            Ok(n) => n,
            Err(err) if err.kind() == io::ErrorKind::Interrupted => continue,
            Err(err) => break Some(err.to_string()),
        };
        for update in client.feed(&buf[..n]) {
            match update {
                Update::Output { pane, bytes } => deliver(panes, shared, pane, &bytes),
                Update::Windows(windows) => {
                    follow_sizes(panes, shared, &windows);
                    shared.push_mux(group, Update::Windows(windows));
                }
                Update::Exit(reason) => {
                    shared.push_mux(group, Update::Exit(reason));
                    return;
                }
                other => shared.push_mux(group, other),
            }
        }
        let commands = client.take_commands();
        if !commands.is_empty() {
            let _ = input.send(commands.into_bytes());
        }
    };
    shared.push_mux(group, Update::Exit(reason));
}

/// Injected under the lock, so a surface the UI is making cannot get its
/// early output after later output.
fn deliver(panes: &Mutex<Panes>, shared: &Shared, pane: u32, bytes: &[u8]) {
    let mut panes = panes.lock().expect("panes");
    if let Some(surface) = panes.live.get(&pane) {
        surface.inject_output(bytes);
        shared.raise(surface.id());
        return;
    }
    let early = panes.early.entry(pane).or_default();
    if early.len() + bytes.len() > EARLY_CAP {
        early.clear();
    }
    early.extend_from_slice(bytes);
}

/// A pane takes tmux's size here, between the output before the layout and
/// the output after it, as tmux applied them.
fn follow_sizes(panes: &Mutex<Panes>, shared: &Shared, windows: &[Window]) {
    let mut panes = panes.lock().expect("panes");
    let (cell_w, cell_h) = panes.cell_px;
    for (pane, (cols, rows)) in windows
        .iter()
        .flat_map(|window| tmux::sizes(&window.layout))
    {
        let Some(surface) = panes.live.get(&pane).cloned() else {
            continue;
        };
        if panes.sizes.insert(pane, (cols, rows)) != Some((cols, rows)) {
            surface.resize(
                cols,
                rows,
                cols.saturating_mul(cell_w),
                rows.saturating_mul(cell_h),
            );
            shared.raise(surface.id());
        }
    }
}

/// Whether tmux sizes this surface. The layout must not resize it.
pub(crate) fn owns(model: &Model, surface: SurfaceId) -> bool {
    model
        .muxes
        .iter()
        .any(|mux| mux.surfaces.values().any(|id| *id == surface))
}

pub(crate) fn apply(model: &mut Model, group: TabGroupId, update: Update, tx: &mut Tx) -> bool {
    let Some(index) = model.muxes.iter().position(|mux| mux.group == group) else {
        return false;
    };
    match update {
        Update::Output { .. } => false,
        Update::Session(name) => {
            if let Some(entry) = model.tab_groups.iter_mut().find(|g| g.id == group) {
                entry.label = name;
            }
            true
        }
        Update::Windows(windows) => {
            follow(model, index, windows, tx);
            true
        }
        Update::Exit(reason) => {
            if let Some(reason) = reason {
                eprintln!("rune-term: tmux: {reason}");
            }
            end(model, index, tx);
            true
        }
    }
}

/// Mirror the windows. While the strip is on this session, or on its first
/// listing, the selected tab follows tmux's current window.
fn follow(model: &mut Model, index: usize, windows: Vec<Window>, tx: &mut Tx) {
    let group = model.muxes[index].group;
    let was = current(&model.muxes[index].windows);
    let first = model.muxes[index].windows.is_empty();
    let on_group = model
        .tabs
        .get(model.active)
        .is_some_and(|tab| tab.group == Some(group));
    let mux = &mut model.muxes[index];
    mux.hidden
        .retain(|id| windows.iter().any(|window| window.id == *id));
    mux.windows = windows;
    let now = current(&mux.windows);
    mux.selected = now;
    sync_tabs(model, index);
    if model.tabs.is_empty() {
        tx.close_window();
        return;
    }
    if (first || on_group) && now != was {
        if let Some(at) = now.and_then(|(window, _)| tab_of(model, group, window)) {
            model.active = at;
        }
    }
    crate::tabs::show_active(model);
    if model.tabs[model.active].group == Some(group) {
        crate::tabs::focus_active(model, tx);
    }
}

fn current(windows: &[Window]) -> Option<(u32, u32)> {
    windows
        .iter()
        .find(|window| window.active)
        .map(|window| (window.id, window.active_pane))
}

fn tab_of(model: &Model, group: TabGroupId, window: u32) -> Option<usize> {
    model
        .tabs
        .iter()
        .position(|tab| tab.group == Some(group) && tab.window == Some(window))
}

/// One tab per shown window, each tree rebuilt from tmux's layout. The
/// selected tab stays selected when tabs before it come or go.
fn sync_tabs(model: &mut Model, index: usize) {
    let group = model.muxes[index].group;
    let selected = model.tabs.get(model.active).map(|tab| {
        (
            tab.group,
            tab.window,
            crate::tabs::surfaces_of(&tab.root).first().copied(),
        )
    });
    let mux = &model.muxes[index];
    let shown: Vec<Window> = mux
        .windows
        .iter()
        .filter(|window| !mux.hidden.contains(&window.id))
        .cloned()
        .collect();
    model.tabs.retain(|tab| {
        tab.group != Some(group) || shown.iter().any(|window| Some(window.id) == tab.window)
    });
    let wanted: HashSet<u32> = shown
        .iter()
        .flat_map(|window| tmux::panes(&window.layout))
        .collect();
    let stale: Vec<(u32, SurfaceId)> = model.muxes[index]
        .surfaces
        .iter()
        .filter(|(pane, _)| !wanted.contains(pane))
        .map(|(pane, id)| (*pane, *id))
        .collect();
    for (pane, id) in stale {
        release(model, index, pane, id);
    }
    let mut leaves = HashMap::new();
    for tab in model.tabs.iter().filter(|tab| tab.group == Some(group)) {
        crate::tabs::walk(&tab.root, &mut |leaf| {
            leaves.insert(leaf.surface, leaf.id);
        });
    }
    let (mut next_leaf, _) = crate::tabs::next_ids(model);
    let mut next_mark = model
        .terminal
        .iter_mut()
        .map(|host| host.mark)
        .max()
        .unwrap_or(0)
        + 1;
    let mut ids = Ids {
        leaves: &leaves,
        next_leaf: &mut next_leaf,
        next_mark: &mut next_mark,
    };
    for window in &shown {
        let root = build(model, index, &window.layout, &mut ids);
        let focused = model.muxes[index]
            .surfaces
            .get(&window.active_pane)
            .and_then(|surface| leaf_of(&root, *surface))
            .unwrap_or_else(|| crate::split::first_leaf(&root));
        match tab_of(model, group, window.id) {
            Some(at) => {
                let tab = &mut model.tabs[at];
                tab.root = root;
                tab.title = window.name.clone();
                tab.focused = focused;
            }
            None => {
                let at = model
                    .tabs
                    .iter()
                    .rposition(|tab| tab.group == Some(group))
                    .map_or(model.tabs.len(), |last| last + 1);
                let profile = model.config.default_profile_name().to_owned();
                model.tabs.insert(
                    at,
                    Tab {
                        title: window.name.clone(),
                        profile,
                        root,
                        focused,
                        group: Some(group),
                        window: Some(window.id),
                    },
                );
            }
        }
    }
    let found = selected.and_then(|(group, window, surface)| {
        model.tabs.iter().position(|tab| match window {
            Some(_) => tab.group == group && tab.window == window,
            None => crate::tabs::surfaces_of(&tab.root).first().copied() == surface,
        })
    });
    model.active = found.unwrap_or(model.active.min(model.tabs.len().saturating_sub(1)));
}

struct Ids<'a> {
    leaves: &'a HashMap<SurfaceId, LeafId>,
    next_leaf: &'a mut u32,
    next_mark: &'a mut u32,
}

/// tmux's splits hold any number of children. The split tree's hold two, so
/// later children nest to the right, one divider each.
fn build(model: &mut Model, index: usize, layout: &Layout, ids: &mut Ids) -> Node {
    match layout {
        Layout::Pane { id, cols, rows } => {
            let surface = match model.muxes[index].surfaces.get(id) {
                Some(surface) => *surface,
                None => {
                    let mark = *ids.next_mark;
                    *ids.next_mark += 1;
                    open_pane(model, index, *id, mark, *cols, *rows)
                }
            };
            let mark = model
                .terminal
                .get(surface)
                .map(|host| host.mark)
                .expect("a live pane has a host");
            let leaf = ids.leaves.get(&surface).copied().unwrap_or_else(|| {
                let leaf = LeafId(*ids.next_leaf);
                *ids.next_leaf += 1;
                leaf
            });
            Node::Leaf(Leaf {
                id: leaf,
                surface,
                mark,
                cols: *cols,
                rows: *rows,
            })
        }
        Layout::Split { axis, children, .. } => {
            let axis = match axis {
                term_mux::Axis::Horizontal => Axis::Horizontal,
                term_mux::Axis::Vertical => Axis::Vertical,
            };
            let nodes: Vec<Node> = children
                .iter()
                .map(|child| build(model, index, child, ids))
                .collect();
            nodes
                .into_iter()
                .rev()
                .reduce(|right, left| Node::Split {
                    axis,
                    left: Box::new(left),
                    right: Box::new(right),
                })
                .expect("a split has children")
        }
    }
}

fn leaf_of(node: &Node, surface: SurfaceId) -> Option<LeafId> {
    match node {
        Node::Leaf(leaf) => (leaf.surface == surface).then_some(leaf.id),
        Node::Split { left, right, .. } => {
            leaf_of(left, surface).or_else(|| leaf_of(right, surface))
        }
    }
}

fn open_pane(
    model: &mut Model,
    index: usize,
    pane: u32,
    mark: u32,
    cols: u16,
    rows: u16,
) -> SurfaceId {
    let desc = SurfaceDesc {
        cols,
        rows,
        ..SurfaceDesc::default()
    };
    let surface = delegate::open_bare(model, mark, cols, rows, &desc);
    let id = surface.id();
    let mux = &mut model.muxes[index];
    model.shared.route_to_mux(id, mux.input.clone(), pane);
    let mut panes = mux.panes.lock().expect("panes");
    if let Some(early) = panes.early.remove(&pane) {
        surface.inject_output(&early);
        model.shared.raise(id);
    }
    panes.live.insert(pane, surface);
    panes.sizes.insert(pane, (cols, rows));
    drop(panes);
    mux.surfaces.insert(pane, id);
    id
}

fn release(model: &mut Model, index: usize, pane: u32, id: SurfaceId) {
    let mux = &mut model.muxes[index];
    {
        let mut held = mux.panes.lock().expect("panes");
        held.live.remove(&pane);
        held.sizes.remove(&pane);
    }
    mux.surfaces.remove(&pane);
    model.shared.forget(id);
    model.terminal.remove(id);
}

/// tmux went away. Its tabs go with it.
fn end(model: &mut Model, index: usize, tx: &mut Tx) {
    let mux = model.muxes.remove(index);
    let group = mux.group;
    for id in mux.surfaces.values() {
        model.shared.forget(*id);
        model.terminal.remove(*id);
    }
    let selected = model
        .tabs
        .get(model.active)
        .map(|tab| crate::tabs::surfaces_of(&tab.root));
    model.tabs.retain(|tab| tab.group != Some(group));
    model.tab_groups.retain(|entry| entry.id != group);
    if model.tabs.is_empty() {
        model.active = 0;
        tx.close_window();
        return;
    }
    model.active = selected
        .and_then(|surfaces| {
            model
                .tabs
                .iter()
                .position(|tab| crate::tabs::surfaces_of(&tab.root) == surfaces)
        })
        .unwrap_or(model.active.min(model.tabs.len() - 1));
    crate::tabs::show_active(model);
    crate::tabs::focus_active(model, tx);
}

/// Closing a window's tab hides it. tmux keeps the window running. The
/// caller drops the tab and its surfaces.
pub(crate) fn hide(model: &mut Model, index: usize) {
    let Some((group, window)) = model
        .tabs
        .get(index)
        .and_then(|tab| Some((tab.group?, tab.window?)))
    else {
        return;
    };
    let Some(mux) = model.muxes.iter_mut().find(|mux| mux.group == group) else {
        return;
    };
    mux.hidden.insert(window);
    let panes = mux
        .windows
        .iter()
        .find(|one| one.id == window)
        .map(|one| tmux::panes(&one.layout))
        .unwrap_or_default();
    let mut live = mux.panes.lock().expect("panes");
    for pane in panes {
        live.live.remove(&pane);
        live.sizes.remove(&pane);
        mux.surfaces.remove(&pane);
    }
}

/// `group:window`, from the menu. The window's tab comes back selected and
/// its panes redraw from tmux.
pub(crate) fn show(model: &mut Model, id: &str) -> bool {
    let Some((group, window)) = id
        .split_once(':')
        .and_then(|(group, window)| Some((group.parse().ok()?, window.parse().ok()?)))
    else {
        return false;
    };
    let group = TabGroupId(group);
    let Some(index) = model.muxes.iter().position(|mux| mux.group == group) else {
        return false;
    };
    let mux = &mut model.muxes[index];
    if !mux.hidden.remove(&window) {
        return false;
    }
    let panes = mux
        .windows
        .iter()
        .find(|one| one.id == window)
        .map(|one| tmux::panes(&one.layout))
        .unwrap_or_default();
    {
        let mut held = mux.panes.lock().expect("panes");
        for pane in &panes {
            held.early.remove(pane);
        }
    }
    for pane in panes {
        let _ = mux.input.send(tmux::capture(pane).into_bytes());
    }
    sync_tabs(model, index);
    if let Some(at) = tab_of(model, group, window) {
        model.active = at;
        crate::tabs::show_active(model);
    }
    true
}

/// The selected tab's session, window, and focused pane.
fn focused(model: &Model) -> Option<(usize, u32, u32)> {
    let tab = model.tabs.get(model.active)?;
    let window = tab.window?;
    let index = model
        .muxes
        .iter()
        .position(|mux| Some(mux.group) == tab.group)?;
    let surface = crate::tabs::surface_of(tab)?;
    let pane = model.muxes[index]
        .surfaces
        .iter()
        .find(|(_, id)| **id == surface)
        .map(|(pane, _)| *pane)?;
    Some((index, window, pane))
}

/// A new tab on a session's tab is a new tmux window. tmux reports it back.
pub(crate) fn open_window(model: &Model) -> bool {
    let Some((index, ..)) = focused(model) else {
        return false;
    };
    let _ = model.muxes[index]
        .input
        .send(tmux::new_window().into_bytes());
    true
}

pub(crate) fn split(model: &Model, axis: Axis) {
    let Some((index, _, pane)) = focused(model) else {
        return;
    };
    let axis = match axis {
        Axis::Horizontal => term_mux::Axis::Horizontal,
        Axis::Vertical => term_mux::Axis::Vertical,
    };
    let _ = model.muxes[index]
        .input
        .send(tmux::split_window(pane, axis).into_bytes());
}

/// A divider drag on a session's tab. `surface` heads the side that takes
/// `cells`. False when the selected tab is not tmux's.
pub(crate) fn resize(model: &mut Model, surface: SurfaceId, axis: Axis, cells: u16) -> bool {
    let Some((index, ..)) = focused(model) else {
        return false;
    };
    let mux = &mut model.muxes[index];
    let Some(pane) = mux
        .surfaces
        .iter()
        .find(|(_, id)| **id == surface)
        .map(|(pane, _)| *pane)
    else {
        return true;
    };
    if mux.resized != Some((pane, axis, cells)) {
        mux.resized = Some((pane, axis, cells));
        let tmux_axis = match axis {
            Axis::Horizontal => term_mux::Axis::Horizontal,
            Axis::Vertical => term_mux::Axis::Vertical,
        };
        let _ = mux
            .input
            .send(tmux::resize_pane(pane, tmux_axis, cells).into_bytes());
    }
    true
}

/// The cells a split window's area holds, from its layout.
pub(crate) fn fill(model: &mut Model, group: TabGroupId, cells: (u16, u16)) {
    if let Some(mux) = model.muxes.iter_mut().find(|mux| mux.group == group) {
        mux.slot = Some(cells);
    }
}

/// Tell tmux the size the selected window has here, and which pane has focus.
pub(crate) fn report(model: &mut Model) {
    let Some((index, window, pane)) = focused(model) else {
        return;
    };
    let cell_px = crate::tabs::surface_of(&model.tabs[model.active])
        .and_then(|id| model.terminal.get(id))
        .map(|host| host.cell_px)
        .filter(|(w, h)| *w > 0 && *h > 0);
    let size = match &model.tabs[model.active].root {
        Node::Leaf(leaf) => model
            .terminal
            .get(leaf.surface)
            .map(|host| (host.cols, host.rows))
            .filter(|(cols, rows)| *cols > 0 && *rows > 0),
        Node::Split { .. } => model.muxes[index].slot,
    };
    let mux = &mut model.muxes[index];
    if let Some(cell_px) = cell_px {
        mux.panes.lock().expect("panes").cell_px = cell_px;
    }
    if let Some((cols, rows)) = size.filter(|size| mux.reported != Some(*size)) {
        mux.reported = Some((cols, rows));
        let _ = mux
            .input
            .send(tmux::refresh_client(cols, rows).into_bytes());
    }
    if mux.selected != Some((window, pane)) {
        mux.selected = Some((window, pane));
        let _ = mux.input.send(tmux::select(window, pane).into_bytes());
    }
}

/// Splits for a session's tab, and every hidden window.
pub(crate) fn menu(model: &Model, mut menu: PopupMenu) -> PopupMenu {
    if focused(model).is_some() {
        menu = menu
            .separator()
            .item(PopupItem::new("split-right", "Split Right"))
            .item(PopupItem::new("split-down", "Split Down"));
    }
    let hidden: Vec<(TabGroupId, &Window)> = model
        .muxes
        .iter()
        .flat_map(|mux| {
            mux.windows
                .iter()
                .filter(|window| mux.hidden.contains(&window.id))
                .map(|window| (mux.group, window))
        })
        .collect();
    if !hidden.is_empty() {
        menu = menu.separator();
    }
    for (group, window) in hidden {
        menu = menu.item(PopupItem::new(
            format!("show-window:{}:{}", group.0, window.id),
            format!("Show {}", window.name),
        ));
    }
    menu
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    /// A tmux server of the test's own, gone with the test.
    struct Server(String);

    impl Drop for Server {
        fn drop(&mut self) {
            let _ = Command::new("tmux")
                .args(["-L", &self.0, "kill-server"])
                .stderr(Stdio::null())
                .status();
        }
    }

    fn attached(name: &str) -> (Server, Model, TabGroupId) {
        let server = Server(format!("rune-term-{}-{name}", std::process::id()));
        let mut model = crate::model::init();
        delegate::setup(&mut model, Arc::new(|| {}));
        let args: Vec<String> = [
            "-L",
            &server.0,
            "-f",
            "/dev/null",
            "-C",
            "new-session",
            "-A",
            "-s",
            name,
            "/bin/sh",
            ";",
            "set",
            "-g",
            "default-shell",
            "/bin/sh",
        ]
        .iter()
        .map(|arg| (*arg).to_owned())
        .collect();
        let group = attach(&mut model, &args).expect("tmux");
        (server, model, group)
    }

    fn until(model: &mut Model, mut done: impl FnMut(&mut Model) -> bool) -> bool {
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            delegate::poll(model, 0.0, &mut Tx::default());
            if done(model) {
                return true;
            }
            thread::sleep(Duration::from_millis(10));
        }
        false
    }

    fn screen(model: &mut Model, surface: SurfaceId) -> String {
        let Some(host) = model.terminal.get_mut(surface) else {
            return String::new();
        };
        host.render.update();
        (0..host.render.lines())
            .map(|line| host.render.text_row(line))
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn leaves(node: &Node) -> usize {
        match node {
            Node::Leaf(_) => 1,
            Node::Split { left, right, .. } => leaves(left) + leaves(right),
        }
    }

    #[test]
    fn a_session_is_a_group_and_its_pane_takes_keys() {
        let (_server, mut model, group) = attached("keys");
        assert!(
            until(&mut model, |model| model.tabs.len() == 2),
            "the window did not become a tab"
        );
        assert_eq!(model.tabs[model.active].group, Some(group));
        assert_eq!(
            model
                .tab_groups
                .iter()
                .find(|g| g.id == group)
                .unwrap()
                .label,
            "keys"
        );
        let pane = crate::tabs::surface_of(&model.tabs[model.active]).unwrap();
        model
            .terminal
            .get(pane)
            .unwrap()
            .surface
            .text("echo $((6*7))\r");
        assert!(
            until(&mut model, |model| screen(model, pane).contains("\n42")),
            "the answer did not come back: {}",
            screen(&mut model, pane)
        );
    }

    #[test]
    fn a_split_is_two_leaves_and_a_hidden_window_comes_back_redrawn() {
        let (_server, mut model, group) = attached("split");
        assert!(until(&mut model, |model| model.tabs.len() == 2));
        let first = crate::tabs::surface_of(&model.tabs[model.active]).unwrap();
        model
            .terminal
            .get(first)
            .unwrap()
            .surface
            .text("echo marker\r");
        assert!(until(&mut model, |model| screen(model, first)
            .contains("\nmarker")));
        split(&model, Axis::Horizontal);
        assert!(
            until(&mut model, |model| leaves(&model.tabs[model.active].root)
                == 2),
            "the split did not reach the tree"
        );

        let window = model.tabs[model.active].window;
        crate::tabs::close_active(&mut model, &mut Tx::default());
        assert_eq!(model.tabs.len(), 1);
        assert!(model.terminal.get(first).is_none());
        let menu = format!("{}:{}", group.0, window.unwrap());
        assert!(show(&mut model, &menu));
        assert_eq!(model.tabs[model.active].window, window);
        assert_eq!(leaves(&model.tabs[model.active].root), 2);
        let mut back = Vec::new();
        crate::tabs::walk(&model.tabs[model.active].root, &mut |leaf| {
            back.push(leaf.surface)
        });
        assert!(
            until(&mut model, |model| back
                .iter()
                .any(|id| screen(model, *id).contains("\nmarker"))),
            "the shown window was not redrawn"
        );
    }

    #[test]
    fn the_server_going_away_closes_its_tabs_only() {
        let (server, mut model, group) = attached("exit");
        assert!(until(&mut model, |model| model.tabs.len() == 2));
        let _ = Command::new("tmux")
            .args(["-L", &server.0, "kill-server"])
            .status();
        assert!(
            until(&mut model, |model| model.tabs.len() == 1),
            "the session's tab outlived it"
        );
        assert_eq!(model.tabs[0].group, None);
        assert!(model.tab_groups.iter().all(|g| g.id != group));
        assert_eq!(model.terminal.len(), 1);
    }

    #[test]
    fn a_divider_drag_resizes_the_pane_in_tmux() {
        let (_server, mut model, _group) = attached("resize");
        assert!(until(&mut model, |model| model.tabs.len() == 2));
        split(&model, Axis::Horizontal);
        assert!(until(&mut model, |model| leaves(
            &model.tabs[model.active].root
        ) == 2));
        let Some((Axis::Horizontal, cells, left)) =
            crate::split::divider(&model.tabs[model.active].root, &[])
        else {
            panic!("a side-by-side split");
        };
        assert!(resize(&mut model, left, Axis::Horizontal, cells - 10));
        assert!(
            until(&mut model, |model| {
                crate::split::divider(&model.tabs[model.active].root, &[])
                    .is_some_and(|(_, now, _)| now == cells - 10)
            }),
            "tmux did not report the new layout"
        );
    }

    /// The far end, not this copy, decides what a resized pane holds.
    #[test]
    fn a_pane_dragged_through_every_size_shows_what_tmux_shows() {
        let (server, mut model, _group) = attached("sweep");
        assert!(until(&mut model, |model| model.tabs.len() == 2));
        // A two-line prompt that, like readline's, goes back up and redraws itself
        // whenever the pane is resized, after history the screen was cleared into.
        let prompt = r#"p() { printf '\033[1A\r\033[Jprompt\n> '; }; trap p WINCH; printf 'old1\nold2\nold3\n\033[H\033[2Jprompt\n> '; while :; do sleep 0.01; done"#;
        Command::new("tmux")
            .args([
                "-L",
                &server.0,
                "split-window",
                "-v",
                "-t",
                "sweep:0.0",
                prompt,
            ])
            .status()
            .unwrap();
        assert!(until(&mut model, |model| leaves(
            &model.tabs[model.active].root
        ) == 2));
        let mut ids = Vec::new();
        crate::tabs::walk(&model.tabs[model.active].root, &mut |leaf| {
            ids.push(leaf.surface)
        });
        let (top, bottom) = (ids[0], ids[1]);
        let mut rt = rune::Runtime::new(
            model,
            crate::chrome::view,
            (800.0, 500.0),
            rune::Env::new(true, 1.0),
        );
        let pump = |rt: &mut rune::Runtime<Model>, ms: u64| {
            let end = Instant::now() + Duration::from_millis(ms);
            while Instant::now() < end {
                rt.update(|model, tx| {
                    delegate::poll(model, 0.0, tx);
                });
                rt.frame(0.0);
                thread::sleep(Duration::from_millis(5));
            }
        };
        pump(&mut rt, 300);
        let sweep: Vec<u16> = (1..=20).chain((1..20).rev()).chain(1..=11).collect();
        for rows in sweep {
            rt.update(|model, _| {
                resize(model, top, Axis::Vertical, rows);
            });
            pump(&mut rt, 10);
        }
        pump(&mut rt, 500);
        let out = Command::new("tmux")
            .args(["-L", &server.0, "capture-pane", "-p", "-t", "sweep:0.1"])
            .output()
            .unwrap();
        let theirs = String::from_utf8_lossy(&out.stdout).trim_end().to_owned();
        let ours = screen(&mut rt.state, bottom).trim_end().to_owned();
        assert_eq!(ours, theirs);
    }
}
