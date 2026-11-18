//! Command-line interface. Rune's own tooling flags are handled by Rune when
//! forwarded; this CLI owns argv parsing so Rune does not intercept it.

use winnow_args::{report, Args, Subcommand};

/// A terminal emulator built on Rune and librio.
#[derive(Args, Debug)]
#[arg(multicall, name = "rune-term")]
pub struct RuneTerm {
    #[arg(subcommand)]
    applet: Applet,
}

#[derive(Subcommand, Debug)]
enum Applet {
    /// Run the terminal emulator.
    #[arg(name = "rune-term")]
    Gui(Gui),
}

#[derive(Args, Debug, Default)]
struct Gui {
    /// Open with the profile NAME. A profile whose backend is tmux or rmux
    /// attaches its session; any other is the first tab's.
    #[arg(long = "profile", value_name = "NAME")]
    profile: Option<String>,

    /// Arguments forwarded to Rune's tooling layer.
    ///
    /// Everything after this flag is passed through, including flags like
    /// `--help`. To list Rune's tooling flags: `rune-term --rune-args --help`.
    #[arg(long = "rune-args", value_name = "ARG", values = 1.., allow_hyphen_values)]
    rune_args: Vec<String>,

    #[arg(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Print the prompt marks script for a shell, to evaluate from its rc file.
    ShellIntegration(ShellIntegration),
    /// Attach the tmux session SESSION, creating it if it does not exist. Its
    /// windows are a group of tabs.
    Attach(Attach),
    /// Print the tmux and rmux sessions here, or on HOST over ssh, one
    /// `PROGRAM<TAB>SESSION` a line.
    Sessions(Sessions),
    /// Print the profiles `--profile` takes, one `NAME<TAB>BACKEND` a line.
    Profiles,
}

#[derive(Args, Debug, PartialEq)]
struct Attach {
    /// Attach an rmux session instead.
    #[arg(long = "rmux")]
    rmux: bool,

    /// Run the session on HOST, over ssh. HOST is what `ssh` takes: a name,
    /// `user@host`, or an alias from ~/.ssh/config.
    #[arg(long = "host", value_name = "HOST")]
    host: Option<String>,

    /// Attach a session of the server NAME, tmux's `-L`, rather than the default server.
    #[arg(long = "server", value_name = "NAME")]
    server: Option<String>,

    #[arg(positional, value_name = "SESSION")]
    session: String,
}

#[derive(Args, Debug)]
struct Sessions {
    /// What `ssh` takes, as `attach --host` does.
    #[arg(positional, value_name = "HOST")]
    host: Option<String>,

    /// The server NAME's sessions, as `attach --server` takes.
    #[arg(long = "server", value_name = "NAME")]
    server: Option<String>,
}

#[derive(Args, Debug)]
struct ShellIntegration {
    #[arg(subcommand)]
    shell: Shell,
}

#[derive(Subcommand, Debug)]
enum Shell {
    /// `eval "$(rune-term shell-integration bash)"` in ~/.bashrc, after starship.
    Bash,
    /// `eval "$(rune-term shell-integration zsh)"` in ~/.zshrc.
    Zsh,
}

impl RuneTerm {
    /// Parse argv and run.
    pub fn run() {
        let cli = match <Self as Args>::parse_from_argv(std::env::args_os()) {
            Ok(cli) => cli,
            Err(e) => std::process::exit(report(&e, "rune-term")),
        };
        match cli.applet {
            Applet::Gui(gui) => gui.run(),
        }
    }
}

impl Gui {
    fn run(self) {
        match &self.command {
            Some(Command::ShellIntegration(ShellIntegration { shell })) => {
                print!(
                    "{}",
                    match shell {
                        Shell::Bash => include_str!("../../assets/shell/rune-term.bash"),
                        Shell::Zsh => include_str!("../../assets/shell/rune-term.zsh"),
                    }
                );
                return;
            }
            Some(Command::Sessions(Sessions { host, server })) => {
                std::process::exit(crate::mux::print_sessions(host.clone(), server.clone()));
            }
            Some(Command::Profiles) => {
                let config = crate::settings::ConfigFile::load();
                let mut names: Vec<_> = config.profile_names().collect();
                names.sort_unstable();
                for name in names {
                    println!("{name}\t{}", config.resolve_profile(name).backend);
                }
                return;
            }
            Some(Command::Attach(_)) | None => {}
        }
        use crate::mux::{Program, Session, Target};
        match (self.command, self.profile) {
            (Some(Command::Attach(_)), Some(_)) => {
                eprintln!("rune-term: --profile names what to open; drop it or attach");
                std::process::exit(2);
            }
            (
                Some(Command::Attach(Attach {
                    rmux,
                    host,
                    server,
                    session,
                })),
                None,
            ) => {
                let program = if rmux { Program::Rmux } else { Program::Tmux };
                let target = Target {
                    program,
                    host,
                    socket: server,
                };
                crate::mux::queue(target, Session::Named(&session));
            }
            (_, Some(name)) => crate::mux::queue_profile(name),
            (_, None) => {}
        }
        crate::app::run_with_rune_args(self.rune_args);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Gui {
        let cli =
            <RuneTerm as Args>::parse_from_argv(["rune-term"].iter().chain(args)).expect("parses");
        let Applet::Gui(gui) = cli.applet;
        gui
    }

    #[test]
    fn attach_takes_tmux_here_unless_told_rmux_a_host_or_a_server() {
        let Some(Command::Attach(attach)) = parse(&["attach", "work"]).command else {
            panic!("not an attach");
        };
        assert_eq!(
            attach,
            Attach {
                rmux: false,
                host: None,
                server: None,
                session: "work".to_owned()
            }
        );
        let Some(Command::Attach(attach)) = parse(&[
            "attach", "--rmux", "--host", "thalia", "--server", "w", "work",
        ])
        .command
        else {
            panic!("not an attach");
        };
        assert_eq!(
            attach,
            Attach {
                rmux: true,
                host: Some("thalia".to_owned()),
                server: Some("w".to_owned()),
                session: "work".to_owned()
            }
        );
    }
}
