//! The delegate runs on the PTY thread and on the UI thread from `inject_output`.
//! It enqueues chrome and wakes. It does not write `Model`.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use librio::{Action, ClipboardType, RenderState, SurfaceDelegate, SurfaceDesc, SurfaceId};
use rune::{Tx, Waker};

use crate::chrome::PANE_MARK;
use crate::model::Model;
use crate::split;
use crate::surfaces::{HostSurface, Progress};

static QUEUED_INJECT: Mutex<Option<Vec<u8>>> = Mutex::new(None);
static PUBLISHED: Mutex<Option<Arc<Shared>>> = Mutex::new(None);

pub struct Harness {
    pub rearms: usize,
    /// True when a surface flag is still set, or a wakeup arrived before its flag existed.
    pub frame_flag: bool,
    pub output_len: usize,
}

/// Bytes the headless test injects as child output. `inject_output` does not
/// wake, so `poll` raises that surface's flag itself.
pub fn queue_inject(bytes: &[u8]) {
    *QUEUED_INJECT.lock().expect("inject") = Some(bytes.to_vec());
}

pub(crate) fn take_inject() -> Option<Vec<u8>> {
    QUEUED_INJECT.lock().expect("inject").take()
}

pub(crate) fn publish(shared: &Arc<Shared>) {
    *PUBLISHED.lock().expect("harness") = Some(Arc::clone(shared));
}

pub fn harness() -> Option<Harness> {
    PUBLISHED
        .lock()
        .expect("harness")
        .as_ref()
        .map(|shared| shared.report())
}

struct Flags {
    live: HashMap<SurfaceId, Arc<AtomicBool>>,
    /// Wakeups that arrived before `setup` published the flag.
    pending: HashSet<SurfaceId>,
}

pub struct Shared {
    events: Mutex<Vec<HostEvent>>,
    waker: Mutex<Option<Waker>>,
    flags: Mutex<Flags>,
    output: Mutex<Vec<u8>>,
    rearms: AtomicUsize,
}

enum HostEvent {
    Action {
        id: SurfaceId,
        action: Action,
    },
    Close(SurfaceId),
    Clipboard {
        id: SurfaceId,
        kind: ClipboardType,
        len: usize,
    },
}

struct Delegate {
    shared: Arc<Shared>,
}

impl Shared {
    pub(crate) fn new() -> Arc<Self> {
        Arc::new(Self {
            events: Mutex::new(Vec::new()),
            waker: Mutex::new(None),
            flags: Mutex::new(Flags {
                live: HashMap::new(),
                pending: HashSet::new(),
            }),
            output: Mutex::new(Vec::new()),
            rearms: AtomicUsize::new(0),
        })
    }

    pub(crate) fn delegate(self: &Arc<Self>) -> Arc<dyn SurfaceDelegate> {
        Arc::new(Delegate {
            shared: Arc::clone(self),
        })
    }

    #[cfg(all(test, not(feature = "pty")))]
    pub(crate) fn take_output(&self) -> Vec<u8> {
        std::mem::take(&mut *self.output.lock().expect("output"))
    }

    pub(crate) fn set_waker(&self, waker: Waker) {
        *self.waker.lock().expect("waker") = Some(waker);
    }

    pub(crate) fn insert_flag(&self, id: SurfaceId, flag: Arc<AtomicBool>) {
        let mut flags = self.flags.lock().expect("flags");
        if flags.pending.remove(&id) {
            flag.store(true, Ordering::Release);
        }
        flags.live.insert(id, flag);
    }

    pub(crate) fn remove_flag(&self, id: SurfaceId) {
        let mut flags = self.flags.lock().expect("flags");
        flags.live.remove(&id);
        flags.pending.remove(&id);
    }

    pub(crate) fn note_rearm(&self) {
        self.rearms.fetch_add(1, Ordering::Relaxed);
    }

    fn take_events(&self) -> Vec<HostEvent> {
        std::mem::take(&mut *self.events.lock().expect("events"))
    }

    fn push(&self, event: HostEvent) {
        self.events.lock().expect("events").push(event);
        self.wake();
    }

    fn wake(&self) {
        if let Some(waker) = self.waker.lock().expect("waker").as_ref() {
            waker();
        }
    }

    /// `wakeup` sets the flag and wakes only on the false→true edge.
    fn raise(&self, id: SurfaceId) {
        let mut flags = self.flags.lock().expect("flags");
        if let Some(flag) = flags.live.get(&id).cloned() {
            drop(flags);
            if !flag.swap(true, Ordering::AcqRel) {
                self.wake();
            }
            return;
        }
        if flags.pending.insert(id) {
            drop(flags);
            self.wake();
        }
    }

    fn report(&self) -> Harness {
        let flags = self.flags.lock().expect("flags");
        let frame_flag = !flags.pending.is_empty()
            || flags.live.values().any(|flag| flag.load(Ordering::Acquire));
        Harness {
            rearms: self.rearms.load(Ordering::Acquire),
            frame_flag,
            output_len: self.output.lock().expect("output").len(),
        }
    }
}

impl SurfaceDelegate for Delegate {
    fn wakeup(&self, surface: SurfaceId) {
        self.shared.raise(surface);
    }

    fn action(&self, surface: SurfaceId, action: Action) {
        self.shared.push(HostEvent::Action {
            id: surface,
            action,
        });
    }

    fn clipboard_write(&self, surface: SurfaceId, kind: ClipboardType, text: String) {
        let len = text.len();
        drop(text);
        self.shared.push(HostEvent::Clipboard {
            id: surface,
            kind,
            len,
        });
    }

    fn close_surface(&self, surface: SurfaceId) {
        self.shared.push(HostEvent::Close(surface));
    }

    fn output(&self, _surface: SurfaceId, bytes: &[u8]) {
        self.shared
            .output
            .lock()
            .expect("output")
            .extend_from_slice(bytes);
    }
}

pub fn setup(model: &mut Model, waker: Waker) {
    model.shared.set_waker(waker);
    let id = open_host(model, PANE_MARK, 0, 0);
    crate::chrome_view::attach_initial(model, id);
}

/// One surface, one damage flag, one effect mark.
pub(crate) fn open_host(model: &mut Model, mark: u32, cols: u16, rows: u16) -> SurfaceId {
    let flag = Arc::new(AtomicBool::new(false));
    let surface = model
        .engine
        .create_surface(&SurfaceDesc::default())
        .expect("create surface");
    let id = surface.id();
    model.shared.insert_flag(id, Arc::clone(&flag));
    let render = RenderState::new(&surface);
    model.surfaces.insert(HostSurface {
        surface,
        render,
        mark,
        title: "rune-term".to_string(),
        bell: false,
        progress: None,
        blink: true,
        frame_flag: flag,
        resized: AtomicBool::new(false),
        cols,
        rows,
        cell_px: (0, 0),
        wheel_remainder: 0.0,
        pointer_button: None,
        selecting: false,
    });
    id
}

pub fn poll(model: &mut Model, _now: f64, tx: &mut Tx) -> bool {
    let events = model.shared.take_events();
    let mut chrome = false;
    for event in events {
        if apply(model, event, tx) {
            chrome = true;
        }
    }
    if let Some(bytes) = model.pending_inject.take() {
        if let Some(host) = model.surfaces.primary_mut() {
            host.surface.inject_output(&bytes);
            host.frame_flag.store(true, Ordering::Release);
        }
    }
    chrome
}

fn apply(model: &mut Model, event: HostEvent, tx: &mut Tx) -> bool {
    match event {
        HostEvent::Action { id, action } => apply_action(model, id, action),
        HostEvent::Close(id) => drop_closed(model, id, tx),
        HostEvent::Clipboard { kind, len, .. } => {
            eprintln!("clipboard write dropped ({kind:?}, {len} bytes)");
            #[cfg(all(test, not(feature = "pty")))]
            record_clipboard_drop(kind, len);
            false
        }
    }
}

/// The shell exited. Drop its leaf. The window closes with the last surface.
fn drop_closed(model: &mut Model, id: SurfaceId, tx: &mut Tx) -> bool {
    model.shared.remove_flag(id);
    if model.surfaces.remove(id).is_none() {
        return false;
    }
    drop_leaf(model, id);
    if model.surfaces.len() == 0 {
        tx.close_window();
    } else {
        crate::tabs::show_active(model);
    }
    true
}

fn drop_leaf(model: &mut Model, surface: SurfaceId) {
    for index in 0..model.tabs.len() {
        match split::detach(model.tabs[index].root.clone(), surface) {
            split::Detach::Missing(_) => continue,
            split::Detach::Rest(root) => {
                if !split::contains_leaf(&root, model.tabs[index].focused) {
                    model.tabs[index].focused = split::first_leaf(&root);
                }
                model.tabs[index].root = root;
                return;
            }
            split::Detach::Empty => {
                model.tabs.remove(index);
                if model.tabs.is_empty() {
                    model.active = 0;
                } else if model.active > index || model.active >= model.tabs.len() {
                    model.active = model.active.saturating_sub(1);
                }
                return;
            }
        }
    }
}

fn apply_action(model: &mut Model, id: SurfaceId, action: Action) -> bool {
    match action {
        Action::SetTitle { title, .. } => {
            let changed = match model.surfaces.get_mut(id) {
                Some(host) if host.title != title => {
                    host.title = title.clone();
                    true
                }
                Some(_) => false,
                None => return false,
            };
            if changed {
                crate::tabs::set_title(model, id, title);
            }
            changed
        }
        Action::RingBell => {
            let Some(host) = model.surfaces.get_mut(id) else {
                return false;
            };
            if host.bell {
                return false;
            }
            host.bell = true;
            true
        }
        Action::CursorBlinkingChange => {
            let Some(host) = model.surfaces.get_mut(id) else {
                return false;
            };
            host.blink = !host.blink;
            true
        }
        Action::Progress { state, value } => {
            let Some(host) = model.surfaces.get_mut(id) else {
                return false;
            };
            let next = Some(Progress { state, value });
            if host.progress == next {
                return false;
            }
            host.progress = next;
            true
        }
    }
}

#[cfg(all(test, not(feature = "pty")))]
static CLIPBOARD_DROP: Mutex<Option<(ClipboardType, usize)>> = Mutex::new(None);

#[cfg(all(test, not(feature = "pty")))]
fn record_clipboard_drop(kind: ClipboardType, len: usize) {
    *CLIPBOARD_DROP.lock().expect("clipboard drop") = Some((kind, len));
}

#[cfg(all(test, not(feature = "pty")))]
mod tests {
    use super::*;
    use crate::chrome::Node;
    use crate::model;
    use crate::split::Axis;
    use rune::{Env, Runtime, Shell};

    fn host() -> Model {
        let mut model = model::init();
        setup(&mut model, std::sync::Arc::new(|| {}));
        model
    }

    fn latch(model: Model, expect_chrome: bool) -> Runtime<Model> {
        let mut rt = Runtime::new(
            model,
            crate::chrome::view,
            (800.0, 500.0),
            Env::new(true, 1.0),
        );
        rt.update(|model, tx| {
            assert_eq!(poll(model, 0.0, tx), expect_chrome);
        });
        rt
    }

    #[test]
    fn osc52_drops_the_payload_and_keeps_the_length() {
        *CLIPBOARD_DROP.lock().expect("clipboard drop") = None;
        let mut model = model::init();
        setup(&mut model, std::sync::Arc::new(|| {}));
        {
            let host = model.surfaces.primary_mut().unwrap();
            // "hello" in base64. The child asks the host to store it.
            host.surface.inject_output(b"\x1b]52;c;aGVsbG8=\x07");
        }
        poll(&mut model, 0.0, &mut Tx::default());
        assert!(model.shared.take_output().is_empty());
        assert_eq!(
            *CLIPBOARD_DROP.lock().expect("clipboard drop"),
            Some((ClipboardType::Clipboard, 5))
        );
    }

    #[test]
    fn the_last_shell_closes_the_window() {
        let model = host();
        let id = model.surfaces.primary().unwrap().surface.id();
        model.shared.delegate().close_surface(id);
        let rt = latch(model, true);
        assert!(rt.state.surfaces.primary().is_none());
        assert!(rt.state.tabs.is_empty());
        assert_eq!(rt.shell_request, Some(Shell::Close));
    }

    #[test]
    fn a_remaining_pane_keeps_the_window() {
        let mut model = host();
        assert!(crate::chrome_view::split_focused(
            &mut model,
            Axis::Horizontal
        ));
        let right = match &model.tabs[0].root {
            Node::Split { right, .. } => match right.as_ref() {
                Node::Leaf(leaf) => leaf.surface,
                other => panic!("expected the new leaf, got {other:?}"),
            },
            other => panic!("expected a split, got {other:?}"),
        };
        let left = model.surfaces.primary().unwrap().surface.id();
        assert_ne!(left, right);
        model.shared.delegate().close_surface(right);
        let rt = latch(model, true);
        assert!(rt.shell_request.is_none());
        assert_eq!(rt.state.surfaces.len(), 1);
        assert_eq!(rt.state.surfaces.primary().unwrap().surface.id(), left);
        assert!(matches!(rt.state.tabs[0].root, Node::Leaf(_)));
        assert_eq!(rt.state.tabs[0].focused, crate::chrome::LEAF);
    }

    #[test]
    fn an_unknown_close_leaves_the_window() {
        let model = host();
        model.shared.delegate().close_surface(usize::MAX);
        let rt = latch(model, false);
        assert!(rt.shell_request.is_none());
        assert_eq!(rt.state.surfaces.len(), 1);
        assert_eq!(rt.state.tabs.len(), 1);
    }
}
