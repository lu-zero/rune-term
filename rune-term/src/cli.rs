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
    /// Print Rune's tooling help (snapshots, scripts, traces, etc.) and exit.
    #[arg(long = "rune-help")]
    rune_help: bool,

    /// Attach the tmux session SESSION, creating it if it does not exist. Its
    /// windows are a group of tabs.
    #[arg(long = "tmux", value_name = "SESSION")]
    tmux: Option<String>,

    /// Attach the rmux session SESSION, as `--tmux` does.
    #[arg(long = "rmux", value_name = "SESSION")]
    rmux: Option<String>,

    /// Open with the profile NAME. A profile whose backend is tmux or rmux
    /// attaches its session; any other is the first tab's.
    #[arg(long = "profile", value_name = "NAME")]
    profile: Option<String>,

    /// Run the `--tmux` or `--rmux` session on HOST, over ssh. HOST is what
    /// `ssh` takes: a name, `user@host`, or an alias from ~/.ssh/config.
    #[arg(long = "ssh", value_name = "HOST")]
    ssh: Option<String>,

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
        if let Some(Command::ShellIntegration(ShellIntegration { shell })) = self.command {
            print!(
                "{}",
                match shell {
                    Shell::Bash => include_str!("../../assets/shell/rune-term.bash"),
                    Shell::Zsh => include_str!("../../assets/shell/rune-term.zsh"),
                }
            );
            return;
        }
        if self.rune_help {
            // Rune prints help and exits.
            rune::tooling::Args::parse_from(vec!["--help".to_owned()]);
            return;
        }
        use crate::mux::{Program, Session, Target};
        let session = match (self.tmux, self.rmux) {
            (Some(_), Some(_)) => {
                eprintln!("rune-term: --tmux and --rmux attach one session; pass one");
                std::process::exit(2);
            }
            (Some(session), None) => Some((Program::Tmux, session)),
            (None, Some(session)) => Some((Program::Rmux, session)),
            (None, None) => None,
        };
        match (session, self.ssh, self.profile) {
            (Some(_), _, Some(_)) => {
                eprintln!("rune-term: --profile names what to open; drop --tmux or --rmux");
                std::process::exit(2);
            }
            (Some((program, session)), host, None) => {
                let target = Target {
                    program,
                    host,
                    socket: None,
                };
                crate::mux::queue(target, Session::Named(&session));
            }
            (None, Some(_), _) => {
                eprintln!("rune-term: --ssh runs a --tmux or --rmux session; pass one");
                std::process::exit(2);
            }
            (None, None, Some(name)) => crate::mux::queue_profile(name),
            (None, None, None) => {}
        }
        crate::app::run_with_rune_args(self.rune_args);
    }
}
