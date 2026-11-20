# PTY handover (rune-term)

## The problem

The user: "iterm2 let you update itself and not lose the open terminals." In
iTerm2, a shell's PTY survives quitting iTerm2 because the shells are owned by
a separate daemon, and the next iTerm2 instance reattaches to the still-running
processes. rune-term today: dropping a `PtyLink` hangs up and reaps the shell,
so closing the app ends every shell.

## What iTerm2 does (findings)

- iTerm2's shells survive the app because they are owned by a separate daemon
  ("multiserver"), not by iTerm2 itself.
  `docs/session-restoration-and-process-reattachment.md` (iTerm2 repo):
  "When iTerm2 quits and relaunches, the shells and programs it was running can
  survive because they are owned by a separate daemon (the 'multiserver')...
  The session is then married to the job manager the daemon handed back."
- The iTerm2 repo has no symbol or doc that says "handover"; the closest names
  are "session restoration and process reattachment" and `ShellLauncher`.
  So: shells survive the app; the new instance reattaches them. That is the
  behavior to enable, scaled down: local shells survive rune-term exiting
  (quit, or replaced by an update), and the new rune-term instance reattaches
  them.
- iTerm2 restores arrangements (tabs, splits). rune-term does not plan to: the
  first adoption becomes the default tab; further adoptions are new tabs.

## What is here already

- `rune-term/src/link.rs`: `PtyLink::spawn` creates the shell through
  `teletypewriter::create_pty_with_spawn`; `*pty.child.id` is the master fd,
  `*pty.child.pid` the shell pid. Dropping the link drops the `Pty`: hangup,
  reaps the shell.
- `rune-term/src/delegate.rs`: `Shared.links: Mutex<HashMap<SurfaceId,
  PtyLink>>` — local shells only; mux panes are created with `open_bare`
  (`rune-term/src/mux.rs`), so `handover_all` sees only local shells.
- Rune exit: `rune::quit::fast_exit(last)` runs `last` in `end_fast` on the
  fast-exit path (the default). `rune::fast_exit` is exported
  (`../rune/crates/rune/src/lib.rs` `pub use quit::fast_exit`).
- etcetera `Xdg` strategy: `state_dir()`/`runtime_dir()`; settings uses
  `Xdg::new()` for the config dir on every platform
  (`rune-term/src/settings.rs` `config_dir`).

## The design

A keeper process owns the shell's master while no rune-term runs; a new
instance adopts it. iTerm2's daemon is the model, at one shell at a time.

Opt-in: by default closing the app kills the shells, as before.
`rune-term --handover` turns the whole thing on for a launch; while a
running instance is up, `rune-term-ctl handover` flips its exit handover
on — the next launch adopts the keepers (the flip writes the marker).

### The keeper: `rune-term-keeper` (new member, bin + lib)

- `rune-term-keeper/src/lib.rs` — the wire protocol:
  `send_fd(sender, fd)` sends an fd with `sendmsg` + `SCM_RIGHTS`;
  `recv_master(sender)` reads the first `SCM_RIGHTS` cmsg with
  `recvmsg`. Both take raw fds, so the client (rune-term) and the keeper use
  the same code.
- `rune-term-keeper/src/main.rs` — the keeper process:
  - Usage: `rune-term-keeper --socket PATH --shell PID [--profile NAME]`.
  - The master arrives on standard input: `recv_master(0)`.
  - `serve`: `create_dir_all(parent)`, `remove_file(socket)`,
    `UnixListener::bind(socket)`, then poll loop over the master and the
    listener, `POLL_MS = 500`:
    - master readable/HUP/ERR: `master.read(&mut buf)`:
      `Ok(0)` or error → break (the shell is gone: unlink, exit).
      `Ok(n)` → append to the ring, keep the last `RING = 8 * 1024` bytes.
    - listener readable: `accept()`, `serve_connection`, then break (exit).
  - `serve_connection`: `send_fd(stream, master)` — the master to the adopter;
    then `write_all(&(ring.len() as u32).to_ne_bytes())` + the ring (the gap
    output); then the line `shell={shell}\nprofile={profile or ""}\n`; then
    `read_exact(&mut [0u8; 1])` for the adoption's ack. On exit: unlink socket.
  - Why the ack: a second adopter that connected while the keeper waits would
    otherwise get the same fd; the keeper exits after the ack so the next
    connect is refused and the stale socket unlinked.

### Handover at exit (rune-term)

- `rune::fast_exit(delegate::handover_all)` in `app::app()`, before
  `App::new`. Runs in `end_fast` when the app quits (window close or
  macOS Quit), the default exit path.
- `delegate::handover_all`:
  - `PUBLISHED` → the `Shared`.
  - `links = shared.links.lock().drain().collect::<Vec<_>>()`.
  - For each `(id, link)`: `link.hand_over() -> Option<(File, u32)>` — the
    master dup + the shell pid. Then
    `handover::hand_over_fd(master, shell, shared.profile_of(id), &keepers_dir())`.
- `handover::hand_over_fd(master, shell, profile, dir)`:
  - `create_dir_all(dir)`, `remove_file(dir/{shell}.sock)` (a stale keeper).
  - `socketpair()` → `(a, b)`: `send_fd(a, master)` — the master goes to the
    keeper; `drop(a)`.
  - Spawn the keeper: `Command::new(keeper_exe())`,
    `--socket dir/{shell}.sock --shell {pid} [--profile {name}]`,
    `stdin(Stdio::from(b))`, stdout/stderr null.
- `keeper_exe()`: `CARGO_BIN_EXE_rune-term-keeper` first (tests), then the
  sibling of the running `rune-term`'s exe, then its parent dir
  (`cargo test` puts the binary in `target/debug`), then `rune-term-keeper`
  on PATH.
- Dropping the `PtyLink`s afterwards: `Core::Spawned(None)` → nothing: the
  keeper holds the master, so no hangup.

### Adoption at startup

- `with_settings_window`'s `app.init` closure calls
  `handover::adopt(&mut model)` after `model::init()`.
- `handover::adopt(model)`:
  - First: `delegate::open_adopted(model, PANE_MARK, ad)` +
    `chrome_view::attach_initial(model, id)`; if the exiting instance's tab
    had a profile, set it on the active tab.
  - More: `tabs::open_adopted(model, ad)`.
- `handover::adopt_all(dir) -> Vec<Adopted>`: `read_dir`, keep `*.sock`,
  sort, `connect_and_adopt(&sock)` per entry; `None` → the socket is stale
  (keeper gone): `remove_file`.
- `connect_and_adopt(sock)`:
  - `UnixStream::connect(sock)` (fail → None), `set_read_timeout(5s)`.
  - `recv_master(stream.as_raw_fd())` → the master fd.
  - `BufReader`: `read_exact` 4 bytes → `u32` → `read_exact` the ring;
    `read_line` → `shell={pid}`; `read_line` → `profile={name}` (empty → None).
  - `write_all(&[1])` — the ack.
  - `Adopted { fd: File::from_raw_fd(fd), shell, profile, winsize: crate::link::winsize_of(fd), blob }`.
- `delegate::open_adopted(model, mark, ad) -> SurfaceId`:
  - `winsize = ad.winsize`; `desc.cols/rows` set; `open_bare(model, mark,
    cols, rows, &desc)`.
  - `PtyLink::adopted(surface.clone(), shared, ad.fd, ad.shell, ad.winsize)`.
  - `note_profile(id, profile)` when present.
  - `surface.inject_output(&ad.blob)` — the output the shell produced while
    no instance ran.
- `tabs::open_adopted(model, ad)`: `next_ids` (leaf id + mark),
  `delegate::open_adopted`, `Tab { root: Leaf { ..., cols, rows },
  profile: ad.profile or the default }`, push, `active = last`,
  `terminal.show(surface)`.
- `delegate::setup`: when the active tab's surface `is_adopted`, skip
  `open_host` (the pane was adopted at startup); `mux::attach_queued` both ways.

### `PtyLink` (rune-term/src/link.rs)

```rust
enum Core {
    /// The spawned shell. `None` once a hand-over took the master.
    Spawned(Option<teletypewriter::Pty>),
    /// A shell a keeper holds. The link's fd is closed on drop.
    Adopted { fd: Option<File>, shell: u32 },
}
```

- `PtyLink::adopted(surface, shared, fd, shell, winsize)`: dups the fd for the
  reader/writer/size threads (as `spawn` does), spawns those threads, returns
  the link holding the master in `Core::Adopted`.
- `hand_over(&mut self) -> Option<(File, u32)>`:
  - Spawned: `dup(*pty.child.id)`, `u32::try_from(*pty.child.pid)`,
    `std::mem::forget(pty)` (the link drops without hanging up: the keeper
    keeps the master).
  - Adopted: `dup(fd)`, the stored shell.
- `is_adopted()`, `master()` accessors; `running()` uses the same fg-pgroup
  check on the master fd for both cores.
- `Drop`: Spawned → `drop(slot.take())` (the `Pty` drop hangs up and reaps, as
  before); Adopted → `drop(fd.take())` (closes the master).
- Thread bodies extracted (`reader_thread`, `writer_thread`, `size_thread`)
  shared by spawn and adopted.

### `Shared` (rune-term/src/delegate.rs)

- New: `profiles: Mutex<HashMap<SurfaceId, String>>`.
- `note_profile(id, profile)`, `profile_of(id)`, `is_adopted(id)`;
  `forget` also removes the profile.
- `open_host` calls `note_profile`.

### Registry dir

`handover::keepers_dir()`:
`Xdg::new().runtime_dir().or_else(|| xdg.state_dir())?` + `"rune-term/keepers"`.

### macOS quirks found while testing

- `sendmsg` on macOS rejects `msg_controllen` beyond the message's own control
  data: set it to `CMSG_SPACE(4)`, not the buffer length. A C probe:
  `controllen=64 → EINVAL`, `controllen=16 → ok`, and the received fd
  round-trips.
- `recvmsg` on macOS with a zero-length first `iov` returns 0 without the
  control message when the fd arrives *while blocked* (C probe sp4b:
  `recvmsg=0 controllen=0, no cmsg`). The fd is delivered only when it is
  already queued: when the fd is there before the call, the child gets
  `recvmsg=0 controllen=16 level=65535 type=1`. So the client sleeps
  `200 ms` after `connect` (the keeper's poll wakes on the listener, accepts
  and `send_fd`s in that time) before calling `recv_master` — the sp4
  pattern: the fd is queued, then the receiver reads it.
- The C probes also showed the received fd is a new number in the receiver.

## Files

- New: `rune-term-keeper/{Cargo.toml,src/lib.rs,src/main.rs}`;
  `rune-term/src/handover.rs`.
- Edited: workspace `Cargo.toml` (member + dep), `rune-term/src/link.rs`
  (Core/hand_over/adopted/Drop/threads), `rune-term/src/delegate.rs`
  (profiles, open_adopted, handover_all, setup, forget),
  `rune-term/src/tabs.rs` (open_adopted), `rune-term/src/app.rs`
  (fast_exit, adopt in init), `rune-term/src/lib.rs` (mod handover).
- Doc: this file.

## Tests

- `rune-term/src/handover.rs`: `the_keeper_hands_the_master_back_for_adoption`
  — spawns `/bin/sh` via teletypewriter, `hand_over_fd` into a temp dir,
  `adopt_all` retries until the keeper answers, asserts `ad.shell == shell`,
  writes `hi\r` through the adopted fd, asserts the shell answers `got:hi` on
  the surface through the spawned link's reader thread.
- `rune-term/src/link.rs` (pty_tests): add an adopted-link echo test if
  `PtyLink::adopted` isn't covered by the handover test.

## Open questions / limits

- Tab layout, scrollback, selections: not restored (only the shells). The new
  instance starts with a fresh tree; adopted shells are tabs.
- The ring is 8 KiB of gap output.
- Mux sessions: not part of this feature; tmux owns its panes.
- The keeper keeps the shell's master open for as long as the shell lives.
  If the user never relaunches, the shell runs on (the keeper reads and drops
  the output). When the shell exits, the keeper unlinks the socket and exits.
- Only `unix` + `feature = "pty"`: the whole feature is gated.
- No SIGHUP reaches the shell on handover; a user closing the adopted pane
  after relaunch does hang up its shell (Adopted drop).
- `just agent-install` precedent: no `keeper-install` added (sibling lookup
  covers `cargo run` and dev; worth considering later).

## Pointers

- User's ask: "iterm2 let you update itself and not lose the open terminals."
- Keeper protocol: `rune-term-keeper/src/main.rs::serve_connection`.
- Handover call: `rune-term/src/delegate.rs::handover_all` (fast_exit hook).
- Adoption: `rune-term/src/handover.rs::adopt` (app.init closure).
