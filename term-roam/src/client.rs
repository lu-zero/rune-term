//! The client's end, blocking: a [`Reader`] and a [`Writer`] for the threads
//! that already speak control mode over a child's pipes.

use std::io::{self, Read, Write};
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::{mpsc, Arc};
use std::thread;
use std::time::Duration;

use quinn::crypto::rustls::QuicClientConfig;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt};
use tokio::sync::mpsc as async_mpsc;

use crate::{tls, Hello, Ticket, SERVER_NAME};

/// A keepalive well inside a NAT's mapping timeout, and an idle timeout that
/// declares the link lost after a minute without an answer.
pub(crate) fn transport() -> Arc<quinn::TransportConfig> {
    let mut transport = quinn::TransportConfig::default();
    transport.keep_alive_interval(Some(Duration::from_secs(5)));
    transport.max_idle_timeout(Some(
        quinn::IdleTimeout::try_from(Duration::from_secs(60)).expect("a minute fits"),
    ));
    Arc::new(transport)
}

pub struct Client {
    pub reader: Reader,
    pub writer: Writer,
    endpoint: quinn::Endpoint,
    runtime: tokio::runtime::Handle,
}

impl Client {
    /// Moves the client to a new local socket, as a change of network does.
    pub fn rebind(&self, socket: std::net::UdpSocket) -> io::Result<()> {
        let _runtime = self.runtime.enter();
        self.endpoint.rebind(socket)
    }
}

/// Connects, shows the token, and has the agent run its multiplexer with `args`.
/// Returns once the agent has started it, or within `timeout` with why not.
pub fn connect(ticket: &Ticket, args: &[String], timeout: Duration) -> io::Result<Client> {
    let (ready, started) = mpsc::sync_channel(1);
    let (bytes_out, reader_rx) = mpsc::channel::<io::Result<Vec<u8>>>();
    let (writer_tx, mut bytes_in) = async_mpsc::unbounded_channel::<Vec<u8>>();
    let ticket = ticket.clone();
    let hello = serde_json::to_vec(&Hello {
        token: ticket.token.clone(),
        args: args.to_vec(),
    })?;
    thread::Builder::new()
        .name("roam".to_owned())
        .spawn(move || {
            let runtime = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(runtime) => runtime,
                Err(err) => {
                    let _ = ready.send(Err(err));
                    return;
                }
            };
            let handle = runtime.handle().clone();
            runtime.block_on(async move {
                let opened = tokio::time::timeout(timeout, open(&ticket, &hello)).await;
                let (endpoint, connection, mut send, recv) = match opened {
                    Ok(Ok(opened)) => opened,
                    Ok(Err(err)) => {
                        let _ = ready.send(Err(err));
                        return;
                    }
                    Err(_) => {
                        let _ = ready.send(Err(io::Error::new(
                            io::ErrorKind::TimedOut,
                            format!("no answer from the agent at {}", ticket.addr),
                        )));
                        return;
                    }
                };
                let _ = ready.send(Ok((endpoint.clone(), handle)));
                let up = tokio::spawn(async move {
                    while let Some(bytes) = bytes_in.recv().await {
                        if send.write_all(&bytes).await.is_err() {
                            return;
                        }
                    }
                    let _ = send.finish();
                });
                let mut recv = recv;
                let mut buf = vec![0u8; 64 * 1024];
                loop {
                    match recv.read(&mut buf).await {
                        Ok(0) => break,
                        Ok(n) => {
                            if bytes_out.send(Ok(buf[..n].to_vec())).is_err() {
                                break;
                            }
                        }
                        Err(err) => {
                            let _ = bytes_out.send(Err(io::Error::new(
                                io::ErrorKind::ConnectionAborted,
                                err.to_string(),
                            )));
                            break;
                        }
                    }
                }
                up.abort();
                connection.close(0u32.into(), b"done");
                endpoint.wait_idle().await;
            });
        })?;
    let (endpoint, runtime) = started
        .recv()
        .map_err(|_| io::Error::other("the link thread stopped"))??;
    Ok(Client {
        reader: Reader {
            rx: reader_rx,
            pending: Vec::new(),
            at: 0,
        },
        writer: Writer { tx: writer_tx },
        endpoint,
        runtime,
    })
}

type Opened = (
    quinn::Endpoint,
    quinn::Connection,
    quinn::SendStream,
    tokio::io::BufReader<quinn::RecvStream>,
);

async fn open(ticket: &Ticket, hello: &[u8]) -> io::Result<Opened> {
    let local: SocketAddr = match ticket.addr {
        SocketAddr::V4(_) => (Ipv4Addr::UNSPECIFIED, 0).into(),
        SocketAddr::V6(_) => (Ipv6Addr::UNSPECIFIED, 0).into(),
    };
    let crypto =
        QuicClientConfig::try_from(tls::client(ticket.fingerprint)?).map_err(io::Error::other)?;
    let mut config = quinn::ClientConfig::new(Arc::new(crypto));
    config.transport_config(transport());
    let endpoint = quinn::Endpoint::client(local)?;
    let connection = endpoint
        .connect_with(config, ticket.addr, SERVER_NAME)
        .map_err(io::Error::other)?
        .await?;
    let (mut send, recv) = connection.open_bi().await?;
    send.write_u32(hello.len() as u32).await?;
    send.write_all(hello).await?;
    let mut recv = tokio::io::BufReader::new(recv);
    let mut status = String::new();
    recv.read_line(&mut status).await?;
    match status.trim_end() {
        "ok" => Ok((endpoint, connection, send, recv)),
        "refused" => Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "the agent refused the token",
        )),
        "" => Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "the agent closed the link",
        )),
        other => Err(io::Error::other(
            other.strip_prefix("error: ").unwrap_or(other).to_owned(),
        )),
    }
}

/// What the agent's multiplexer wrote. A lost link reads as an error saying why.
pub struct Reader {
    rx: mpsc::Receiver<io::Result<Vec<u8>>>,
    pending: Vec<u8>,
    at: usize,
}

impl Read for Reader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.at == self.pending.len() {
            match self.rx.recv() {
                Ok(Ok(bytes)) => {
                    self.pending = bytes;
                    self.at = 0;
                }
                Ok(Err(err)) => return Err(err),
                Err(_) => return Ok(0),
            }
        }
        let n = buf.len().min(self.pending.len() - self.at);
        buf[..n].copy_from_slice(&self.pending[self.at..self.at + n]);
        self.at += n;
        Ok(n)
    }
}

/// Input for the multiplexer. Dropping it ends that input.
pub struct Writer {
    tx: async_mpsc::UnboundedSender<Vec<u8>>,
}

impl Write for Writer {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.tx
            .send(buf.to_vec())
            .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "the link is closed"))?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
