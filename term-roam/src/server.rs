//! The agent's end: each connection that shows the token runs the multiplexer
//! with the arguments it asked for, joined to the connection's stream.

use std::io;
use std::net::{IpAddr, SocketAddr};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use quinn::crypto::rustls::QuicServerConfig;
use tokio::io::AsyncReadExt;
use tokio::process::Command;

use crate::{hex, same, tls, Hello, Ticket};

/// The longest hello the agent reads, well past any argument list it is sent.
const HELLO_CAP: u32 = 64 * 1024;
/// How long a connection may take to show its token. One that never does
/// would otherwise keep the agent from ever counting as idle.
const HELLO_WAIT: Duration = Duration::from_secs(10);
/// How long an agent nobody has reached waits: a client that could not reach
/// it once is not likely to later, and the next start makes another.
const UNREACHED_LINGER: Duration = Duration::from_secs(2 * 60);

/// What a connection runs: the multiplexer, given the client's arguments.
pub type Spawn = dyn Fn(&[String]) -> Command + Send + Sync;

pub struct Server {
    runtime: tokio::runtime::Runtime,
    endpoint: quinn::Endpoint,
    token: String,
}

/// A server on `addr`, and the ticket that reaches it at `advertised`.
pub fn bind(addr: SocketAddr, advertised: IpAddr) -> io::Result<(Server, Ticket)> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let (crypto, fingerprint) = tls::server()?;
    let crypto = QuicServerConfig::try_from(crypto).map_err(io::Error::other)?;
    let mut config = quinn::ServerConfig::with_crypto(Arc::new(crypto));
    config.transport_config(crate::client::transport());
    let endpoint = {
        let _runtime = runtime.enter();
        quinn::Endpoint::server(config, addr)?
    };
    let mut secret = [0u8; 32];
    ring::rand::SecureRandom::fill(&ring::rand::SystemRandom::new(), &mut secret)
        .map_err(|_| io::Error::other("no randomness for a token"))?;
    let token = hex(&secret);
    let ticket = Ticket {
        addr: SocketAddr::new(advertised, endpoint.local_addr()?.port()),
        fingerprint,
        token: token.clone(),
    };
    Ok((
        Server {
            runtime,
            endpoint,
            token,
        },
        ticket,
    ))
}

impl Server {
    /// Serves until no connection has been open for `linger`.
    pub fn run(self, spawn: Arc<Spawn>, linger: Duration) -> io::Result<()> {
        let Server {
            runtime,
            endpoint,
            token,
        } = self;
        let token = Arc::new(token);
        let open = Arc::new(AtomicUsize::new(0));
        let reached = Arc::new(AtomicBool::new(false));
        let idle_since = Arc::new(Mutex::new(Instant::now()));
        runtime.block_on(async move {
            loop {
                let linger = match reached.load(Ordering::SeqCst) {
                    true => linger,
                    false => linger.min(UNREACHED_LINGER),
                };
                let idle = open.load(Ordering::SeqCst) == 0
                    && idle_since.lock().expect("idle").elapsed() >= linger;
                if idle {
                    break;
                }
                let incoming = tokio::select! {
                    incoming = endpoint.accept() => incoming,
                    () = tokio::time::sleep(Duration::from_secs(1)) => continue,
                };
                let Some(incoming) = incoming else {
                    break;
                };
                let (token, spawn, open, idle_since, reached) = (
                    Arc::clone(&token),
                    Arc::clone(&spawn),
                    Arc::clone(&open),
                    Arc::clone(&idle_since),
                    Arc::clone(&reached),
                );
                open.fetch_add(1, Ordering::SeqCst);
                tokio::spawn(async move {
                    let _ = serve(incoming, &token, spawn.as_ref(), &reached).await;
                    *idle_since.lock().expect("idle") = Instant::now();
                    open.fetch_sub(1, Ordering::SeqCst);
                });
            }
            endpoint.close(0u32.into(), b"idle");
            endpoint.wait_idle().await;
        });
        Ok(())
    }
}

async fn serve(
    incoming: quinn::Incoming,
    token: &str,
    spawn: &Spawn,
    reached: &AtomicBool,
) -> io::Result<()> {
    let connection = incoming.await?;
    let hello = tokio::time::timeout(HELLO_WAIT, hello(&connection)).await;
    let Ok(Ok(Some((mut send, recv, hello)))) = hello else {
        connection.close(1u32.into(), b"no hello");
        return Ok(());
    };
    if !same(hello.token.as_bytes(), token.as_bytes()) {
        send.write_all(b"refused\n").await?;
        let _ = send.finish();
        let _ = tokio::time::timeout(Duration::from_secs(2), connection.closed()).await;
        return Ok(());
    }
    reached.store(true, Ordering::SeqCst);
    let child = spawn(&hello.args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn();
    let mut child = match child {
        Ok(child) => child,
        Err(err) => {
            send.write_all(format!("error: {err}\n").as_bytes()).await?;
            let _ = send.finish();
            let _ = tokio::time::timeout(Duration::from_secs(2), connection.closed()).await;
            return Ok(());
        }
    };
    send.write_all(b"ok\n").await?;
    let mut recv = recv;
    let mut stdin = child.stdin.take().expect("piped stdin");
    let mut stdout = child.stdout.take().expect("piped stdout");
    // The multiplexer exits once its input ends, which ends its output too.
    tokio::spawn(async move {
        let _ = tokio::io::copy(&mut recv, &mut stdin).await;
    });
    tokio::select! {
        _ = tokio::io::copy(&mut stdout, &mut send) => {
            let _ = send.finish();
            let _ = tokio::time::timeout(Duration::from_secs(5), connection.closed()).await;
        }
        _ = connection.closed() => {}
    }
    let _ = child.kill().await;
    Ok(())
}

/// The client's first stream and its hello. None for one too long to read.
async fn hello(
    connection: &quinn::Connection,
) -> io::Result<Option<(quinn::SendStream, quinn::RecvStream, Hello)>> {
    let (send, mut recv) = connection.accept_bi().await?;
    let len = recv.read_u32().await?;
    if len > HELLO_CAP {
        return Ok(None);
    }
    let mut hello = vec![0u8; len as usize];
    AsyncReadExt::read_exact(&mut recv, &mut hello).await?;
    Ok(Some((send, recv, serde_json::from_slice(&hello)?)))
}
