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

    /// Arguments forwarded to Rune's tooling layer.
    ///
    /// Everything after this flag is passed through, including flags like
    /// `--help`. To list Rune's tooling flags: `rune-term --rune-args --help`.
    #[arg(long = "rune-args", value_name = "ARG", values = 1.., allow_hyphen_values)]
    rune_args: Vec<String>,
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
        if self.rune_help {
            // Rune prints help and exits.
            rune::tooling::Args::parse_from(vec!["--help".to_owned()]);
            return;
        }
        crate::app::run_with_rune_args(self.rune_args);
    }
}
