//! Lightweight NDJSON (newline-delimited JSON) framing for the control protocol.
//!
//! Each message is one JSON value encoded as UTF-8 followed by `\n`.
//! This keeps the transport simple, stream-friendly, and easy to debug while
//! reusing the serde-tagged protocol types.

use std::io::{self, BufRead, Write};

use serde::{de::DeserializeOwned, Serialize};

/// Write one NDJSON message.
pub fn write_message<W, T>(mut writer: W, value: &T) -> io::Result<()>
where
    W: Write,
    T: Serialize,
{
    serde_json::to_writer(&mut writer, value)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    writer.write_all(b"\n")?;
    writer.flush()
}

/// Read one NDJSON message. Returns `Ok(None)` on clean EOF before a message.
pub fn read_message<R, T>(reader: &mut R) -> io::Result<Option<T>>
where
    R: BufRead,
    T: DeserializeOwned,
{
    let mut line = String::new();
    let n = reader.read_line(&mut line)?;
    if n == 0 {
        return Ok(None);
    }
    if line.ends_with('\n') {
        line.pop();
        if line.ends_with('\r') {
            line.pop();
        }
    }
    let value =
        serde_json::from_str(&line).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    Ok(Some(value))
}
