//! A local shell on a PTY. librio is built without its own PTY, so the shell's
//! output is read here into `inject_output`, and `SurfaceDelegate::output` is
//! written back to it.

use std::fs::File;
use std::io::{self, Read, Write};
use std::os::fd::FromRawFd;
use std::sync::{mpsc, Arc};
use std::thread;

use librio::{Surface, SurfaceDesc};
use teletypewriter::{Pty, WinsizeBuilder};

use crate::delegate::Shared;

pub struct PtyLink {
    // Dropping the Pty hangs up and reaps the shell.
    pty: Pty,
    input: mpsc::Sender<Vec<u8>>,
    winsize: (u16, u16, u16, u16),
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
        let mut reader = dup(fd)?;
        let mut writer = dup(fd)?;
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
        Ok(Self {
            pty,
            input,
            winsize: (desc.cols, desc.rows, desc.pixel_width, desc.pixel_height),
        })
    }

    pub fn write(&self, bytes: &[u8]) {
        let _ = self.input.send(bytes.to_vec());
    }

    pub fn resize(&mut self, winsize: (u16, u16, u16, u16)) {
        if winsize == self.winsize {
            return;
        }
        self.winsize = winsize;
        let (cols, rows, width, height) = winsize;
        let _ = self.pty.child.set_winsize(WinsizeBuilder {
            rows,
            cols,
            width,
            height,
        });
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
