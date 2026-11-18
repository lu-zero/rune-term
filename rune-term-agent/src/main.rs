//! Started over ssh by rune-term, as mosh-server is by mosh: it prints a ticket
//! and leaves ssh, then serves the multiplexer's control mode over QUIC.

use std::io::{BufRead, BufReader, Write};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::os::unix::process::CommandExt;
use std::process::{Command, ExitCode, Stdio};
use std::sync::Arc;
use std::time::Duration;

use winnow_args::{report, Args, Subcommand};

/// Hold a tmux or rmux control client for rune-term, over a link that roams.
#[derive(Args, Debug)]
#[arg(name = "rune-term-agent", version)]
struct Cli {
    #[arg(subcommand)]
    command: Mode,
}

#[derive(Subcommand, Debug)]
enum Mode {
    /// Start an agent apart from this shell, print its ticket, and return.
    Serve(Options),
    /// Be the agent, printing its ticket on standard output.
    Run(Options),
}

#[derive(Args, Debug)]
struct Options {
    /// The multiplexer: `tmux` or `rmux`.
    #[arg(long = "program", value_name = "PROGRAM")]
    program: String,

    /// Its server other than the default, as its `-L` takes.
    #[arg(long = "server", value_name = "NAME")]
    server: Option<String>,

    /// Seconds without a client after which the agent exits. The session stays.
    #[arg(long = "linger", value_name = "SECONDS")]
    linger: Option<u64>,
}

fn main() -> ExitCode {
    let cli = match Cli::parse_from(std::env::args_os().skip(1)) {
        Ok(cli) => cli,
        Err(e) => std::process::exit(report(&e, "rune-term-agent")),
    };
    let result = match cli.command {
        Mode::Serve(options) => serve(&options),
        Mode::Run(options) => run(options),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("rune-term-agent: {err}");
            ExitCode::FAILURE
        }
    }
}

/// ssh returns once nothing holds its channel, so the agent runs as a child
/// in a group of its own, with only a pipe back to this process for the ticket.
fn serve(options: &Options) -> Result<(), String> {
    known(&options.program)?;
    let mut command = Command::new(std::env::current_exe().map_err(|e| e.to_string())?);
    command.args(["run", "--program", &options.program]);
    if let Some(server) = &options.server {
        command.args(["--server", server]);
    }
    if let Some(linger) = options.linger {
        command.args(["--linger", &linger.to_string()]);
    }
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
        .map_err(|e| e.to_string())?;
    let mut line = String::new();
    let read = BufReader::new(child.stdout.take().expect("piped stdout")).read_line(&mut line);
    match read {
        Ok(n) if n > 0 => {
            print!("{line}");
            std::io::stdout().flush().map_err(|e| e.to_string())
        }
        _ => {
            let status = child.wait().map_err(|e| e.to_string())?;
            Err(format!("the agent stopped before it listened: {status}"))
        }
    }
}

/// The address ssh reached this host on, which the client can reach too.
fn reachable() -> IpAddr {
    std::env::var("SSH_CONNECTION")
        .ok()
        .and_then(|value| value.split_whitespace().nth(2)?.parse().ok())
        .unwrap_or(IpAddr::V4(Ipv4Addr::LOCALHOST))
}

fn known(program: &str) -> Result<(), String> {
    match program {
        "tmux" | "rmux" => Ok(()),
        other => Err(format!("'{other}' is not tmux or rmux")),
    }
}

fn run(options: Options) -> Result<(), String> {
    known(&options.program)?;
    let advertised = reachable();
    let any = match advertised {
        IpAddr::V4(_) => IpAddr::V4(Ipv4Addr::UNSPECIFIED),
        IpAddr::V6(_) => IpAddr::V6(Ipv6Addr::UNSPECIFIED),
    };
    let (server, ticket) =
        term_roam::bind(SocketAddr::new(any, 0), advertised).map_err(|e| e.to_string())?;
    println!("{}", ticket.line());
    std::io::stdout().flush().map_err(|e| e.to_string())?;
    let Options {
        program,
        server: socket,
        linger,
    } = options;
    let spawn: Arc<term_roam::Spawn> = Arc::new(move |args: &[String]| {
        let mut command = tokio::process::Command::new(&program);
        if let Some(socket) = &socket {
            command.args(["-L", socket]);
        }
        command.args(args);
        command
    });
    let linger = Duration::from_secs(linger.unwrap_or(24 * 60 * 60));
    server.run(spawn, linger).map_err(|e| e.to_string())
}
