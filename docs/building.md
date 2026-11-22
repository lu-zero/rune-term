# Building and testing

## System dependencies

- **macOS**, the platform this builds on: Rune draws with Metal, so the Xcode
  command-line tools are all it needs.
- **Linux**: the window needs `libxkbcommon`; linking fails without it.
  Wayland or X11 client libraries, fontconfig and dbus may be needed as well,
  depending on the session and the features built. Package names vary by
  distribution.
- **Headless GPU tests** (`grid_pipeline`, and `headless` without default
  features) render through wgpu and need a graphics adapter: Vulkan or OpenGL
  drivers, or a software one such as Mesa's lavapipe on a machine without a
  GPU.

## Features

`pty`, the default, spawns shells. Without default features, rune-term is the
inject harness: it draws what it is fed and spawns nothing, which the window
and settings tests use.

## Tests

`just check-all` is the gate before a commit: formatting, clippy with
`-D warnings` on both feature sets, and the lib tests. One at a time:

```
cargo test -p term-grid --lib
cargo test -p term-input --lib
cargo test -p rune-term --lib
cargo test -p rune-term --no-default-features --lib
cargo test -p rune-term --test grid_pipeline
cargo test -p rune-term --no-default-features --test headless
```

The tmux and rmux tests run their own servers on private sockets, and skip
when the program is not installed.

## Crates

- `term-atlas`: the CPU glyph atlas, sprites, faces and fallback. No rendering
  backend, no Rune and no Rio.
- `term-font`: system font discovery and colour glyph rasterization. No Rune
  and no Rio.
- `term-font-data-*`: the zstd-compressed bundled faces read by `term-atlas`.
- `term-grid`: the wgpu cell pass over `term-atlas` pixels. No Rune and no Rio.
- `term-vt`: librio's host layer without the C ABI; the app's terminal state.
  It needs Rio's unreleased 0.6.0 fixes, so it is not on crates.io yet.
- `term-input`: physical keys to `term_input::KeyEvent`, and cell hit testing. No
  window types.
- `term-mux`: tmux control mode and its layouts, in cells.
- `term-roam`: the QUIC link between rune-term and rune-term-agent.
- `term-host`: generic terminal state and input delivery. No Rune.
- `term-control`: the IPC protocol `rune-term-ctl` speaks.
- `rune-term-core`: the Rune adapter over `term-host`; Rune-side conversions.
- `color-picker`, `font-picker`: settings widgets.
- `rune-term`: the binary.
- `rune-term-ctl`: the control command line.
- `rune-term-agent`: the roaming end on a host ([`agent.md`](./agent.md)).
