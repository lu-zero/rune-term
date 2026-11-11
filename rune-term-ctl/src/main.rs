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
    let info = AppInfo {
        name: "rune-term-ctl".to_owned(),
        version: env!("CARGO_PKG_VERSION").to_owned(),
        pid: Some(std::process::id()),
        socket: term_control::default_socket_path(),
    };
    if json {
        println!(
            "{}",
            serde_json::json!({
                "protocol_version": PROTOCOL_VERSION,
                "client": info,
                "ipc": { "connected": false, "reason": "live GUI IPC server not implemented yet" }
            })
        );
    } else {
        println!("rune-term-ctl {}", info.version);
        println!("protocol: v{PROTOCOL_VERSION}");
        if let Some(socket) = info.socket {
            println!("socket: {}", socket.display());
        }
        println!("ipc: not connected (live GUI IPC server not implemented yet)");
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
    let msg = "live GUI IPC server not implemented yet; cannot list running tabs/panes";
    if json {
        let out = term_control::Response::Error {
            code: "ipc_unavailable".to_owned(),
            message: format!("{msg} (socket: {:?})", socket),
        };
        println!(
            "{}",
            serde_json::to_string_pretty(&out).map_err(|e| e.to_string())?
        );
    } else {
        eprintln!("{msg}");
        if let Some(socket) = socket {
            eprintln!("expected socket: {}", socket.display());
        }
    }
    Err(msg.to_owned())
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
