# rune-term

A terminal emulator. Rune owns the window. librio owns the PTY, the VT, and
the render state. The cell grid is a wgpu pass on the device Rune already
created.

macOS first. The crates are unpublished. This page is about the repository.
The plan and what is still missing are in [`docs/todo/`](./docs/todo/).

## Crates

- `term-grid`: the cell pass and the atlas. No Rune and no librio.
- `term-input`: physical keys to `librio::KeyEvent`. No window types.
- `rune-term`: the binary.

## Building

`rune` and `librio` are the upstream git dependencies. `[patch]` replaces
them with [lu-zero/rune](https://github.com/lu-zero/rune) branch
`physical-key-text` and [lu-zero/rio](https://github.com/lu-zero/rio) branch
`librio-update-rearm`. Cascadia Code NF is vendored in `term-grid/`. wgpu
is pinned at `=30.0.1`.

### System dependencies

**macOS:** the target platform. Rune uses its Metal GPU backend on Apple
platforms (system Metal/frameworks), so there are no separate Wayland/X11
runtime packages to install; the normal macOS/Xcode command-line toolchain is
expected.

**Linux (Wayland/X11):** building/running the windowed app and linking tests
that depend on Rune/winit requires the platform input and display libraries.
At minimum `libxkbcommon` is required (linking fails without it). Depending on
the session/backend and enabled features, Wayland and/or X11 client libraries
and common desktop dependencies (e.g. fontconfig/dbus) may also be needed.

Headless GPU tests (`grid_pipeline`, `--no-default-features --test headless`)
use Rune/wgpu headless rendering and require a working wgpu graphics adapter
(e.g. Vulkan or OpenGL with appropriate drivers; on headless machines a
software adapter such as Mesa/lavapipe is often used). Package names vary by
distribution.

```
cargo run -p rune-term
cargo test -p term-grid --lib
cargo test -p term-input --lib
cargo test -p rune-term --lib
cargo test -p rune-term --test grid_pipeline
cargo test -p rune-term --no-default-features --test headless
```

`cargo run -p rune-term` opens the window. The default feature is `pty`.
`--no-default-features` is the inject harness and does not spawn a shell.

## License

MIT: [`LICENSE`](./LICENSE). Cascadia Code NF is SIL Open Font License 1.1:
[`term-grid/CascadiaCode-LICENSE.txt`](./term-grid/CascadiaCode-LICENSE.txt).
