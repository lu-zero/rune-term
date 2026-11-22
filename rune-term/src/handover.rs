//! The keeper registry: shells that survive the app, and how the next
//! instance adopts them.
//!
//! A local shell is handed to a `rune-term-keeper` at exit, which holds the
//! master and serves it to the next instance. `adopt_all` scans the registry
//! at startup; `hand_over_fd` spawns the keeper at exit.

use std::fs::{self, File};
use std::io::{BufRead, BufReader, Read, Write};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use etcetera::BaseStrategy;
use rune_term_keeper::{recv_master, send_fd};

use crate::chrome::PANE_MARK;
use crate::delegate;
use crate::model::Model;

/// One shell a keeper holds: `fd` is the master, `blob` the output it read
/// while waiting for the next instance.
pub(crate) struct Adopted {
    pub(crate) fd: File,
    pub(crate) shell: u32,
    pub(crate) profile: Option<String>,
    pub(crate) winsize: Winsize,
    pub(crate) blob: Vec<u8>,
}

type Winsize = (u16, u16, u16, u16);

/// How long an adoption waits for the keeper to answer.
const ADOPT_TIMEOUT: Duration = Duration::from_secs(5);

/// The CLI asked for the handover (`--handover` or `rune-term-ctl handover`);
/// default off.
static REQUESTED: AtomicBool = AtomicBool::new(false);

/// Marker the flip writes so the next launch adopts the keepers; a ctl flip
/// sticks across instances.
const HANDOVER_MARKER: &str = "handover.on";

pub(crate) fn set_requested(on: bool) {
    REQUESTED.store(on, Ordering::Relaxed);
    if on {
        if !keeper_found() {
            eprintln!(
                "rune-term: the shell keeper was not found; the handover will not happen at exit"
            );
        }
        if let Some(dir) = keepers_dir() {
            let _ = fs::create_dir_all(&dir);
            let _ = File::create(dir.join(HANDOVER_MARKER));
        }
    }
}

/// Whether the keeper is findable now: a file next to the running tree, or
/// the bare name on PATH.
fn keeper_found() -> bool {
    let exe = keeper_exe();
    if exe.is_file() {
        return true;
    }
    exe.to_str() == Some("rune-term-keeper") && path_has("rune-term-keeper")
}

fn path_has(name: &str) -> bool {
    std::env::var_os("PATH").is_some_and(|paths| {
        std::env::split_paths(paths.as_os_str()).any(|dir| dir.join(name).is_file())
    })
}

pub(crate) fn requested() -> bool {
    REQUESTED.load(Ordering::Relaxed)
        || keepers_dir().is_some_and(|dir| dir.join(HANDOVER_MARKER).is_file())
}

/// The shells waiting for the next instance.
pub(crate) fn adopt_all(dir: &Path) -> Vec<Adopted> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut socks: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|sock| sock.extension().and_then(|ext| ext.to_str()) == Some("sock"))
        .collect();
    socks.sort();
    socks
        .into_iter()
        .filter_map(|sock| connect_and_adopt(&sock))
        .collect()
}

/// The shells the previous instance handed over, as panes of this one.
pub(crate) fn adopt(model: &mut Model) {
    let Some(dir) = keepers_dir() else {
        return;
    };
    for ad in adopt_all(&dir) {
        let profile = ad.profile.clone();
        if model.terminal.primary().is_none() {
            let id = delegate::open_adopted(model, PANE_MARK, ad);
            crate::chrome_view::attach_initial(model, id);
            if let Some(profile) = profile {
                model.tabs[model.active].profile = profile;
            }
        } else {
            crate::tabs::open_adopted(model, ad);
        }
    }
}

fn connect_and_adopt(sock: &Path) -> Option<Adopted> {
    let stream = UnixStream::connect(sock).ok()?;
    stream.set_read_timeout(Some(ADOPT_TIMEOUT)).ok()?;
    // macOS delivers the fd only if it is queued when recv_master runs; let
    // the keeper's poll accept and send first.
    std::thread::sleep(Duration::from_millis(200));
    let fd = recv_master(stream.as_raw_fd())?;
    let mut reader = BufReader::new(stream);
    let mut len = [0u8; 4];
    reader.read_exact(&mut len).ok()?;
    let mut blob = vec![0u8; u32::from_ne_bytes(len) as usize];
    reader.read_exact(&mut blob).ok()?;
    let mut line = String::new();
    reader.read_line(&mut line).ok()?;
    let shell = line.strip_prefix("shell=")?.trim().parse().ok()?;
    let mut line = String::new();
    reader.read_line(&mut line).ok()?;
    let profile = line
        .strip_prefix("profile=")
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_owned);
    reader.get_ref().write_all(&[1]).ok()?;
    Some(Adopted {
        fd: unsafe { File::from_raw_fd(fd) },
        shell,
        profile,
        winsize: crate::link::winsize_of(fd),
        blob,
    })
}

/// Spawn the keeper for `master` at exit: the keeper holds the master while no
/// rune-term runs, and the next instance adopts it from the registry.
pub(crate) fn hand_over_fd(master: File, shell: u32, profile: Option<String>, dir: &Path) {
    if fs::create_dir_all(dir).is_err() {
        eprintln!(
            "rune-term: cannot create the keepers directory {}",
            dir.display()
        );
        return;
    }
    let sock = dir.join(format!("{shell}.sock"));
    let _ = fs::remove_file(&sock);
    let (sender, keeper_stdin) = socketpair();
    let sender = unsafe { UnixStream::from_raw_fd(sender) };
    if send_fd(sender.as_raw_fd(), master.as_raw_fd()).is_err() {
        return;
    }
    drop(sender);
    let mut command = Command::new(keeper_exe());
    command
        .arg("--socket")
        .arg(&sock)
        .arg("--shell")
        .arg(shell.to_string());
    if let Some(profile) = &profile {
        command.arg("--profile").arg(profile);
    }
    command
        .stdin(Stdio::from(unsafe { OwnedFd::from_raw_fd(keeper_stdin) }))
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if let Err(err) = command.spawn() {
        eprintln!("rune-term: cannot start the shell keeper: {err}");
    }
}

/// The keepers directory: the user's runtime dir if it has one, else state.
pub(crate) fn keepers_dir() -> Option<PathBuf> {
    let xdg = etcetera::base_strategy::Xdg::new().ok()?;
    Some(
        xdg.runtime_dir()
            .or_else(|| xdg.state_dir())?
            .join("rune-term")
            .join("keepers"),
    )
}

fn socketpair() -> (RawFd, RawFd) {
    let mut fds = [0 as libc::c_int; 2];
    if unsafe { libc::socketpair(libc::AF_UNIX, libc::SOCK_STREAM, 0, fds.as_mut_ptr()) } < 0 {
        return (-1, -1);
    }
    (fds[0], fds[1])
}

/// The sibling `rune-term-keeper`, next to the running `rune-term`; a test
/// binary walks up to `target/debug`.
fn keeper_exe() -> PathBuf {
    if let Some(path) = std::env::var_os("CARGO_BIN_EXE_rune-term-keeper") {
        return PathBuf::from(path);
    }
    let mut dir = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf));
    while let Some(dir_path) = dir {
        let path = dir_path.join("rune-term-keeper");
        if path.is_file() {
            return path;
        }
        dir = dir_path.parent().map(Path::to_path_buf);
    }
    PathBuf::from("rune-term-keeper")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn within<T>(secs: u64, mut done: impl FnMut() -> Option<T>) -> Option<T> {
        let deadline = std::time::Instant::now() + Duration::from_secs(secs);
        while std::time::Instant::now() < deadline {
            if let Some(value) = done() {
                return Some(value);
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        None
    }

    #[test]
    fn the_keeper_hands_the_master_back_for_adoption() {
        let dir = std::env::temp_dir().join(format!("rune-term-keepers-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let shared = delegate::Shared::new();
        let engine = term_vt::Engine::new(shared.delegate());
        let desc = term_vt::SurfaceDesc {
            shell: Some("/bin/sh".to_owned()),
            args: vec![
                "-c".to_owned(),
                r#"read line; printf 'got:%s' "$line""#.to_owned(),
            ],
            ..term_vt::SurfaceDesc::default()
        };
        let surface = std::sync::Arc::new(engine.create_surface(&desc).expect("create surface"));
        let mut link = crate::link::PtyLink::spawn(
            std::sync::Arc::clone(&surface),
            &desc,
            std::sync::Arc::clone(&shared),
        )
        .expect("spawn the shell");
        let (master, shell) = link.hand_over().expect("hand the master over");
        hand_over_fd(master, shell, None, &dir);
        let Some(mut adopted) = within(5, || adopt_all(&dir).pop()) else {
            panic!("the keeper never answered");
        };
        assert_eq!(adopted.shell, shell);
        adopted
            .fd
            .write_all(b"hi\r")
            .expect("write the adopted master");
        let mut render = term_vt::RenderState::new(&surface);
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            render.update();
            if (0..render.lines()).any(|line| render.text_row(line).contains("got:hi")) {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "the shell never answered"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        let _ = fs::remove_dir_all(&dir);
    }
}
