# rune-term

A terminal emulator. Rune owns the window. [librio](https://github.com/raphamorim/rio)
owns the PTY, the VT, and the pull-based render state. The cell grid is a wgpu
pass on the device Rune already created.

macOS first. The crates are unpublished. This page is about the repository.

## What runs

One window, one pane, and a tab strip. The face is Cascadia Code NF at 14
points, the palette is Flexoki dark, and a 2 point margin sits around the
grid. A codepoint the face does not cover, including a color emoji, is drawn
by the macOS font cascade.

The PTY winsize is `cols * cell_px` by `rows * cell_px`. Leftover device
pixels are padding. The cell count updates while the window is dragged.

Keys go to the shell, copied from the platform text and the modifiers that
text already consumed. Command chords stay with the window.

[`docs/todo/rune-term.md`](./docs/todo/rune-term.md) describes a larger
program. This tree has the window, the grid, the keyboard, and the winsize.
Tabs beyond the one pane, splits, mouse, selection, search, IME, images, and
soft-wrap reflow of scrollback are still in that document. Reflow lives on
Rio's `rio-vt-reflow` branch and is not required to build here.

## Crates

- `crates/term-grid`: the cell pass, the atlas, and the solid fill. No Rune
  and no librio. The macOS cascade is `fallback`.
- `crates/term-input`: a physical key to a `librio::KeyEvent`, the app-chord
  check, and cell hit testing. No window types.
- `crates/rune-term`: the binary and the `AppSpec`.

`images`, `ime`, `search`, `selection`, and `sprite` are empty seams.

## Building

Two sibling checkouts sit next to this repository:

- `../rune`, on `dev`, with `PhysicalInput::Key` carrying platform `text` and
  `consumed_mods`.
- `../rio`, on `librio-update-rearm`, with `RenderState::update_rearm`. The
  atlas compiles in Cascadia Code NF from
  `rio/sugarloaf/src/font/resources/CascadiaCode/CascadiaCodeNF.ttf`.

The link line is `rune` and `librio`. `sugarloaf` as a crate, `rio-window`,
`rioterm`, `rio-backend`, and WezTerm's GUI stay out of it. wgpu is pinned at
`=30.0.1`, the same crate as Rune's device.

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

## Documents

- [`docs/todo/rune-term.md`](./docs/todo/rune-term.md): the design and the
  work that is still ahead.

## License

MIT: [`LICENSE`](./LICENSE). Cascadia Code NF is SIL Open Font License 1.1:
[`crates/term-grid/CascadiaCode-LICENSE.txt`](./crates/term-grid/CascadiaCode-LICENSE.txt).
