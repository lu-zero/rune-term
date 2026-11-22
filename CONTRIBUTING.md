# Contributing to rune-term

This page covers the build, the tests, the crate layout, and the PTY handover.
`AGENTS.md` has the rules we follow when an agent helps with the code; you do
not need it just to build.

## Getting set up

- **macOS** is the platform we build on. Rune draws with Metal, so the Xcode
  command-line tools are all it needs.
- **Linux** needs `libxkbcommon` to link the window. Depending on your session
  and the features built, Wayland or X11 client libraries, fontconfig and dbus
  may be needed too. Package names vary by distribution.
- **Headless GPU tests** (`grid_pipeline`, and `headless` without default
  features) render through wgpu. They need a Vulkan or OpenGL driver, or a
  software renderer such as Mesa's lavapipe.
- wgpu stays pinned at `=30.0.1`. `Cargo.lock` is gitignored.

## How the crates fit together

`term-vt` holds the host layer copied from librio without its C ABI: `Surface`,
`RenderState`, the delegate and the terminal modes. It calls Rio's patched
`rio-vt`, `rio-graphics`, `corcovado` and `teletypewriter` directly.
`term-input` owns the key types and encoder and depends on neither Rune nor Rio.

`term-host` owns generic terminal state — surfaces, grid sync, scrollback,
selection, search and input delivery — with no Rune dependency.
`rune-term-core` is the Rune adapter: Rune-side key, pointer-phase and colour
conversions live there.

`term-atlas` rasterizes glyphs, sprites and fallback into CPU pixel buffers;
`term-grid` uploads those buffers to wgpu. The atlas stores either RGBA or ARGB
bytes, chosen at construction, so a CPU frontend can share it. `term-font`
finds system faces and rasterizes COLR v1 colour glyphs; its parser is adapted
from Rune and stays out of `term-grid`.

## Fonts

`font-bundled` embeds Maple Mono NF, the default face. `font-fallback-common`
adds common Han, kana and Hangul and is on by default. `font-fallback-ja` and
`font-fallback-zh-full` add more coverage and weight; `font-cascadia` adds a
second selectable face. `emoji-fallback` adds a small monochrome Noto Emoji
subset for hosts with no emoji face. Multi-codepoint emoji are shaped with
rustybuzz against the installed emoji face or that subset.

With `--no-default-features` no fonts are embedded at all. The GUI then picks
any installed family (monospaced first) and shows a download link only if no
font is installed.

The `term-font-data-*` crates hold zstd-compressed TTFs, decoded once at
runtime by the pure-Rust `ruzstd`. `term-atlas/fallback-subset.py` regenerates
them from the installed V8 NF CN/JP/KR faces; it needs `fonttools` and the
`zstd` CLI.

## Dependencies and patches

Sibling checkouts (`../rune`, `../rio`, `../wezterm`) are for reading. The
workspace depends on upstream `rune` and Rio's crates; `[patch]` replaces those
sources with the `physical-key-text` and `for-rune-term` branches. A local
checkout can replace a patch entry with a path. Never copy WezTerm source in,
and never link `sugarloaf`, `rio-window`, `rioterm`, `rio-backend`, or WezTerm's
`window` or `mux`.

`term-font`, `term-atlas` and the `term-font-data-*` crates are on crates.io.
The rest wait on Rune or Rio releases, or on names already taken; 0.0.1
placeholders reserve those names.

Code adapted from Rio and Rune is credited in `THIRD-PARTY-NOTICES.md`. The
font data crates are OFL-1.1, not MIT, and each ships its font's notice in
`LICENSE-OFL.txt`. Keep both up to date when you lift code or add a face.

## Tests and the pre-commit gate

Run `just check-all` before committing. It checks formatting, runs clippy with
`-D warnings` on both feature sets, and runs the library tests:

```
cargo test -p term-grid --lib
cargo test -p term-input --lib
cargo test -p rune-term --lib
cargo test -p rune-term --no-default-features --lib
cargo test -p rune-term --test grid_pipeline
cargo test -p rune-term --no-default-features --test headless
```

`just test-headless` adds the headless snapshots;
`cargo build --release -p rune-term` checks the full link.

## Site and generated docs

`just shots` regenerates the captures in `docs/images`. `just site` stages
README.md, the docs pages and those images into `target/site-src`, then builds
an mdBook into `target/site`; `just site-serve` previews it on port 3000. mdBook
is needed (`cargo install mdbook`), and edits under `docs/` need a re-run of
either site recipe.

`just docs-cli` regenerates `docs/cli/` and `man/` from the `winnow-args`
derive spec. Commit both with CLI changes.

## The PTY handover

The handover lets local shells survive the app. It is off by default. Pass
`rune-term --handover` at startup, or run `rune-term-ctl handover` against a
running instance. Either writes a marker that makes the next launch adopt the
shells when this one exits.

### The keeper (`rune-term-keeper`)

One per shell, spawned at exit. It receives the master fd over a socketpair
via `SCM_RIGHTS`, so it holds the master open and the shell never sees a
hangup; it reads the shell's output into an 8 KiB ring; and it binds
`keepers/<shell-pid>.sock` in the keepers dir — etcetera's Xdg runtime dir,
else its state dir, + `rune-term/keepers`. When serving the next instance it
sends the fd, then the gap output as `u32 len + bytes`, then the line
`shell=N\nprofile=P\n`, then waits for a 1-byte ack and exits (unlinking the
socket).

### Handover at exit

`app::app()` registers `rune::fast_exit(delegate::handover_all)` — the app's
final step before `_exit`. The hook gates on `handover::requested()`; when on,
it drains `Shared.links` (local shells; mux panes aren't links) and for each
`PtyLink::hand_over()` — dup the master, `mem::forget` the `Pty` (no hangup) —
calls `handover::hand_over_fd`: socketpair, `send_fd` the master, spawn the
keeper with `--socket --shell --profile`, master on stdin.

### Adoption at startup

`app.init` calls `handover::adopt` (gated on `requested()`):
`adopt_all(keepers_dir)` connects to every `keepers/*.sock` — connect,
`recv_master`, read the ring and the `shell=`/`profile=` lines, ack — into
`Adopted { fd, shell, profile, winsize, blob }`. The first shell becomes the
initial pane (`chrome_view::attach_initial`); the rest go through
`tabs::open_adopted` (one tab each). `PtyLink::adopted` dups the fd into the
reader/writer/size threads and `surface.inject_output(&blob)` replays the gap
output, so the shell sees nothing of it. `delegate::setup` skips `open_host`
for adopted panes.

### The opt-in

- `rune-term --handover` — `cli::Gui::run` → `handover::set_requested(true)`:
  sets the `REQUESTED` atomic, writes the marker `keepers/handover.on` (so the
  next launch adopts), and warns if the keeper isn't findable.
- `rune-term-ctl handover` — the RPC `Request::Handover { on: true }` handled
  in `ipc::handle_request` → `set_requested(true)`: the running instance
  flips (atomic + marker); its exit hands the shells over.
- Keeper lookup (`handover::keeper_exe()`): `CARGO_BIN_EXE_rune-term-keeper`
  (tests), the sibling of the running `rune-term` exe, a walk up the
  ancestors, then the bare name on PATH. If not findable at request time:
  `rune-term: the shell keeper was not found; the handover will not happen at exit`.

### `PtyLink`

`Core::{Spawned(Option<Pty>), Adopted { fd: Option<File>, shell: u32 }}`.
`running()` uses `tcgetpgrp` on the master for both cores. `Drop`: Spawned →
the `Pty` drop hangs up and reaps; Adopted → closes the fd.

### macOS quirks

- `sendmsg` rejects `msg_controllen` beyond the message's own control data:
  use `CMSG_SPACE(4)`.
- `recvmsg` with a zero-length iov doesn't deliver the fd when it arrives
  while blocked: the client sleeps 200 ms after `connect` so the keeper has
  queued it.

### Tests

`cargo test -p rune-term --lib handover cli::tests`:

- `handover::tests::the_keeper_hands_the_master_back_for_adoption` — a real
  `/bin/sh` goes through a real keeper: spawn, `hand_over_fd`, `adopt_all`,
  write `hi\r` through the adopted master, assert the shell answers `got:hi`.
- `link::tests::an_adopted_link_answers_a_shell_line_through_the_adopted_master`.
- `cli::tests::the_handover_flag_opts_the_gui_in`.
- `rune-term-keeper`'s `tests::the_protocol_sends_and_receives_an_fd` — the
  socketpair round trip.

### Design doc

`docs/todo/pty-handover.md` is the design.
