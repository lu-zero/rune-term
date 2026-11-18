//! Which multiplexer a group talks to, and over what: tmux or rmux, here or on
//! an ssh host. Both speak tmux's control mode, so only the command differs.

use std::process::Command;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Program {
    Tmux,
    Rmux,
}

impl Program {
    pub fn name(self) -> &'static str {
        match self {
            Program::Tmux => "tmux",
            Program::Rmux => "rmux",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "tmux" => Some(Program::Tmux),
            "rmux" => Some(Program::Rmux),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct Target {
    pub program: Program,
    /// An ssh destination, as `ssh` takes it: a host, `user@host`, or a config alias.
    pub host: Option<String>,
    /// A server other than the default one, tmux's `-L`.
    pub socket: Option<String>,
}

impl Target {
    pub fn local(program: Program) -> Self {
        Self {
            program,
            host: None,
            socket: None,
        }
    }

    /// The group's label before the multiplexer names the session.
    pub fn label(&self, session: &str) -> String {
        let label = match &self.host {
            Some(host) => format!("{session}@{host}"),
            None => session.to_owned(),
        };
        match &self.socket {
            Some(server) => format!("{label} ({server})"),
            None => label,
        }
    }

    /// The submenu of a target other than this machine's default server.
    pub fn place(&self) -> String {
        match (&self.host, &self.socket) {
            (Some(host), Some(server)) => format!("{host} ({server})"),
            (Some(host), None) => host.clone(),
            (None, Some(server)) => format!("Server {server}"),
            (None, None) => "This machine".to_owned(),
        }
    }

    /// The multiplexer with `args`. On a host, ssh joins its arguments into one
    /// line for the remote shell, so each is quoted. BatchMode keeps ssh from
    /// waiting on a password prompt nobody can see, and the timeout on a host that is down.
    pub fn command(&self, args: &[String]) -> Command {
        let mut line: Vec<&str> = vec![self.program.name()];
        if let Some(socket) = &self.socket {
            line.extend(["-L", socket]);
        }
        line.extend(args.iter().map(String::as_str));
        self.run(&line)
    }

    /// `line`, a program and its arguments, here or on the host.
    pub fn run(&self, line: &[&str]) -> Command {
        match &self.host {
            None => {
                let mut command = Command::new(line[0]);
                command.args(&line[1..]);
                command
            }
            Some(host) => {
                let mut command = Command::new("ssh");
                command
                    .args([
                        "-T",
                        "-o",
                        "BatchMode=yes",
                        "-o",
                        "ConnectTimeout=10",
                        "--",
                        host,
                    ])
                    .arg(
                        line.iter()
                            .map(|arg| quote(arg))
                            .collect::<Vec<_>>()
                            .join(" "),
                    );
                command
            }
        }
    }
}

/// One word for a POSIX shell, whatever it holds.
pub(super) fn quote(arg: &str) -> String {
    format!("'{}'", arg.replace('\'', r"'\''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(command: &Command) -> Vec<String> {
        command
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn a_remote_session_name_reaches_the_far_shell_as_one_word() {
        let target = Target {
            program: Program::Rmux,
            host: Some("box".to_owned()),
            socket: Some("work".to_owned()),
        };
        let name = "it's a $HOME `x`";
        let command = target.command(&[
            "-C".to_owned(),
            "new-session".to_owned(),
            "-s".to_owned(),
            name.to_owned(),
        ]);
        assert_eq!(command.get_program(), "ssh");
        let mut args = args(&command);
        let line = args.pop().unwrap();
        assert_eq!(
            args,
            [
                "-T",
                "-o",
                "BatchMode=yes",
                "-o",
                "ConnectTimeout=10",
                "--",
                "box"
            ]
        );
        let out = Command::new("/bin/sh")
            .args([
                "-c",
                &format!("printf '%s\\n' {}", line.replacen("'rmux'", "", 1)),
            ])
            .output()
            .unwrap();
        assert_eq!(
            String::from_utf8(out.stdout).unwrap(),
            format!("-L\nwork\n-C\nnew-session\n-s\n{name}\n")
        );
    }
}
