//! A tmux session in control mode as a tab group: a window is a tab and a pane
//! is a leaf. tmux owns the layout. rune-term reports its size in cells and
//! mirrors the layout tmux sends back. rmux speaks the same protocol, and
//! either can run on an ssh host.

mod remote;
mod target;

use std::collections::{BTreeSet, HashMap, HashSet};
use std::io::{self, Read, Write};
use std::process::{ChildStdout, Stdio};
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
pub(crate) use target::{Program, Target};

/// What the command line asked to open once the window is up.
enum Queued {
    Session(Target, Vec<String>),
    Profile(String),
}

static QUEUED: Mutex<Option<Queued>> = Mutex::new(None);
/// Above any group a test or the strip made by hand.
static NEXT_GROUP: AtomicU32 = AtomicU32::new(1 << 16);

/// Output kept for a pane that has no surface yet. Past this it is dropped:
/// showing a hidden window redraws it from a capture.
const EARLY_CAP: usize = 1 << 20;

pub(crate) struct Mux {
    pub group: TabGroupId,
    target: Target,
    /// The session's name once the multiplexer reports it.
    session: Option<String>,
    /// The profile its tabs take.
    profile: String,
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

/// The session `setup` attaches.
pub(crate) fn queue(target: Target, session: Session) {
    *QUEUED.lock().expect("tmux") = Some(Queued::Session(target, session_args(session)));
}

/// The profile `setup` opens: a session one attaches, any other is the first tab's.
pub(crate) fn queue_profile(name: String) {
    *QUEUED.lock().expect("tmux") = Some(Queued::Profile(name));
}

/// Before the first tab's shell starts: a queued profile that is not a session
/// becomes that tab's profile.
pub(crate) fn start_queued_profile(model: &mut Model) {
    let mut queued = QUEUED.lock().expect("tmux");
    let Some(Queued::Profile(name)) = &*queued else {
        return;
    };
    let Some(profile) = model.config.profile(name) else {
        eprintln!("rune-term: no profile named '{name}'");
        *queued = None;
        return;
    };
    if Program::from_name(profile.backend.trim()).is_some() {
        return;
    }
    if let Some(tab) = model.tabs.get_mut(model.active) {
        tab.profile = name.clone();
    }
    *queued = None;
    drop(queued);
    crate::tabs::apply_active_profile(model);
}

pub(crate) fn attach_queued(model: &mut Model) {
    match QUEUED.lock().expect("tmux").take() {
        Some(Queued::Session(target, args)) => {
            if let Err(err) = attach(model, &target, &args) {
                eprintln!("rune-term: {}: {err}", target.program.name());
            }
        }
        Some(Queued::Profile(name)) => {
            open_profile(model, &name);
        }
        None => {}
    }
}

/// The sessions on the target's server. None when the program is not installed,
/// and empty when no server runs. It runs on the UI thread, so only for a local target.
pub(crate) fn sessions(target: &Target) -> Option<Vec<String>> {
    let out = target
        .command(&["list-sessions", "-F", "#{session_name}"].map(String::from))
        .stderr(Stdio::null())
        .output()
        .ok()?;
    Some(
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .map(str::to_owned)
            .collect(),
    )
}

/// What a name prompt names: a session to start, or a profile that reopens a group.
pub(crate) enum Naming {
    Session(Target),
    Profile(TabGroupId),
}

pub(crate) struct NamePrompt {
    pub naming: Naming,
    pub edit: rune::TextEdit,
    /// Why the last name was refused.
    pub error: Option<String>,
}

impl NamePrompt {
    fn new(naming: Naming, text: &str) -> Self {
        let mut edit = rune::TextEdit::new(text);
        edit.focused = true;
        Self {
            naming,
            edit,
            error: None,
        }
    }
}

pub(crate) enum Session<'a> {
    /// tmux picks the name.
    New,
    /// Attaches the session instead when one has this name.
    Named(&'a str),
    Attach(&'a str),
}

fn session_args(session: Session) -> Vec<String> {
    let args = match session {
        Session::New => vec!["new-session".to_owned()],
        Session::Named(name) => ["new-session", "-A", "-s", name].map(String::from).to_vec(),
        // `=` matches the name exactly, not as a prefix.
        Session::Attach(name) => vec!["attach-session".into(), "-t".into(), format!("={name}")],
    };
    [vec!["-C".to_owned()], args].concat()
}

/// Where a profile whose backend is tmux or rmux runs, and the session it names.
fn profile_target(profile: &crate::settings::Profile) -> Option<(Target, Option<String>)> {
    let program = Program::from_name(profile.backend.trim())?;
    let non_empty = |field: &Option<String>| {
        field
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
    };
    let target = Target {
        program,
        host: non_empty(&profile.host),
        socket: None,
    };
    Some((target, non_empty(&profile.session)))
}

/// A profile whose backend is tmux or rmux: its session, here or on its host.
/// False for any other backend.
pub(crate) fn open_profile(model: &mut Model, name: &str) -> bool {
    let Some((target, session)) = model.config.profile(name).and_then(profile_target) else {
        return false;
    };
    let session = match &session {
        Some(session) => Session::Named(session),
        None => Session::New,
    };
    match attach(model, &target, &session_args(session)) {
        Ok(group) => {
            if let Some(mux) = model.muxes.iter_mut().find(|mux| mux.group == group) {
                mux.profile = name.to_owned();
            }
        }
        Err(err) => eprintln!("rune-term: {}: {err}", target.program.name()),
    }
    true
}

/// From the menu. The session's windows arrive as tabs once tmux lists them.
pub(crate) fn open_session(model: &mut Model, target: &Target, session: Session) {
    if let Err(err) = attach(model, target, &session_args(session)) {
        eprintln!("rune-term: {}: {err}", target.program.name());
    }
}

pub(crate) fn ask_name(model: &mut Model, target: Target) {
    model.name_prompt = Some(NamePrompt::new(Naming::Session(target), ""));
}

/// The prompt to save the tab's group as a profile, named for its session.
pub(crate) fn ask_profile_name(model: &mut Model, index: usize) {
    let Some((mux, _)) = window_of(model, index) else {
        return;
    };
    let mux = &model.muxes[mux];
    let name = mux
        .target
        .label(mux.session.as_deref().unwrap_or(mux.target.program.name()));
    model.name_prompt = Some(NamePrompt::new(Naming::Profile(mux.group), &name));
}

/// The name the prompt holds. An empty session name starts nothing; a refused
/// profile name keeps the prompt open with the reason.
pub(crate) fn submit_name(model: &mut Model) {
    let Some(prompt) = model.name_prompt.take() else {
        return;
    };
    let name = prompt.edit.text.trim().to_owned();
    match &prompt.naming {
        Naming::Session(target) => {
            if !name.is_empty() {
                open_session(model, target, Session::Named(&name));
            }
        }
        Naming::Profile(group) => {
            if let Err(err) = save_profile(model, *group, &name) {
                model.name_prompt = Some(NamePrompt {
                    error: Some(err),
                    ..prompt
                });
            }
        }
    }
}

/// A profile that attaches the group's session again, here or on its host, with
/// the font and theme its tabs have.
fn save_profile(model: &mut Model, group: TabGroupId, name: &str) -> Result<(), String> {
    if !crate::settings::name_ok(name) {
        return Err(format!("'{name}' cannot name a profile"));
    }
    if model.config.profile(name).is_some() {
        return Err(format!("A profile named '{name}' exists"));
    }
    let Some(mux) = model.muxes.iter().find(|mux| mux.group == group) else {
        return Err("The session is gone".to_owned());
    };
    let mut profile = model
        .config
        .profile(&mux.profile)
        .cloned()
        .unwrap_or_default();
    profile.backend = mux.target.program.name().to_owned();
    profile.host = mux.target.host.clone();
    profile.session = mux.session.clone();
    profile.shell = None;
    profile.cwd = None;
    model
        .config
        .profiles
        .insert(name.to_owned(), profile.clone());
    if let Err(err) = model.config.save() {
        model.config.profiles.remove(name);
        return Err(format!("Could not save: {err}"));
    }
    if let Some(settings) = &model.settings {
        let name = name.to_owned();
        settings.update(move |settings, _tx| {
            crate::settings_ui::profiles::add(settings, name, profile)
        });
    }
    Ok(())
}

/// `args` follow the program and its socket, and include `-C`.
pub(crate) fn attach(
    model: &mut Model,
    target: &Target,
    args: &[String],
) -> io::Result<TabGroupId> {
    let group = TabGroupId(NEXT_GROUP.fetch_add(1, Ordering::Relaxed));
    let panes = Arc::new(Mutex::new(Panes::default()));
    let input = link(&model.shared, target, args, group, &panes)?;
    model.tab_groups.push(TabGroup {
        id: group,
        label: target.label(target.program.name()),
        collapsed: false,
        lost: false,
    });
    model.muxes.push(Mux {
        group,
        target: target.clone(),
        session: None,
        profile: model.config.default_profile_name().to_owned(),
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

/// The multiplexer, with a thread writing what the group sends it and one
/// reading what it says back into the group's panes.
fn link(
    shared: &Arc<Shared>,
    target: &Target,
    args: &[String],
    group: TabGroupId,
    panes: &Arc<Mutex<Panes>>,
) -> io::Result<mpsc::Sender<Vec<u8>>> {
    let mut child = target
        .command(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    let mut stdin = child.stdin.take().expect("piped stdin");
    let stdout = child.stdout.take().expect("piped stdout");
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
    let reader = (Arc::clone(panes), Arc::clone(shared), input.clone());
    thread::Builder::new()
        .name(format!("tmux-read-{}", group.0))
        .spawn(move || {
            let (panes, shared, input) = reader;
            match read(stdout, &panes, &shared, &input, group) {
                Ok(reason) => {
                    shared.push_mux(group, Update::Exit(reason));
                    let _ = child.wait();
                }
                Err(err) => {
                    let status = child.wait();
                    let status =
                        status.map_or_else(|err| err.to_string(), |status| status.to_string());
                    shared.push_mux_lost(group, err.unwrap_or(status));
                }
            }
        })?;
    Ok(input)
}

/// The `%exit` reason, or, when the link ended without one, the read error if any.
fn read(
    mut stdout: ChildStdout,
    panes: &Mutex<Panes>,
    shared: &Shared,
    input: &mpsc::Sender<Vec<u8>>,
    group: TabGroupId,
) -> Result<Option<String>, Option<String>> {
    let mut client = tmux::Client::default();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = match stdout.read(&mut buf) {
            Ok(0) => return Err(None),
            Ok(n) => n,
            Err(err) if err.kind() == io::ErrorKind::Interrupted => continue,
            Err(err) => return Err(Some(err.to_string())),
        };
        for update in client.feed(&buf[..n]) {
            match update {
                Update::Output { pane, bytes } => deliver(panes, shared, pane, &bytes),
                Update::Windows(windows) => {
                    follow_sizes(panes, shared, &windows);
                    shared.push_mux(group, Update::Windows(windows));
                }
                Update::Exit(reason) => return Ok(reason),
                other => shared.push_mux(group, other),
            }
        }
        let commands = client.take_commands();
        if !commands.is_empty() {
            let _ = input.send(commands.into_bytes());
        }
    }
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
            let label = model.muxes[index].target.label(&name);
            model.muxes[index].session = Some(name);
            if let Some(entry) = model.tab_groups.iter_mut().find(|g| g.id == group) {
                entry.label = label;
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
                let profile = model.muxes[index].profile.clone();
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

/// The link dropped. A group that never reached its session goes; one that
/// did stays, greyed, for its tab menu to reconnect.
pub(crate) fn lost(model: &mut Model, group: TabGroupId, reason: &str, tx: &mut Tx) -> bool {
    let Some(index) = model.muxes.iter().position(|mux| mux.group == group) else {
        return false;
    };
    let mux = &model.muxes[index];
    eprintln!(
        "rune-term: {}: the link dropped: {reason}",
        mux.target.program.name()
    );
    if mux.session.is_none() {
        end(model, index, tx);
        return true;
    }
    if let Some(entry) = model.tab_groups.iter_mut().find(|entry| entry.id == group) {
        entry.lost = true;
    }
    true
}

/// A lost group attaches its session again, over a new link, into the tabs it has.
/// tmux redraws every pane on attach. A session gone in the meantime starts anew.
pub(crate) fn reconnect(model: &mut Model, index: usize) {
    let Some((at, _)) = window_of(model, index) else {
        return;
    };
    let mux = &model.muxes[at];
    let Some(session) = mux.session.clone() else {
        return;
    };
    let args = session_args(Session::Named(&session));
    let input = match link(&model.shared, &mux.target, &args, mux.group, &mux.panes) {
        Ok(input) => input,
        Err(err) => {
            eprintln!("rune-term: {}: {err}", mux.target.program.name());
            return;
        }
    };
    for (pane, id) in &mux.surfaces {
        model.shared.route_to_mux(*id, input.clone(), *pane);
    }
    let group = mux.group;
    let mux = &mut model.muxes[at];
    mux.input = input;
    mux.reported = None;
    mux.selected = None;
    mux.resized = None;
    if let Some(entry) = model.tab_groups.iter_mut().find(|entry| entry.id == group) {
        entry.lost = false;
    }
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
/// The session behind tab `index` and the tmux window it shows.
pub(crate) fn window_of(model: &Model, index: usize) -> Option<(usize, u32)> {
    let tab = model.tabs.get(index)?;
    let mux = model
        .muxes
        .iter()
        .position(|mux| Some(mux.group) == tab.group)?;
    Some((mux, tab.window?))
}

/// tmux closes the window and its shells. The tab goes when tmux reports it gone.
pub(crate) fn kill_window(model: &Model, index: usize) {
    if let Some((mux, window)) = window_of(model, index) {
        let _ = model.muxes[mux]
            .input
            .send(tmux::kill_window(window).into_bytes());
    }
}

/// The session keeps running in tmux. Every tab of it goes from here.
pub(crate) fn detach(model: &mut Model, index: usize, tx: &mut Tx) {
    if let Some((mux, _)) = window_of(model, index) {
        end(model, mux, tx);
    }
}

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
    menu = hidden(model, menu);
    for program in [Program::Tmux, Program::Rmux] {
        menu = program_menu(model, &Target::local(program), menu);
    }
    menu
}

/// The hosts a session profile or an open group names, for each program.
fn remote_targets(model: &Model) -> Vec<Target> {
    let mut targets: Vec<Target> = model
        .config
        .profiles
        .values()
        .filter_map(profile_target)
        .map(|(target, _)| target)
        .chain(model.muxes.iter().map(|mux| mux.target.clone()))
        .filter(|target| target.host.is_some() && target.socket.is_none())
        .collect();
    targets.sort_by(|a, b| (a.program.name(), &a.host).cmp(&(b.program.name(), &b.host)));
    targets.dedup();
    targets
}

/// A host's entry in its program's submenu: a new session there, and the sessions it last listed.
fn host_menu(model: &Model, target: &Target, menu: PopupMenu) -> PopupMenu {
    let host = target.host.as_deref().expect("a remote target");
    let id = |action: &str| format!("{}-{action}@{host}", target.program.name());
    let sessions = match remote::listing(target) {
        remote::Listed::Missing => {
            let missing = PopupItem::new(id("missing"), "Not Installed").enabled(false);
            return menu.submenu(host, PopupMenu::new().item(missing));
        }
        remote::Listed::Asking => vec![PopupItem::new(id("asking"), "Listing…").enabled(false)],
        remote::Listed::Unreachable => {
            vec![PopupItem::new(id("unreachable"), "Unreachable").enabled(false)]
        }
        remote::Listed::Sessions(sessions) => sessions
            .iter()
            .filter(|session| !attached_here(model, target, session))
            .map(|session| PopupItem::new(format!("{}:{session}", id("attach")), session.as_str()))
            .collect(),
    };
    menu.submenu(
        host,
        PopupMenu::new()
            .item(PopupItem::new(id("new"), "New Session"))
            .separator()
            .items(sessions),
    )
}

/// A program's submenu, when it is installed here or a host names it. Its ids
/// are the program's name, a dash, and the action.
fn program_menu(model: &Model, target: &Target, menu: PopupMenu) -> PopupMenu {
    let name = target.program.name();
    let remote: Vec<Target> = remote_targets(model)
        .into_iter()
        .filter(|remote| remote.program == target.program)
        .collect();
    let submenu = match sessions(target) {
        None if remote.is_empty() => return menu,
        None => PopupMenu::new(),
        Some(sessions) => {
            // Greyed out, rather than gone, when every session is already open here.
            let attach: Vec<PopupItem> = sessions
                .iter()
                .filter(|session| !attached_here(model, target, session))
                .map(|session| PopupItem::new(format!("{name}-attach:{session}"), session.as_str()))
                .collect();
            let open = !attach.is_empty();
            let submenu = PopupMenu::new()
                .item(PopupItem::new(format!("{name}-new"), "New Session"))
                .item(PopupItem::new(
                    format!("{name}-named"),
                    "New Named Session…",
                ))
                .separator()
                .submenu_enabled("Attach Session", PopupMenu::new().items(attach), open);
            match remote.is_empty() {
                true => submenu,
                false => submenu.separator(),
            }
        }
    };
    let submenu = remote
        .iter()
        .fold(submenu, |submenu, remote| host_menu(model, remote, submenu));
    menu.separator().submenu(name, submenu)
}

/// A menu id from [`program_menu`]. True when it was one.
pub(crate) fn menu_action(model: &mut Model, id: &str) -> bool {
    let Some((program, action)) = id.split_once('-') else {
        return false;
    };
    let Some(program) = Program::from_name(program) else {
        return false;
    };
    let Some((target, action)) = parse_action(program, action) else {
        return false;
    };
    match action {
        Action::New => open_session(model, &target, Session::New),
        Action::Named => ask_name(model, target),
        Action::Attach(session) => open_session(model, &target, Session::Attach(session)),
    }
    true
}

enum Action<'a> {
    New,
    Named,
    Attach(&'a str),
}

/// `new`, `named`, or `attach:SESSION`; a remote one has `@HOST` after the verb.
/// tmux turns a `:` in a session name into `_`, so the last one splits.
fn parse_action(program: Program, action: &str) -> Option<(Target, Action<'_>)> {
    let (verb, session) = match action.rsplit_once(':') {
        Some((verb, session)) => (verb, Some(session)),
        None => (action, None),
    };
    let (verb, host) = match verb.split_once('@') {
        Some((verb, host)) => (verb, Some(host.to_owned())),
        None => (verb, None),
    };
    let action = match (verb, session) {
        ("new", None) => Action::New,
        ("named", None) => Action::Named,
        ("attach", Some(session)) => Action::Attach(session),
        _ => return None,
    };
    let target = Target {
        program,
        host,
        socket: None,
    };
    Some((target, action))
}

/// One Hidden Windows entry. Its windows, or with several sessions a submenu each. A
/// window goes by its tmux index, since windows running the same program share a name.
fn hidden(model: &Model, menu: PopupMenu) -> PopupMenu {
    let mut sessions: Vec<(&str, Vec<PopupItem>)> = Vec::new();
    for mux in &model.muxes {
        let windows: Vec<PopupItem> = mux
            .windows
            .iter()
            .filter(|window| mux.hidden.contains(&window.id))
            .map(|window| {
                PopupItem::new(
                    format!("show-window:{}:{}", mux.group.0, window.id),
                    format!("{}: {}", window.index, window.name),
                )
            })
            .collect();
        if windows.is_empty() {
            continue;
        }
        let session = model
            .tab_groups
            .iter()
            .find(|group| group.id == mux.group)
            .map_or("tmux", |group| group.label.as_str());
        sessions.push((session, windows));
    }
    let list = match sessions.len() {
        0 => return menu,
        1 => PopupMenu::new().items(sessions.pop().expect("one session").1),
        _ => sessions
            .into_iter()
            .fold(PopupMenu::new(), |list, (session, windows)| {
                list.submenu(session, PopupMenu::new().items(windows))
            }),
    };
    menu.separator().submenu("Hidden Windows", list)
}

fn attached_here(model: &Model, target: &Target, session: &str) -> bool {
    let label = target.label(session);
    model.muxes.iter().any(|mux| {
        mux.target == *target
            && model
                .tab_groups
                .iter()
                .any(|group| group.id == mux.group && group.label == label)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;
    use std::time::{Duration, Instant};

    /// A server of the test's own, gone with the test.
    struct Server(Target);

    impl Server {
        fn run(&self, args: &[&str]) -> Command {
            let args: Vec<String> = args.iter().map(|arg| (*arg).to_owned()).collect();
            self.0.command(&args)
        }
    }

    impl Drop for Server {
        fn drop(&mut self) {
            let _ = self.run(&["kill-server"]).stderr(Stdio::null()).status();
        }
    }

    fn attached(name: &str) -> Option<(Server, Model, TabGroupId)> {
        attached_on(Program::Tmux, name)
    }

    /// On the host `RUNE_TERM_SSH_HOST` names; without it the ssh tests skip.
    fn attached_over_ssh(name: &str) -> Option<(Server, Model, TabGroupId)> {
        let Ok(host) = std::env::var("RUNE_TERM_SSH_HOST") else {
            eprintln!("RUNE_TERM_SSH_HOST is not set: skipped");
            return None;
        };
        let server = Server(Target {
            program: Program::Tmux,
            host: Some(host),
            socket: Some(format!("rune-term-{}-{name}", std::process::id())),
        });
        let mut model = crate::model::init();
        delegate::setup(&mut model, Arc::new(|| {}));
        let args: Vec<String> = [
            "-f",
            "/dev/null",
            "-C",
            "new-session",
            "-A",
            "-s",
            name,
            "/bin/sh",
        ]
        .iter()
        .map(|arg| (*arg).to_owned())
        .collect();
        let group = attach(&mut model, &server.0, &args).expect("ssh");
        Some((server, model, group))
    }

    /// A machine without the program skips these tests rather than failing them.
    fn attached_on(program: Program, name: &str) -> Option<(Server, Model, TabGroupId)> {
        let installed = Command::new(program.name())
            .arg("-V")
            .output()
            .is_ok_and(|out| out.status.success());
        if !installed {
            eprintln!("{} is not installed: skipped", program.name());
            return None;
        }
        let server = Server(Target {
            program,
            host: None,
            socket: Some(format!("rune-term-{}-{name}", std::process::id())),
        });
        let mut model = crate::model::init();
        delegate::setup(&mut model, Arc::new(|| {}));
        let args: Vec<String> = [
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
        let group = attach(&mut model, &server.0, &args).expect("tmux");
        Some((server, model, group))
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
        session_takes_keys(Program::Tmux);
    }

    #[test]
    fn a_session_is_a_group_and_its_pane_takes_keys_in_rmux() {
        session_takes_keys(Program::Rmux);
    }

    fn session_takes_keys(program: Program) {
        let Some((_server, mut model, group)) = attached_on(program, "keys") else {
            return;
        };
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
        let Some((_server, mut model, group)) = attached("split") else {
            return;
        };
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
        let item = hidden(&model, PopupMenu::new())
            .find(&format!("show-window:{menu}"))
            .map(|item| item.title().to_owned());
        // `/bin/sh` names the window `sh` or `bash`, depending on the system.
        assert!(
            item.as_ref().is_some_and(|title| title.starts_with("0: ")),
            "{item:?}"
        );
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
        let Some((server, mut model, group)) = attached("exit") else {
            return;
        };
        assert!(until(&mut model, |model| model.tabs.len() == 2));
        let _ = server.run(&["kill-server"]).status();
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
        divider_drag_resizes(Program::Tmux);
    }

    #[test]
    fn a_divider_drag_resizes_the_pane_in_tmux_in_rmux() {
        divider_drag_resizes(Program::Rmux);
    }

    fn divider_drag_resizes(program: Program) {
        let Some((_server, mut model, _group)) = attached_on(program, "resize") else {
            return;
        };
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
        let Some((server, mut model, _group)) = attached("sweep") else {
            return;
        };
        assert!(until(&mut model, |model| model.tabs.len() == 2));
        // A two-line prompt that, like readline's, goes back up and redraws itself
        // whenever the pane is resized, after history the screen was cleared into.
        let prompt = r#"p() { printf '\033[1A\r\033[Jprompt\n> '; }; trap p WINCH; printf 'old1\nold2\nold3\n\033[H\033[2Jprompt\n> '; while :; do sleep 0.01; done"#;
        let out = server
            .run(&["split-window", "-v", "-t", "sweep:0.0", prompt])
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "split-window: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(
            until(&mut model, |model| leaves(&model.tabs[model.active].root)
                == 2),
            "the split did not reach the tree: {} tabs, active {}",
            model.tabs.len(),
            model.active
        );
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
        let out = server
            .run(&["capture-pane", "-p", "-t", "sweep:0.1"])
            .output()
            .unwrap();
        let theirs = String::from_utf8_lossy(&out.stdout).trim_end().to_owned();
        let ours = screen(&mut rt.state, bottom).trim_end().to_owned();
        assert_eq!(ours, theirs);
    }

    #[test]
    fn a_listed_session_attaches_as_its_own_group() {
        let Some((server, mut model, first)) = attached("listed") else {
            return;
        };
        assert!(until(&mut model, |model| model.tabs.len() == 2));
        server
            .run(&["new-session", "-d", "-s", "work", "/bin/sh"])
            .status()
            .unwrap();
        let listed = sessions(&server.0).expect("tmux is installed");
        assert!(listed.iter().any(|name| name == "work"), "{listed:?}");
        let args = session_args(Session::Attach("work"));
        let second = attach(&mut model, &server.0, &args).expect("tmux");
        assert!(
            until(&mut model, |model| model.tabs.len() == 3
                && attached_here(model, &server.0, "work")),
            "the session did not come in"
        );
        assert_eq!(model.tabs[model.active].group, Some(second));
        assert!(model.tabs.iter().any(|tab| tab.group == Some(first)));
    }

    #[test]
    fn a_named_session_that_exists_is_attached_not_duplicated() {
        named_session_is_attached(Program::Tmux);
    }

    #[test]
    fn a_named_session_that_exists_is_attached_not_duplicated_in_rmux() {
        named_session_is_attached(Program::Rmux);
    }

    fn named_session_is_attached(program: Program) {
        let Some((server, mut model, _first)) = attached_on(program, "named") else {
            return;
        };
        assert!(until(&mut model, |model| model.tabs.len() == 2));
        let args = session_args(Session::Named("named"));
        attach(&mut model, &server.0, &args).expect("tmux");
        assert!(
            until(&mut model, |model| model.tabs.len() == 3),
            "the second client did not come in"
        );
        assert_eq!(sessions(&server.0).unwrap(), ["named"]);
    }

    #[test]
    fn a_session_over_ssh_is_a_group_named_for_its_host() {
        let Some((server, mut model, group)) = attached_over_ssh("remote") else {
            return;
        };
        let host = server.0.host.clone().unwrap();
        assert!(
            until(&mut model, |model| model.tabs.len() == 2),
            "the remote window did not become a tab"
        );
        let label = &model
            .tab_groups
            .iter()
            .find(|g| g.id == group)
            .unwrap()
            .label;
        assert_eq!(*label, format!("remote@{host}"));
        let pane = crate::tabs::surface_of(&model.tabs[model.active]).unwrap();
        model
            .terminal
            .get(pane)
            .unwrap()
            .surface
            .text("uname -s; echo $((6*7))\r");
        assert!(
            until(&mut model, |model| screen(model, pane).contains("\n42")),
            "the answer did not come back: {}",
            screen(&mut model, pane)
        );
    }

    #[test]
    fn a_saved_session_reopens_from_its_profile() {
        let Some((server, mut model, group)) = attached("saved") else {
            return;
        };
        let dir = std::env::temp_dir().join(format!("rune-term-profile-{}", std::process::id()));
        model.config = crate::settings::ConfigFile::default().at_dir(&dir);
        assert!(until(&mut model, |model| model.muxes[0].session.is_some()
            && model.tabs.iter().any(|tab| tab.group == Some(group))));
        let index = model
            .tabs
            .iter()
            .position(|tab| tab.group == Some(group))
            .unwrap();
        ask_profile_name(&mut model, index);
        assert_eq!(model.name_prompt.as_ref().unwrap().edit.text, "saved");
        submit_name(&mut model);
        assert!(model.name_prompt.is_none(), "the name was taken");
        let profile = model.config.profile("saved").expect("the profile");
        let (target, session) = profile_target(profile).expect("a session profile");
        assert_eq!(
            target,
            Target {
                socket: None,
                ..server.0.clone()
            }
        );
        assert_eq!(session.as_deref(), Some("saved"));
        let on_disk = crate::settings::ConfigFile::open(&dir).unwrap();
        assert_eq!(on_disk.profile("saved"), Some(profile));

        ask_profile_name(&mut model, index);
        submit_name(&mut model);
        let refused = model
            .name_prompt
            .as_ref()
            .expect("a taken name keeps the prompt");
        assert!(refused
            .error
            .as_deref()
            .is_some_and(|err| err.contains("exists")));
        assert!(model.tab_groups.iter().any(|g| g.id == group));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_local_profile_names_no_session() {
        assert!(profile_target(&crate::settings::Profile::default()).is_none());
        let remote = crate::settings::Profile {
            backend: "rmux".to_owned(),
            host: Some(" box ".to_owned()),
            session: Some(String::new()),
            ..Default::default()
        };
        let (target, session) = profile_target(&remote).unwrap();
        assert_eq!(target.program, Program::Rmux);
        assert_eq!(target.host.as_deref(), Some("box"));
        assert_eq!(session, None, "a blank session starts a new one each time");
    }

    #[test]
    fn a_profile_host_lists_in_the_menu_once_it_answers() {
        if sessions(&Target::local(Program::Tmux)).is_none() {
            eprintln!("tmux is not installed: skipped");
            return;
        }
        let mut model = crate::model::init();
        model.config.profiles.insert(
            "far".to_owned(),
            crate::settings::Profile {
                backend: "tmux".to_owned(),
                host: Some("rune-term-test.invalid".to_owned()),
                ..Default::default()
            },
        );
        let host = "tmux-new@rune-term-test.invalid";
        assert!(menu(&model, PopupMenu::new()).find(host).is_some());
        let started = std::time::Instant::now();
        while menu(&model, PopupMenu::new())
            .find("tmux-unreachable@rune-term-test.invalid")
            .is_none()
        {
            assert!(
                started.elapsed() < std::time::Duration::from_secs(15),
                "no answer"
            );
            thread::sleep(std::time::Duration::from_millis(50));
        }
    }

    #[test]
    fn a_remote_attach_names_its_host_and_session() {
        let (target, action) = parse_action(Program::Rmux, "attach@me@box:a@b").unwrap();
        assert_eq!(target.host.as_deref(), Some("me@box"));
        assert!(matches!(action, Action::Attach("a@b")));
        let (target, action) = parse_action(Program::Tmux, "attach:a@b").unwrap();
        assert_eq!(target.host, None);
        assert!(matches!(action, Action::Attach("a@b")));
        assert!(parse_action(Program::Tmux, "unreachable@box").is_none());
    }

    #[test]
    fn a_blank_name_closes_the_prompt_and_starts_nothing() {
        let mut model = crate::model::init();
        ask_name(&mut model, Target::local(Program::Tmux));
        model.name_prompt.as_mut().unwrap().edit.text = "  ".to_owned();
        submit_name(&mut model);
        assert!(model.name_prompt.is_none());
        assert!(model.muxes.is_empty());
    }

    #[test]
    fn detaching_drops_the_tabs_and_leaves_the_session_running() {
        let Some((server, mut model, group)) = attached("detach") else {
            return;
        };
        assert!(until(&mut model, |model| model.tabs.len() == 2));
        let index = model.active;
        detach(&mut model, index, &mut Tx::default());
        assert_eq!(model.tabs.len(), 1);
        assert!(model.tab_groups.iter().all(|g| g.id != group));
        assert!(model.muxes.is_empty());
        let deadline = Instant::now() + Duration::from_secs(5);
        let attached_clients = || {
            server
                .run(&["list-clients"])
                .output()
                .map(|out| out.stdout.len())
                .unwrap_or(0)
        };
        while attached_clients() > 0 && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(attached_clients(), 0, "the client did not detach");
        assert_eq!(sessions(&server.0).unwrap(), ["detach"]);
    }

    #[test]
    fn a_dropped_link_greys_the_group_and_reconnects_into_its_tabs() {
        let Some((server, mut model, group)) = attached("dropped") else {
            return;
        };
        assert!(until(&mut model, |model| model.muxes[0].session.is_some()
            && model.tabs.len() == 2));
        let pane = crate::tabs::surface_of(&model.tabs[model.active]).unwrap();
        let out = server
            .run(&["list-clients", "-F", "#{client_pid}"])
            .output()
            .unwrap();
        let pid = String::from_utf8(out.stdout).unwrap();
        let killed = Command::new("kill")
            .args(["-9", pid.trim()])
            .status()
            .unwrap();
        assert!(killed.success());
        let lost = |model: &Model| model.tab_groups.iter().any(|g| g.id == group && g.lost);
        assert!(
            until(&mut model, |model| lost(model)),
            "the group did not grey out"
        );
        assert_eq!(model.tabs.len(), 2, "its tab stays");

        let index = model.active;
        reconnect(&mut model, index);
        assert!(!lost(&model));
        let surface = |model: &Model| crate::tabs::surface_of(&model.tabs[model.active]);
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
        assert_eq!(surface(&model), Some(pane), "the pane reconnected in place");
        assert_eq!(sessions(&server.0).unwrap(), ["dropped"]);
    }

    #[test]
    fn closing_a_window_kills_it_and_keeps_the_rest() {
        let Some((server, mut model, group)) = attached("kill") else {
            return;
        };
        assert!(until(&mut model, |model| model.tabs.len() == 2));
        assert!(open_window(&model));
        assert!(until(&mut model, |model| model.tabs.len() == 3));
        let index = model.active;
        let window = model.tabs[index].window;
        kill_window(&model, index);
        assert!(
            until(&mut model, |model| model.tabs.len() == 2),
            "the killed window kept its tab"
        );
        assert!(model.tabs.iter().all(|tab| tab.window != window));
        assert!(model.tabs.iter().any(|tab| tab.group == Some(group)));
        assert!(
            model.muxes[0].hidden.is_empty(),
            "a killed window is not hidden"
        );
        assert_eq!(sessions(&server.0).unwrap(), ["kill"]);
    }
}
