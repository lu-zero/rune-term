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
      fractional line.
- [x] **6. Split model.** The tree is in cells. A divider costs one cell.
      `distribute` spends a resize left or top first and will not take a
      leaf's last cell. The view is still one pane.
- [x] **10. Multiple panes.** A tab can hold more than one leaf. Each
      leaf has a surface and an effect mark. A split sizes the leaves
      from `distribute`, and the divider costs one cell. A click on a
      leaf moves focus. Injecting into one leaf does not dirty the
      other. One leaf still fills the window. Keys, the wheel, and the
      compositor still address the first pane.
- [x] **11. Selection and clipboard.** A drag the program did not take
      selects cells. Two clicks select a word and three select a line.
      The copy chord copies that text, and paste calls `Surface::paste`.
      OSC 52 still drops the payload and logs its type and length. The
      tint swaps the selected cells. Keys, the wheel, and the pointer
      still address the first pane.
- [x] **12. Scrollback viewport.** A bar beside each pane tracks
      `history_size` and `display_offset`. Dragging the thumb calls
      `Surface::scroll`. The thumb is painted in the compositor, so a
      wheel does not commit the view. The cursor stays hidden while
      the view is in history. Shift+PageUp and Shift+PageDown are
      unchanged.
- [x] **13. Search.** Cmd+F (Ctrl+Shift+F off macOS) opens a field
      over the pane. The field takes focus, so a key does not reach
      the shell. `Surface::search` matches are tinted on the viewport
      rows they occupy. A match above the viewport is not drawn.
- [~] **14. IME.** `on_ime` handles preedit/commit/enable/disable, commits
      text to the focused surface, mounts an `ime_cursor` positioned from
      cursor/cell metrics, and overlays preedit text in the grid source at the
      cursor. Preedit overlay handles wide-char display widths/spacers and
      highlights the IME selection byte range. Manual macOS marked-text
      verification and richer clause/caret styling are still needed.
- [x] **15. Kitty images.** Kitty placements are read from `RenderState`,
      uploaded/cached by image stamp, and drawn in z-buckets (under-text and
      over-text) around the grid pass. Headless `graphics` test injects a small
      kitty image and verifies pixels. Sixel/iTerm2 remain item 18.
- [ ] **16. Settings.** The palette and the 14 point face are fixed in the
      tree.
- [x] **17. Box-drawing and braille sprites.** Box drawing, block elements
      (`U+2580–U+259F`), braille, legacy computing sextants/octants, powerline/
      extra separators (`U+E0B0–U+E0BF`, `U+E0D2`, `U+E0D4`), geometric corner
      triangles, and branch drawing glyphs (`U+F5D0–U+F60D`) are rasterized into
      the gray atlas and cached like glyphs, matching the Rio/sugarloaf sprite
      families this tree targets. Other symbols still come from the face or
      cascade.
- [ ] **18. Sixel and iTerm2.**
- [~] **19. Hyperlinks and URL opening.** OSC 8 and detected URLs can be
      hovered and opened, without breaking mouse-reporting apps or selection.
      - [~] **19a. OSC 8 hyperlinks.** Reads `RenderState::link_at`/`link_run`
            and underlines the hovered run via a `GridSource` tint. Full hover
            affordance/pointer polish still pending.
      - [~] **19b. Implicit URL detection.** Uses `librio::Surface::url_at`
            (OSC 8 takes precedence). Matching scope/punctuation rules follow
            librio's implementation; rune-term does not add a parallel detector.
      - [~] **19c. Hit-test and interaction.** Bare motion resolves hover and
            forces redraw (probe cached by cell); modifier+click (Cmd on macOS,
            Ctrl elsewhere) opens, clears the local selection, and consumes the
            gesture before selection/mouse-report handling. Pointer/wheel/physical
            key handlers can target a pane by mark and physical input is wired per
            pane. Click/focus updates rune-term's focused leaf and shown surface.
            Modifier-press hover refresh/cursor affordance and observing Rune focus
            traversal for multi-pane focus sync need Rune API/events.
      - [~] **19d. Open action by host OS.** Spawns `open` (macOS), `xdg-open`
            (Linux/FreeDesktop), or `cmd /C start` (Windows) on a background
            thread and logs spawn failures. Non-web schemes are classified and
            logged; `open_uri_with` provides a confirmation/policy hook. Mod+click
            prompts for risky/non-web schemes with a modal confirmation; safe web
            and mailto schemes open directly.

## Outside this repository

- [x] **4. Rune paired text.** `PhysicalInput::Key` carries `text` and
      `consumed_mods` on `lu-zero/rune` branch `physical-key-text`.
- [x] **`RenderState::update_rearm`.** On `lu-zero/rio` branch
      `librio-update-rearm`. This tree calls it.
- [~] **3. Reflow.** Join-then-split is on Rio's local branch
      `rio-vt-reflow`. This tree builds without that branch.
      `preserve_scrollback` is still false on macOS in the plan.

The design header still records an empty workspace. This page is the status.
