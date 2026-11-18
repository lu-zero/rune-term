//! The link through rune-term-agent: ssh starts the agent once, and QUIC
//! carries control mode after that, wherever the client moves.

use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use term_roam::{Client, Ticket};

use super::Target;

/// How long a connection or an agent's start may take before it counts as lost.
const TIMEOUT: Duration = Duration::from_secs(10);

/// A non-interactive ssh shell often lacks the directories an install puts the agent in.
const START: &str = r#"PATH="$PATH:$HOME/.local/bin:$HOME/.cargo/bin" exec rune-term-agent "$@""#;

/// How long a link that started an agent nobody reached retries that agent's
/// ticket before it starts another.
const STRANDED: Duration = Duration::from_secs(5 * 60);

#[derive(Default)]
pub(crate) struct Roam {
    pub ticket: Option<Ticket>,
    /// When an agent started for this link did not answer. Another started
    /// soon after likely would not either, and each one left lingers.
    stranded: Option<Instant>,
    /// The newest link. A Reconnect while a retry waits out its backoff makes
    /// a newer one, and the older stops rather than attach a second client.
    link: u64,
}

pub(crate) type Shared = Arc<Mutex<Roam>>;

/// A new link, which every older one yields to.
pub(crate) fn supersede(roam: &Shared) -> u64 {
    let mut roam = roam.lock().expect("roam");
    roam.link += 1;
    roam.link
}

pub(crate) fn current(roam: &Shared, link: u64) -> bool {
    roam.lock().expect("roam").link == link
}

/// The agent's ticket, from an agent started on the target's host, or here.
fn start(target: &Target) -> Result<Ticket, String> {
    let mut line = vec![
        "sh",
        "-c",
        START,
        "sh",
        "serve",
        "--program",
        target.program.name(),
    ];
    if let Some(server) = &target.socket {
        line.extend(["--server", server]);
    }
    let out = target
        .run(&line)
        .stdin(Stdio::null())
        .output()
        .map_err(|err| err.to_string())?;
    let stdout = String::from_utf8_lossy(&out.stdout);
    Ticket::find(&stdout).ok_or_else(|| {
        let stderr = String::from_utf8_lossy(&out.stderr);
        stderr
            .lines()
            .map(str::trim)
            .rfind(|line| !line.is_empty())
            .map_or_else(
                || format!("rune-term-agent did not start: {}", out.status),
                str::to_owned,
            )
    })
}

/// A connection with the ticket it has, or failing that through a new agent.
pub(crate) fn connect(target: &Target, roam: &Shared, args: &[String]) -> Result<Client, String> {
    let (ticket, stranded) = {
        let roam = roam.lock().expect("roam");
        let stranded = roam
            .stranded
            .is_some_and(|since| since.elapsed() < STRANDED);
        (roam.ticket.clone(), stranded)
    };
    let mut why = None;
    if let Some(ticket) = &ticket {
        match term_roam::connect(ticket, args, TIMEOUT) {
            Ok(client) => {
                roam.lock().expect("roam").stranded = None;
                return Ok(client);
            }
            Err(err) if stranded => return Err(err.to_string()),
            Err(err) => why = Some(err.to_string()),
        }
    }
    let ticket =
        start(target).map_err(|err| why.map_or(err.clone(), |why| format!("{why}; {err}")))?;
    let connected = term_roam::connect(&ticket, args, TIMEOUT);
    let mut roam = roam.lock().expect("roam");
    roam.stranded = connected.is_err().then(Instant::now);
    roam.ticket = Some(ticket);
    connected.map_err(|err| err.to_string())
}
