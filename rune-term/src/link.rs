//! A local shell on a PTY. librio is built without its own PTY, so the shell's
//! output is read here into `inject_output`, and `SurfaceDelegate::output` is
//! written back to it. A shell that outlived the app, adopted from a keeper,
//! is the same over the keeper's master.

use std::fs::File;
use std::io::{self, Read, Write};
use std::os::fd::{AsRawFd, FromRawFd};
use std::sync::{mpsc, Arc};
use std::thread;
use std::time::Duration;

use librio::{Surface, SurfaceDesc, SurfaceId};
use teletypewriter::Pty;

use crate::delegate::Shared;

pub(crate) type Winsize = (u16, u16, u16, u16);

/// Where the master lives: the spawned shell, or a keeper-held shell.
enum Core {
    /// The spawned shell. `None` once a hand-over took the master.
    Spawned(Option<Pty>),
    /// A shell a keeper holds. The link's fd is closed on drop.
    Adopted { fd: Option<File>, shell: u32 },
}

pub struct PtyLink {
    core: Core,
    input: mpsc::Sender<Vec<u8>>,
    sizes: mpsc::Sender<Winsize>,
    winsize: Winsize,
}

impl PtyLink {
    pub fn spawn(
        surface: Arc<Surface>,
        desc: &SurfaceDesc,
        shared: Arc<Shared>,
    ) -> io::Result<Self> {
        let pty = teletypewriter::create_pty_with_spawn(
            desc.shell.as_deref(),
            desc.args.clone(),
            &desc.working_dir,
            Some(env()),
            desc.cols,
            desc.rows,
            desc.pixel_width,
            desc.pixel_height,
        )?;
        let fd = *pty.child.id;
        // teletypewriter opens the main side non-blocking for its poll loop.
        // These two threads block on it instead.
        unsafe {
            let flags = libc::fcntl(fd, libc::F_GETFL);
            libc::fcntl(fd, libc::F_SETFL, flags & !libc::O_NONBLOCK);
        }
        let id = surface.id();
        let reader = dup(fd)?;
        let writer = dup(fd)?;
        let sized = dup(fd)?;
        reader_thread(reader, surface, shared, id)?;
        let (input, queued) = mpsc::channel::<Vec<u8>>();
        writer_thread(writer, id, queued)?;
        let (sizes, pending) = mpsc::channel::<Winsize>();
        size_thread(sized, id, pending)?;
        Ok(Self {
            core: Core::Spawned(Some(pty)),
            input,
            sizes,
            winsize: (desc.cols, desc.rows, desc.pixel_width, desc.pixel_height),
        })
    }

    /// A shell a keeper holds: `fd` is the master it handed back, `shell` its
    /// pid. The link reads and writes the master; dropping it closes the fd.
    pub(crate) fn adopted(
        surface: Arc<Surface>,
        shared: Arc<Shared>,
        fd: File,
        shell: u32,
        winsize: Winsize,
    ) -> io::Result<Self> {
        let id = surface.id();
        let reader = dup(fd.as_raw_fd())?;
        let writer = dup(fd.as_raw_fd())?;
        let sized = dup(fd.as_raw_fd())?;
        reader_thread(reader, surface, shared, id)?;
        let (input, queued) = mpsc::channel::<Vec<u8>>();
        writer_thread(writer, id, queued)?;
        let (sizes, pending) = mpsc::channel::<Winsize>();
        size_thread(sized, id, pending)?;
        Ok(Self {
            core: Core::Adopted {
                fd: Some(fd),
                shell,
            },
            input,
            sizes,
            winsize,
        })
    }

    /// Take the master, so a keeper can hold it: the next instance adopts it.
    pub(crate) fn hand_over(&mut self) -> Option<(File, u32)> {
        match &mut self.core {
            Core::Spawned(slot) => {
                let pty = slot.take()?;
                let master = dup(*pty.child.id).ok()?;
                let shell = u32::try_from(*pty.child.pid).ok()?;
                // The link drops without hanging up: the keeper holds the master.
                std::mem::forget(pty);
                Some((master, shell))
            }
            Core::Adopted { fd, shell } => Some((dup(fd.as_ref()?.as_raw_fd()).ok()?, *shell)),
        }
    }

    pub(crate) fn is_adopted(&self) -> bool {
        matches!(self.core, Core::Adopted { .. })
    }

    /// The program in the foreground, unless it is the shell itself.
    pub fn running(&self) -> Option<String> {
        let (fd, shell) = match &self.core {
            Core::Spawned(Some(pty)) => (*pty.child.id, *pty.child.pid),
            Core::Spawned(None) => return None,
            Core::Adopted {
                fd: Some(fd),
                shell,
            } => (fd.as_raw_fd(), *shell as libc::pid_t),
            Core::Adopted { fd: None, .. } => return None,
        };
        let group = unsafe { libc::tcgetpgrp(fd) };
        if group <= 0 || group == shell {
            return None;
        }
        Some(teletypewriter::foreground_process_name(fd, shell as u32))
            .filter(|name| !name.is_empty())
    }

    pub fn write(&self, bytes: &[u8]) {
        let _ = self.input.send(bytes.to_vec());
    }

    /// The grid has already reflowed; the shell hears of it once resizes settle.
    pub fn resize(&mut self, winsize: Winsize) {
        if winsize == self.winsize {
            return;
        }
        self.winsize = winsize;
        let _ = self.sizes.send(winsize);
    }
}

impl Drop for PtyLink {
    fn drop(&mut self) {
        match &mut self.core {
            // Dropping the Pty hangs up and reaps the shell.
            Core::Spawned(slot) => drop(slot.take()),
            Core::Adopted { fd, .. } => drop(fd.take()),
        }
    }
}

fn reader_thread(
    mut reader: File,
    surface: Arc<Surface>,
    shared: Arc<Shared>,
    id: SurfaceId,
) -> io::Result<()> {
    thread::Builder::new()
        .name(format!("pty-read-{id}"))
        .spawn(move || {
            let mut buf = vec![0u8; 64 * 1024];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        surface.inject_output(&buf[..n]);
                        shared.raise(id);
                    }
                    Err(err) if err.kind() == io::ErrorKind::Interrupted => {}
                    // macOS reports the shell's exit as EIO.
                    Err(_) => break,
                }
            }
            shared.closed(id);
        })
        .map(|_| ())
}

fn writer_thread(
    mut writer: File,
    id: SurfaceId,
    queued: mpsc::Receiver<Vec<u8>>,
) -> io::Result<()> {
    thread::Builder::new()
        .name(format!("pty-write-{id}"))
        .spawn(move || {
            for bytes in queued {
                if writer.write_all(&bytes).is_err() {
                    break;
                }
            }
        })
        .map(|_| ())
}

fn size_thread(sized: File, id: SurfaceId, pending: mpsc::Receiver<Winsize>) -> io::Result<()> {
    thread::Builder::new()
        .name(format!("pty-size-{id}"))
        .spawn(move || {
            while let Ok(first) = pending.recv() {
                set_winsize(&sized, settle(&pending, first, RESIZE_SETTLE));
            }
        })
        .map(|_| ())
}

/// A drag resizes on every frame. A shell redraws its line on each SIGWINCH,
/// and one that has not caught up with the reflow redraws over the line above.
const RESIZE_SETTLE: Duration = Duration::from_millis(100);

/// The last size sent before `wait` passes with no other.
fn settle(pending: &mpsc::Receiver<Winsize>, mut size: Winsize, wait: Duration) -> Winsize {
    while let Ok(next) = pending.recv_timeout(wait) {
        size = next;
    }
    size
}

pub(crate) fn winsize_of(fd: libc::c_int) -> Winsize {
    let mut ws: libc::winsize = unsafe { std::mem::zeroed() };
    if unsafe { libc::ioctl(fd, libc::TIOCGWINSZ, &mut ws as *mut _) } == 0 {
        (ws.ws_col, ws.ws_row, ws.ws_xpixel, ws.ws_ypixel)
    } else {
        (0, 0, 0, 0)
    }
}

fn set_winsize(pty: &File, (cols, rows, width, height): Winsize) {
    let size = libc::winsize {
        ws_row: rows,
        ws_col: cols,
        ws_xpixel: width,
        ws_ypixel: height,
    };
    unsafe {
        libc::ioctl(pty.as_raw_fd(), libc::TIOCSWINSZ, &size);
    }
}

fn dup(fd: libc::c_int) -> io::Result<File> {
    let copy = unsafe { libc::dup(fd) };
    if copy < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(unsafe { File::from_raw_fd(copy) })
}

/// A GUI host inherits no TERM, or a stale one. Prefer Rio's terminfo when it
/// is installed, as librio's own PTY did.
fn env() -> Vec<(String, String)> {
    let term = if teletypewriter::terminfo_exists("xterm-rio") {
        "xterm-rio"
    } else if teletypewriter::terminfo_exists("rio") {
        "rio"
    } else {
        "xterm-256color"
    };
    vec![
        ("TERM".to_owned(), term.to_owned()),
        ("COLORTERM".to_owned(), "truecolor".to_owned()),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    fn within(secs: u64, mut done: impl FnMut() -> bool) -> bool {
        let deadline = Instant::now() + Duration::from_secs(secs);
        while Instant::now() < deadline {
            if done() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        false
    }

    #[test]
    fn an_adopted_link_answers_a_shell_line_through_the_adopted_master() {
        let shared = Shared::new();
        let engine = librio::Engine::new(shared.delegate());
        let desc = SurfaceDesc {
            shell: Some("/bin/sh".to_owned()),
            args: vec![
                "-c".to_owned(),
                r#"read line; printf 'got:%s' "$line""#.to_owned(),
            ],
            ..SurfaceDesc::default()
        };
        let surface = Arc::new(engine.create_surface(&desc).expect("create surface"));
        let mut link = PtyLink::spawn(Arc::clone(&surface), &desc, Arc::clone(&shared))
            .expect("spawn the shell");
        let (master, shell) = link.hand_over().expect("hand the master over");
        assert!(
            link.hand_over().is_none(),
            "a second hand-over finds nothing"
        );
        let winsize = winsize_of(master.as_raw_fd());
        let adopted = PtyLink::adopted(surface.clone(), shared, master, shell, winsize)
            .expect("the adopted link");
        assert!(adopted.is_adopted());
        adopted.write(b"hi\r");
        let mut render = librio::RenderState::new(&surface);
        assert!(within(5, || {
            render.update();
            (0..render.lines()).any(|line| render.text_row(line).contains("got:hi"))
        }));
    }
}
