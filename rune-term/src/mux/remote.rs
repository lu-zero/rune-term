//! The sessions on ssh hosts and other servers, for the menu. Asking a host takes a round trip
//! the menu cannot wait for, so it shows what the host last said and asks again.

use std::collections::HashMap;
use std::process::Stdio;
use std::sync::Mutex;
use std::thread;

use super::Target;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Listed {
    Asking,
    Sessions(Vec<String>),
    /// ssh could not reach the host or log in, and what it said.
    Unreachable(String),
    /// The host has no such program.
    Missing,
}

/// The last answer of each host, and whether a question is out.
static LISTINGS: Mutex<Option<HashMap<Target, (Listed, bool)>>> = Mutex::new(None);

/// What the host last said, asking it again unless a question is already out.
pub(crate) fn listing(target: &Target) -> Listed {
    let mut listings = LISTINGS.lock().expect("listings");
    let entry = listings
        .get_or_insert_with(HashMap::new)
        .entry(target.clone())
        .or_insert((Listed::Asking, false));
    if !entry.1 {
        entry.1 = true;
        let target = target.clone();
        thread::spawn(move || {
            let listed = list(&target);
            let mut listings = LISTINGS.lock().expect("listings");
            if let Some(entry) = listings.as_mut().and_then(|map| map.get_mut(&target)) {
                *entry = (listed, false);
            }
        });
    }
    entry.0.clone()
}

/// ssh exits 255 for its own failures, and a remote shell 127 for a missing command.
pub(crate) fn list(target: &Target) -> Listed {
    let out = target
        .command(&["list-sessions", "-F", "#{session_name}"].map(String::from))
        .stdin(Stdio::null())
        .output();
    let out = match out {
        Ok(out) => out,
        Err(err) => return Listed::Unreachable(err.to_string()),
    };
    match out.status.code() {
        Some(255) => Listed::Unreachable(
            String::from_utf8_lossy(&out.stderr)
                .lines()
                .map(str::trim)
                .rfind(|line| !line.is_empty())
                .unwrap_or("ssh failed")
                .to_owned(),
        ),
        Some(127) => Listed::Missing,
        _ => Listed::Sessions(
            String::from_utf8_lossy(&out.stdout)
                .lines()
                .map(str::to_owned)
                .collect(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mux::Program;

    #[test]
    fn a_host_ssh_cannot_reach_is_unreachable() {
        let target = Target {
            program: Program::Tmux,
            host: Some("rune-term-test.invalid".to_owned()),
            socket: None,
        };
        let Listed::Unreachable(said) = list(&target) else {
            panic!("reached a host that does not exist");
        };
        assert!(said.contains("rune-term-test.invalid"), "{said}");
    }
}
