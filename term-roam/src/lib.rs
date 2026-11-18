//! The link between rune-term and rune-term-agent: one byte stream over QUIC,
//! which follows the client to a new address. ssh starts the agent and carries
//! back a [`Ticket`]; the client trusts only the certificate and token it names.

mod client;
mod server;
mod tls;

use std::net::SocketAddr;

use serde::{Deserialize, Serialize};

pub use client::{connect, Client, Reader, Writer};
pub use server::{bind, Server, Spawn};
/// What a [`Spawn`] makes: the agent runs it on its own runtime.
pub use tokio::process::Command;

const ALPN: &[u8] = b"rune-term-agent/1";
const SERVER_NAME: &str = "rune-term-agent";
/// What the agent's line starts with, so the client skips whatever a login script prints first.
const MAGIC: &str = "rune-term-agent";

/// Where the agent listens, the certificate it answers with, and the token it wants.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ticket {
    pub addr: SocketAddr,
    pub fingerprint: [u8; 32],
    pub token: String,
}

impl Ticket {
    /// The line the agent prints for ssh to carry back.
    pub fn line(&self) -> String {
        format!(
            "{MAGIC} 1 {} {} {}",
            self.addr,
            hex(&self.fingerprint),
            self.token
        )
    }

    /// The ticket in the agent's output, if a line of it is one.
    pub fn find(output: &str) -> Option<Self> {
        output.lines().find_map(|line| {
            let mut words = line.split_whitespace();
            if words.next() != Some(MAGIC) || words.next() != Some("1") {
                return None;
            }
            let addr = words.next()?.parse().ok()?;
            let fingerprint = unhex(words.next()?)?;
            let token = words.next()?.to_owned();
            Some(Self {
                addr,
                fingerprint,
                token,
            })
        })
    }
}

/// The client's first message: who it is, and the multiplexer's arguments.
#[derive(Serialize, Deserialize)]
struct Hello {
    token: String,
    args: Vec<String>,
}

/// Compares every byte, so how long a comparison takes says nothing of where a guess went wrong.
fn same(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |diff, (a, b)| diff | (a ^ b)) == 0
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn unhex(text: &str) -> Option<[u8; 32]> {
    let mut out = [0u8; 32];
    if text.len() != 64 {
        return None;
    }
    for (i, byte) in out.iter_mut().enumerate() {
        *byte = u8::from_str_radix(text.get(2 * i..2 * i + 2)?, 16).ok()?;
    }
    Some(out)
}

#[cfg(test)]
mod tests;
