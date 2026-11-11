use std::process::ExitCode;

use term_control::{AppInfo, ProfileInfo, PROTOCOL_VERSION};
use winnow_args::{report, Args, Subcommand};

/// Control and inspect rune-term (profiles, tabs, runtime state).
#[derive(Args, Debug)]
#[arg(name = "rune-term-ctl", version)]
struct Cli {
    /// Output machine-readable JSON.
    #[arg(long, global)]
    json: bool,

    #[arg(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Print basic CLI/protocol information.
    Info,
    /// Inspect configured profiles.
    Profiles(ProfilesArgs),
    /// Inspect live tabs/panes (requires a running rune-term control endpoint).
    Tabs(TabsArgs),
}

#[derive(Args, Debug)]
struct ProfilesArgs {
    #[arg(subcommand)]
    command: ProfilesCommand,
}

#[derive(Subcommand, Debug)]
enum ProfilesCommand {
    /// List configured profiles.
    List,
    /// Show one profile by name.
    Show {
        /// Profile name.
        #[arg(positional, value_name = "NAME")]
        name: String,
    },
}

#[derive(Args, Debug)]
struct TabsArgs {
    #[arg(subcommand)]
    command: TabsCommand,
}

#[derive(Subcommand, Debug)]
enum TabsCommand {
    /// List tabs/panes from the running instance.
    List,
}

fn main() -> ExitCode {
    let cli = match Cli::parse_from(std::env::args_os().skip(1)) {
        Ok(cli) => cli,
        Err(e) => std::process::exit(report(&e, "rune-term-ctl")),
    };

    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("rune-term-ctl: {err}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<(), String> {
    match cli.command {
        Command::Info => cmd_info(cli.json),
        Command::Profiles(args) => match args.command {
            ProfilesCommand::List => cmd_profiles_list(cli.json),
            ProfilesCommand::Show { name } => cmd_profiles_show(cli.json, &name),
        },
        Command::Tabs(args) => match args.command {
            TabsCommand::List => cmd_tabs_list(cli.json),
        },
    }
}

fn cmd_info(json: bool) -> Result<(), String> {
    let socket = term_control::default_socket_path();
    if let Some(path) = &socket {
        if let Ok(resp) = term_control::transport::round_trip(path, &term_control::Request::Info) {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&resp).map_err(|e| e.to_string())?
                );
            } else {
                match resp {
                    term_control::Response::Info { app } => {
                        println!("app: {} {}", app.name, app.version);
                        if let Some(pid) = app.pid {
                            println!("pid: {pid}");
                        }
                        if let Some(sock) = app.socket {
                            println!("socket: {}", sock.display());
                        }
                    }
                    term_control::Response::Error { code, message } => {
                        println!("ipc error: {code}: {message}");
                    }
                    other => {
                        println!(
                            "{}",
                            serde_json::to_string_pretty(&other).map_err(|e| e.to_string())?
                        );
                    }
                }
            }
            return Ok(());
        }
    }

    let info = AppInfo {
        name: "rune-term-ctl".to_owned(),
        version: env!("CARGO_PKG_VERSION").to_owned(),
        pid: Some(std::process::id()),
        socket,
    };
    if json {
        println!(
            "{}",
            serde_json::json!({
                "protocol_version": PROTOCOL_VERSION,
                "client": info,
                "ipc": { "connected": false, "reason": "live GUI IPC server not available" }
            })
        );
    } else {
        println!("rune-term-ctl {}", info.version);
        println!("protocol: v{PROTOCOL_VERSION}");
        if let Some(socket) = info.socket {
            println!("socket: {}", socket.display());
        }
        println!("ipc: not connected (live GUI IPC server not available)");
    }
    Ok(())
}

fn cmd_profiles_list(json: bool) -> Result<(), String> {
    let cfg = rune_term::settings::ConfigFile::load();
    let default_profile = cfg.default_profile_name().to_owned();
    let mut profiles: Vec<ProfileInfo> = cfg
        .profile_names()
        .map(|name| profile_info(&cfg, name))
        .collect();
    profiles.sort_by(|a, b| a.name.cmp(&b.name));

    if json {
        let out = term_control::Response::Profiles {
            default_profile,
            profiles,
        };
        println!(
            "{}",
            serde_json::to_string_pretty(&out).map_err(|e| e.to_string())?
        );
    } else {
        for p in &profiles {
            let mark = if p.is_default { "*" } else { " " };
            println!("{mark} {}", p.name);
            println!(
                "    backend={} font={:?} theme={:?} cwd={:?}",
                p.backend, p.font_size, p.theme, p.cwd
            );
        }
    }
    Ok(())
}

fn cmd_profiles_show(json: bool, name: &str) -> Result<(), String> {
    let cfg = rune_term::settings::ConfigFile::load();
    let profile = if cfg.profile(name).is_some() {
        Some(profile_info(&cfg, name))
    } else {
        None
    };

    if json {
        let out = term_control::Response::Profile { profile };
        println!(
            "{}",
            serde_json::to_string_pretty(&out).map_err(|e| e.to_string())?
        );
        return Ok(());
    }

    let Some(p) = profile else {
        return Err(format!("profile '{name}' not found"));
    };

    println!("name: {}", p.name);
    println!("default: {}", p.is_default);
    println!("font_family: {}", p.font_family);
    println!("font_size: {:?}", p.font_size);
    println!("theme: {:?}", p.theme);
    println!("backend: {}", p.backend);
    println!("shell_program: {:?}", p.shell_program);
    println!("shell_args: {:?}", p.shell_args);
    println!("cwd: {:?}", p.cwd);
    Ok(())
}

fn cmd_tabs_list(json: bool) -> Result<(), String> {
    let socket = term_control::default_socket_path();
    if let Some(path) = &socket {
        match term_control::transport::round_trip(path, &term_control::Request::ListTabs) {
            Ok(resp) => {
                if json {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&resp).map_err(|e| e.to_string())?
                    );
                } else {
                    match resp {
                        term_control::Response::Tabs { windows } => {
                            if windows.is_empty() {
                                println!("no windows");
                            }
                            for (wi, w) in windows.iter().enumerate() {
                                println!("window[{wi}] id={} focused={}", w.id, w.focused);
                                for tab in &w.tabs {
                                    let active = if tab.active { "*" } else { " " };
                                    println!(
                                        "  {active} tab[{}] id={} title={:?} profile={} panes={}",
                                        tab.index,
                                        tab.id,
                                        tab.title,
                                        tab.profile,
                                        tab.panes.len()
                                    );
                                }
                            }
                        }
                        term_control::Response::Error { code, message } => {
                            eprintln!("ipc error: {code}: {message}");
                            return Err(message);
                        }
                        other => {
                            println!(
                                "{}",
                                serde_json::to_string_pretty(&other).map_err(|e| e.to_string())?
                            );
                        }
                    }
                }
                return Ok(());
            }
            Err(err) => {
                let msg = format!("cannot connect to control socket: {err}");
                if json {
                    let out = term_control::Response::Error {
                        code: "ipc_unavailable".to_owned(),
                        message: format!("{msg} (socket: {})", path.display()),
                    };
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&out).map_err(|e| e.to_string())?
                    );
                } else {
                    eprintln!("{msg}");
                    eprintln!("expected socket: {}", path.display());
                }
                return Err(msg);
            }
        }
    }
    Err("no control socket path available".to_owned())
}

fn profile_info(cfg: &rune_term::settings::ConfigFile, name: &str) -> ProfileInfo {
    let is_default = cfg.default_profile_name() == name;
    let profile = cfg.profile(name).cloned().unwrap_or_default();
    let resolved = cfg.resolve_profile(name);
    let (shell_program, shell_args) = match &profile.shell {
        Some(shell) => (shell.program.clone(), shell.args.clone()),
        None => (None, Vec::new()),
    };
    ProfileInfo {
        name: name.to_owned(),
        is_default,
        font_family: profile.font_family,
        font_size: profile.font_size.or(Some(resolved.font_size)),
        theme: profile.theme.or(Some(resolved.theme)),
        backend: resolved.backend,
        shell_program,
        shell_args,
        cwd: profile.cwd.or(resolved.cwd),
    }
}
