//! A local shell on a PTY. librio is built without its own PTY, so the shell's
//! output is read here into `inject_output`, and `SurfaceDelegate::output` is
//! written back to it.

use std::fs::File;
use std::io::{self, Read, Write};
use std::os::fd::{AsRawFd, FromRawFd};
use std::sync::{mpsc, Arc};
use std::thread;
use std::time::Duration;

use librio::{Surface, SurfaceDesc};
use teletypewriter::Pty;

use crate::delegate::Shared;

pub struct PtyLink {
    // Dropping the Pty hangs up and reaps the shell.
    pty: Pty,
    input: mpsc::Sender<Vec<u8>>,
    sizes: mpsc::Sender<Winsize>,
    winsize: Winsize,
}

type Winsize = (u16, u16, u16, u16);

/// A drag resizes on every frame. A shell redraws its line on each SIGWINCH,
/// and one that has not caught up with the reflow redraws over the line above.
const RESIZE_SETTLE: Duration = Duration::from_millis(100);

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
        let mut reader = dup(fd)?;
        let mut writer = dup(fd)?;
        let sized = dup(fd)?;
        let id = surface.id();
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
            })?;
        let (input, queued) = mpsc::channel::<Vec<u8>>();
        thread::Builder::new()
            .name(format!("pty-write-{id}"))
            .spawn(move || {
                for bytes in queued {
                    if writer.write_all(&bytes).is_err() {
                        break;
                    }
                }
            })?;
        let (sizes, pending) = mpsc::channel::<Winsize>();
        thread::Builder::new()
            .name(format!("pty-size-{id}"))
            .spawn(move || {
                while let Ok(first) = pending.recv() {
                    set_winsize(&sized, settle(&pending, first, RESIZE_SETTLE));
                }
            })?;
        Ok(Self {
            pty,
            input,
            sizes,
            winsize: (desc.cols, desc.rows, desc.pixel_width, desc.pixel_height),
        })
    }

    /// The program in the foreground, unless it is the shell itself.
    pub fn running(&self) -> Option<String> {
        let (fd, shell) = (*self.pty.child.id, *self.pty.child.pid);
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

/// The last size sent before `wait` passes with no other.
fn settle(pending: &mpsc::Receiver<Winsize>, mut size: Winsize, wait: Duration) -> Winsize {
    while let Ok(next) = pending.recv_timeout(wait) {
        size = next;
    }
    size
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

    #[test]
    fn a_burst_of_sizes_settles_on_the_last() {
        let (sizes, pending) = mpsc::channel();
        for cols in [79, 78] {
            sizes.send((cols, 24, cols * 8, 384)).unwrap();
        }
        let wait = Duration::from_millis(20);
        assert_eq!(
            settle(&pending, (80, 24, 640, 384), wait),
            (78, 24, 624, 384)
        );
    }
}
