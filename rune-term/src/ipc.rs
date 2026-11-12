//! Local control IPC server (read-only scaffolding).
//!
//! A background thread accepts NDJSON connections on an owner-scoped Unix
//! socket and serves protocol requests. Requests that require live UI state
//! are bridged to the main thread via the shared control queue.

use std::io::{BufReader, BufWriter};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::thread::{self, JoinHandle};
use std::time::Duration;

#[cfg(unix)]
use std::os::unix::net::UnixListener;

use term_control::codec::{read_message, write_message};
use term_control::{
    AppInfo, PaneInfo, ProfileInfo, Request, Response, TabInfo, WindowInfo, PROTOCOL_VERSION,
};

use crate::delegate::Shared;
use crate::model::Model;
use crate::settings::ConfigFile;

pub struct Server {
    path: Option<PathBuf>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        #[cfg(unix)]
        if let Some(path) = &self.path {
            // Wake the blocking accept loop before removing the socket.
            let _ = std::os::unix::net::UnixStream::connect(path);
        }
        if let Some(handle) = self.thread.take() {
            let _ = handle.join();
        }
        if let Some(path) = &self.path {
            term_control::transport::cleanup_socket(path);
        }
    }
}

pub fn start(shared: Arc<Shared>) -> Server {
    if cfg!(test) || std::env::var_os("RUNE_TERM_DISABLE_IPC").is_some() {
        return Server {
            path: None,
            stop: Arc::new(AtomicBool::new(true)),
            thread: None,
        };
    }

    let Some(path) = term_control::default_socket_path() else {
        return Server {
            path: None,
            stop: Arc::new(AtomicBool::new(true)),
            thread: None,
        };
    };

    #[cfg(not(unix))]
    {
        let _ = (path, shared);
        return Server {
            path: None,
            stop: Arc::new(AtomicBool::new(true)),
            thread: None,
        };
    }

    #[cfg(unix)]
    {
        let listener = match term_control::transport::bind(&path) {
            Ok(l) => l,
            Err(err) => {
                eprintln!(
                    "rune-term: failed to bind control socket {}: {err}",
                    path.display()
                );
                return Server {
                    path: Some(path),
                    stop: Arc::new(AtomicBool::new(true)),
                    thread: None,
                };
            }
        };

        let stop = Arc::new(AtomicBool::new(false));
        let stop_thread = Arc::clone(&stop);
        let path_thread = path.clone();
        let thread = thread::Builder::new()
            .name("rune-term-ipc".to_owned())
            .spawn(move || serve(listener, path_thread, stop_thread, shared))
            .ok();

        Server {
            path: Some(path),
            stop,
            thread,
        }
    }
}

#[cfg(unix)]
fn serve(listener: UnixListener, path: PathBuf, stop: Arc<AtomicBool>, shared: Arc<Shared>) {
    let _ = listener.set_nonblocking(false);
    for stream in listener.incoming() {
        if stop.load(Ordering::Relaxed) {
            break;
        }
        let stream = match stream {
            Ok(s) => s,
            Err(_) => {
                if stop.load(Ordering::Relaxed) {
                    break;
                }
                continue;
            }
        };
        let path = path.clone();
        let shared = Arc::clone(&shared);
        let _ = thread::Builder::new()
            .name("rune-term-ipc-conn".to_owned())
            .spawn(move || handle_connection(stream, path, shared));
    }
    term_control::transport::cleanup_socket(&path);
}

#[cfg(unix)]
fn handle_connection(
    stream: std::os::unix::net::UnixStream,
    socket_path: PathBuf,
    shared: Arc<Shared>,
) {
    let reader_stream = match stream.try_clone() {
        Ok(s) => s,
        Err(_) => return,
    };
    let mut reader = BufReader::new(reader_stream);
    let mut writer = BufWriter::new(stream);

    loop {
        let req: Option<Request> = match read_message(&mut reader) {
            Ok(r) => r,
            Err(_) => break,
        };
        let Some(req) = req else { break };
        let resp = handle_request(req, &socket_path, &shared);
        if write_message(&mut writer, &resp).is_err() {
            break;
        }
    }
}

#[cfg(unix)]
fn handle_request(req: Request, socket_path: &PathBuf, shared: &Arc<Shared>) -> Response {
    match req {
        Request::Hello(hello) => Response::Hello {
            protocol_version: PROTOCOL_VERSION.max(hello.protocol_version),
            app: Some(app_info(socket_path)),
        },
        Request::Info => Response::Info {
            app: app_info(socket_path),
        },
        Request::ListProfiles => {
            let cfg = ConfigFile::load();
            let default_profile = cfg.default_profile_name().to_owned();
            let mut profiles: Vec<ProfileInfo> = cfg
                .profile_names()
                .map(|name| profile_info(&cfg, name))
                .collect();
            profiles.sort_by(|a, b| a.name.cmp(&b.name));
            Response::Profiles {
                default_profile,
                profiles,
            }
        }
        Request::GetProfile { name } => {
            let cfg = ConfigFile::load();
            let profile = if cfg.profile(&name).is_some() {
                Some(profile_info(&cfg, &name))
            } else {
                None
            };
            Response::Profile { profile }
        }
        Request::ListTabs | Request::Subscribe { .. } => bridge_to_ui(req, shared),
    }
}

#[cfg(unix)]
fn bridge_to_ui(req: Request, shared: &Arc<Shared>) -> Response {
    let (tx, rx) = mpsc::sync_channel(1);
    shared.enqueue_control(req, tx);
    match rx.recv_timeout(Duration::from_secs(2)) {
        Ok(resp) => resp,
        Err(_) => Response::Error {
            code: "timeout".to_owned(),
            message: "timed out waiting for UI response".to_owned(),
        },
    }
}

#[cfg(unix)]
fn app_info(socket_path: &PathBuf) -> AppInfo {
    AppInfo {
        name: "rune-term".to_owned(),
        version: env!("CARGO_PKG_VERSION").to_owned(),
        pid: Some(std::process::id()),
        socket: Some(socket_path.clone()),
    }
}

#[cfg(unix)]
fn profile_info(cfg: &ConfigFile, name: &str) -> ProfileInfo {
    let is_default = cfg.default_profile_name() == name;
    let profile = cfg.profile(name).cloned().unwrap_or_default();
    let resolved = cfg.resolve_profile(name);
    let (shell_program, shell_args) = match &profile.shell {
        Some(shell) => (shell.program.clone(), shell.args.clone()),
        None => (None, Vec::new()),
    };
    ProfileInfo {
        name: name.to_owned(),
        is_default,
        font_family: profile.font_family,
        font_size: profile.font_size.or(Some(resolved.font_size)),
        theme: profile.theme.or(Some(resolved.theme)),
        backend: resolved.backend,
        shell_program,
        shell_args,
        cwd: profile.cwd.or(resolved.cwd),
    }
}

/// Handle a control request that requires live UI/model state.
/// Called from the UI thread (poll).
pub(crate) fn handle_control_request(model: &mut Model, req: Request) -> Response {
    match req {
        Request::ListTabs => Response::Tabs {
            windows: vec![snapshot_window(model)],
        },
        Request::Info => Response::Info {
            app: AppInfo {
                name: "rune-term".to_owned(),
                version: env!("CARGO_PKG_VERSION").to_owned(),
                pid: Some(std::process::id()),
                socket: term_control::default_socket_path(),
            },
        },
        Request::Subscribe { .. } => Response::Error {
            code: "not_implemented".to_owned(),
            message: "event subscription bridge not implemented yet".to_owned(),
        },
        Request::Hello(hello) => Response::Hello {
            protocol_version: PROTOCOL_VERSION.max(hello.protocol_version),
            app: Some(AppInfo {
                name: "rune-term".to_owned(),
                version: env!("CARGO_PKG_VERSION").to_owned(),
                pid: Some(std::process::id()),
                socket: term_control::default_socket_path(),
            }),
        },
        Request::ListProfiles => {
            let cfg = ConfigFile::load();
            let default_profile = cfg.default_profile_name().to_owned();
            let mut profiles: Vec<ProfileInfo> = cfg
                .profile_names()
                .map(|name| profile_info(&cfg, name))
                .collect();
            profiles.sort_by(|a, b| a.name.cmp(&b.name));
            Response::Profiles {
                default_profile,
                profiles,
            }
        }
        Request::GetProfile { name } => {
            let cfg = ConfigFile::load();
            let profile = if cfg.profile(&name).is_some() {
                Some(profile_info(&cfg, &name))
            } else {
                None
            };
            Response::Profile { profile }
        }
    }
}

fn snapshot_window(model: &Model) -> WindowInfo {
    let tabs = model
        .tabs
        .iter()
        .enumerate()
        .map(|(index, tab)| snapshot_tab(model, index, tab))
        .collect();
    WindowInfo {
        id: "window-1".to_owned(),
        focused: true,
        tabs,
    }
}

fn snapshot_tab(model: &Model, index: usize, tab: &crate::chrome::Tab) -> TabInfo {
    let panes = collect_panes(model, &tab.root, tab.focused);
    TabInfo {
        id: format!("tab-{}", index + 1),
        index,
        title: tab.title.clone(),
        profile: tab.profile.clone(),
        active: index == model.active,
        panes,
    }
}

fn collect_panes(
    model: &Model,
    node: &crate::chrome::Node,
    focused: crate::chrome::LeafId,
) -> Vec<PaneInfo> {
    let mut out = Vec::new();
    collect_panes_inner(model, node, focused, &mut out);
    out
}

fn collect_panes_inner(
    model: &Model,
    node: &crate::chrome::Node,
    focused: crate::chrome::LeafId,
    out: &mut Vec<PaneInfo>,
) {
    match node {
        crate::chrome::Node::Leaf(leaf) => {
            let (cols, rows, backend, session) =
                if let Some(host) = model.surfaces.get(leaf.surface) {
                    (host.cols, host.rows, "local".to_owned(), None)
                } else {
                    (leaf.cols, leaf.rows, "local".to_owned(), None)
                };
            out.push(PaneInfo {
                id: format!("pane-{}", leaf.id.0),
                focused: leaf.id == focused,
                cols,
                rows,
                backend,
                session,
            });
        }
        crate::chrome::Node::Split { left, right, .. } => {
            collect_panes_inner(model, left, focused, out);
            collect_panes_inner(model, right, focused, out);
        }
    }
}
