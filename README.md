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

Next to this repository: `../rune` on `dev`, with `PhysicalInput::Key`
carrying `text` and `consumed_mods`, and `../rio` on `librio-update-rearm`,
with `RenderState::update_rearm`. The atlas compiles in Cascadia Code NF
from that Rio tree. wgpu is pinned at `=30.0.1`.

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
