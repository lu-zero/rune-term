//! The shell keeper. Rune-term starts one at exit to own a shell's master
//! while no rune-term runs; the next instance connects to adopt it.
//!
//! Usage: `rune-term-keeper --socket PATH --shell PID [--profile NAME]`
//!
//! The master arrives on standard input as ancillary data
//! ([`rune_term_keeper::recv_master`]). The keeper binds the socket the next
//! instance scans, holds the master open, reads the output into a ring, and
//! on the connection serves the master and the ring back.

use std::fs::{self, File};
use std::io::{ErrorKind, Read, Write};
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::path::PathBuf;
use std::time::Duration;

use libc::{poll, pollfd, POLLERR, POLLHUP, POLLIN};
use rune_term_keeper::{recv_master, send_fd};

/// The most output the keeper holds for an adoption that comes back late.
const RING: usize = 8 * 1024;
/// The poll timeout while nobody is connected.
const POLL_MS: i32 = 500;
/// How long serving the master waits for the adoption's ack.
const ACK_TIMEOUT: Duration = Duration::from_secs(3);

fn main() {
    let socket = PathBuf::from(arg("--socket").expect("--socket PATH"));
    let shell = arg("--shell")
        .and_then(|value| value.parse().ok())
        .expect("--shell PID");
    let profile = arg("--profile");
    let master =
        unsafe { File::from_raw_fd(recv_master(0).expect("the master on standard input")) };
    if let Err(err) = serve(&socket, master, shell, profile) {
        eprintln!("rune-term-keeper: {err}");
    }
}

fn arg(name: &str) -> Option<String> {
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == name {
            return args.next();
        }
    }
    None
}

fn serve(
    socket: &Path,
    mut master: File,
    shell: u32,
    profile: Option<String>,
) -> std::io::Result<()> {
    if let Some(dir) = socket.parent() {
        fs::create_dir_all(dir)?;
    }
    let _ = fs::remove_file(socket);
    let listener = UnixListener::bind(socket)?;
    let mut ring: Vec<u8> = Vec::with_capacity(RING);
    let mut fds = [
        pollfd {
            fd: master.as_raw_fd(),
            events: POLLIN,
            revents: 0,
        },
        pollfd {
            fd: listener.as_raw_fd(),
            events: POLLIN,
            revents: 0,
        },
    ];
    let mut buf = [0u8; 8 * 1024];
    loop {
        if unsafe { poll(fds.as_mut_ptr(), fds.len() as _, POLL_MS) } < 0 {
            let err = std::io::Error::last_os_error();
            if err.kind() == ErrorKind::Interrupted {
                continue;
            }
            break;
        }
        if fds[0].revents & (POLLIN | POLLHUP | POLLERR) != 0 {
            match master.read(&mut buf) {
                // The shell is gone: nothing to hold.
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    ring.extend_from_slice(&buf[..n]);
                    if ring.len() > RING {
                        ring.drain(..ring.len() - RING);
                    }
                }
            }
        }
        if fds[1].revents & (POLLIN | POLLERR) != 0 {
            if let Ok((mut stream, _)) = listener.accept() {
                serve_connection(&mut stream, &master, &ring, shell, &profile);
                break;
            }
        }
    }
    let _ = fs::remove_file(socket);
    Ok(())
}

/// Serve the master to one adopter: the master, the ring, the shell and its
/// profile, then wait for the adoption's ack so the next writer is sure the
/// master moved.
fn serve_connection(
    stream: &mut UnixStream,
    master: &File,
    ring: &[u8],
    shell: u32,
    profile: &Option<String>,
) {
    if send_fd(stream.as_raw_fd(), master.as_raw_fd()).is_err() {
        return;
    }
    let _ = stream.write_all(&(ring.len() as u32).to_ne_bytes());
    let _ = stream.write_all(ring);
    let _ = stream.write_all(
        format!(
            "shell={shell}\nprofile={}\n",
            profile.clone().unwrap_or_default()
        )
        .as_bytes(),
    );
    let _ = stream.set_read_timeout(Some(ACK_TIMEOUT));
    let mut ack = [0u8; 1];
    let _ = stream.read_exact(&mut ack);
}
