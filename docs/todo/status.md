# Status

[`rune-term.md`](./rune-term.md) is the plan. This page is how far this tree
has got. `[x]` is in this tree. `[~]` is partial. `[ ]` is not started.
The numbers are that plan's.

## In this repository

- [x] **1. Foundation.** One window, one surface, a tab strip, and the
      headless inject harness.
- [x] **2. term-grid.** The cell pass, the gray atlas, and dirty-row upload.
      The face is Cascadia Code NF at 14 points, the palette is Flexoki dark,
      and a codepoint the face misses is drawn by the macOS cascade, color
      emoji included. `sprite` still returns nothing.
- [x] **5. term-input.** `map_key` copies platform text and `consumed_mods`.
      App chords, scroll chords, and `cell_at` are here. There is no
      US-QWERTY table.
- [x] **7. Wire the grid.** A live `RenderState` feeds the pass.
- [x] **8. Winsize.** The reported size is `cols * cell_px` by
      `rows * cell_px`. A 2 point margin sits outside it. Layout is
      `live_only`. The wrapped-line inject in the plan waits on reflow.
- [x] **9. Input.** Keys, wheel, and mouse reach the focused pane.
      Shift+PageUp and Shift+PageDown scroll. A wheel keeps its
      fractional line. Selection still starts nowhere.
- [x] **6. Split model.** The tree is in cells. A divider costs one cell.
      `distribute` spends a resize left or top first and will not take a
      leaf's last cell. The view is still one pane.
- [ ] **10. Multiple panes.**
- [ ] **11. Selection and clipboard.** `selection` is an identity tint.
      Paste does nothing. OSC 52 does not write the clipboard.
- [ ] **12. Scrollback viewport.** Page scrolling exists. There is no
      scrollbar.
- [ ] **13. Search.** `search` is an identity tint.
- [ ] **14. IME.** The caret element is the pane unchanged, and `on_ime`
      returns false.
- [ ] **15. Kitty images.** `images::draw` is empty.
- [ ] **16. Settings.** The palette and the 14 point face are fixed in the
      tree.
- [ ] **17. Box-drawing and braille sprites.** Box drawing and legacy
      computing symbols are stretched from the face. The sprite atlas is
      still empty.
- [ ] **18. Sixel and iTerm2.**

## Outside this repository

- [x] **4. Rune paired text.** `PhysicalInput::Key` carries `text` and
      `consumed_mods` on `lu-zero/rune` branch `physical-key-text`.
- [x] **`RenderState::update_rearm`.** On `lu-zero/rio` branch
      `librio-update-rearm`. This tree calls it.
- [~] **3. Reflow.** Join-then-split is on Rio's local branch
      `rio-vt-reflow`. This tree builds without that branch.
      `preserve_scrollback` is still false on macOS in the plan.

The design header still records an empty workspace. This page is the status.
