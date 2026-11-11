//! Local socket transport helpers for the control protocol.

use std::fs;
use std::io::{self, BufReader, BufWriter};
use std::path::{Path, PathBuf};
use std::time::Duration;

#[cfg(unix)]
use std::os::unix::net::{UnixListener, UnixStream};

use crate::codec::{read_message, write_message};
use crate::{Request, Response};

#[cfg(unix)]
pub fn connect(path: &Path) -> io::Result<UnixStream> {
    let stream = UnixStream::connect(path)?;
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));
    Ok(stream)
}

#[cfg(unix)]
pub fn bind(path: &Path) -> io::Result<UnixListener> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let _ = fs::remove_file(path);
    let listener = UnixListener::bind(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
    }
    Ok(listener)
}

#[cfg(unix)]
pub fn send_request(stream: &UnixStream, req: &Request) -> io::Result<()> {
    let mut writer = BufWriter::new(stream.try_clone()?);
    write_message(&mut writer, req)
}

#[cfg(unix)]
pub fn read_response(stream: &UnixStream) -> io::Result<Response> {
    let mut reader = BufReader::new(stream.try_clone()?);
    match read_message::<_, Response>(&mut reader)? {
        Some(resp) => Ok(resp),
        None => Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "connection closed",
        )),
    }
}

#[cfg(unix)]
pub fn round_trip(path: &Path, req: &Request) -> io::Result<Response> {
    let stream = connect(path)?;
    send_request(&stream, req)?;
    read_response(&stream)
}

/// Remove a socket file if it exists.
pub fn cleanup_socket(path: &Path) {
    let _ = fs::remove_file(path);
}

#[cfg(not(unix))]
pub fn round_trip(_path: &Path, _req: &Request) -> io::Result<Response> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "control IPC transport is not implemented on this platform",
    ))
}

#[cfg(not(unix))]
pub fn cleanup_socket(_path: &Path) {}

/// Resolve a socket path from an explicit value or default.
pub fn resolve_socket_path(explicit: Option<PathBuf>) -> Option<PathBuf> {
    explicit.or_else(crate::default_socket_path)
}
