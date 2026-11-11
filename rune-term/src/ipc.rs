//! Local control IPC server (read-only scaffolding).
//!
//! A background thread accepts NDJSON connections on an owner-scoped Unix
//! socket and serves protocol requests. Requests that need live UI state are
//! answered with a stub until main-thread bridging is wired.

use std::io::{BufReader, BufWriter};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};

#[cfg(unix)]
use std::os::unix::net::UnixListener;

use term_control::codec::{read_message, write_message};
use term_control::{AppInfo, ProfileInfo, Request, Response, PROTOCOL_VERSION};

use crate::settings::ConfigFile;

pub struct Server {
    path: Option<PathBuf>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(path) = &self.path {
            term_control::transport::cleanup_socket(path);
            // Unblock accept by connecting to self on Unix.
            #[cfg(unix)]
            {
                let _ = std::os::unix::net::UnixStream::connect(path);
            }
        }
        if let Some(handle) = self.thread.take() {
            let _ = handle.join();
        } else if let Some(path) = &self.path {
            term_control::transport::cleanup_socket(path);
        }
    }
}

pub fn start() -> Server {
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
        let _ = path;
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
            .spawn(move || serve(listener, path_thread, stop_thread))
            .ok();

        Server {
            path: Some(path),
            stop,
            thread,
        }
    }
}

#[cfg(unix)]
fn serve(listener: UnixListener, path: PathBuf, stop: Arc<AtomicBool>) {
    // Allow periodic stop checks in simple implementations if needed.
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
        let _ = thread::Builder::new()
            .name("rune-term-ipc-conn".to_owned())
            .spawn(move || handle_connection(stream, path));
    }
    term_control::transport::cleanup_socket(&path);
}

#[cfg(unix)]
fn handle_connection(stream: std::os::unix::net::UnixStream, socket_path: PathBuf) {
    let reader_stream = match stream.try_clone() {
        Ok(s) => s,
        Err(_) => return,
    };
    let mut reader = BufReader::new(reader_stream);
    let mut writer = BufWriter::new(stream);

    // Simple round-trip loop: one or more requests on the same connection.
    loop {
        let req: Option<Request> = match read_message(&mut reader) {
            Ok(r) => r,
            Err(_) => break,
        };
        let Some(req) = req else { break };
        let resp = handle_request(req, &socket_path);
        if write_message(&mut writer, &resp).is_err() {
            break;
        }
    }
}

#[cfg(unix)]
fn handle_request(req: Request, socket_path: &PathBuf) -> Response {
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
        Request::ListTabs => Response::Error {
            code: "not_implemented".to_owned(),
            message: "live tab/window query bridge not implemented yet".to_owned(),
        },
        Request::Subscribe { .. } => Response::Error {
            code: "not_implemented".to_owned(),
            message: "event subscription bridge not implemented yet".to_owned(),
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
