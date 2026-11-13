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
- [~] **16. Settings.** Config loads from and saves to TOML
      (`rune-term.toml`, XDG/platform path); unreadable/unparseable config is
      not overwritten. Defaults/profile font size and theme are applied on init
      and Cascadia Code NF metrics scale with font size. The previous in-app
      settings UI has been removed pending rework; manual TOML editing is the
      configuration path for now. Named profile UI/switching, live in-app
      editing (including font/theme/family/profile flows), font family wired to
      atlas, and broader settings polish remain. Design:
      [`settings-profiles.md`](./settings-profiles.md). Follow-on:
      [`profiles-ipc-cli.md`](./profiles-ipc-cli.md).
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
            and underlines the hovered run via a `GridSource` tint. App-level
            modifier-hover refresh and pointer cursor affordance are wired when
            the activation modifier is held over a hovered link; full upstream
            hover-modifier integration/polish still pending.
      - [~] **19b. Implicit URL detection.** Uses `librio::Surface::url_at`
            (OSC 8 takes precedence). Matching scope/punctuation rules follow
            librio's implementation; rune-term does not add a parallel detector.
      - [~] **19c. Hit-test and interaction.** Bare motion resolves hover and
            forces redraw (probe cached by cell); modifier+click (Cmd on macOS,
            Ctrl elsewhere) opens, clears the local selection, and consumes the
            gesture before selection/mouse-report handling. `App::on_modifiers`
            refreshes hover affordance on modifier changes while a link is
            hovered (pointer cursor when activatable). Pointer/wheel/physical
            key handlers can target a pane by mark and physical input is wired
            per pane. Click/focus updates rune-term's focused leaf and shown
            surface. Observing Rune focus traversal for multi-pane focus sync
            tracks upstream `anomalyco/rune#106` (`El::on_focus_change`).
            Native stationary-pointer modifier hover events/cursor affordance
            still track upstream `anomalyco/rune#393`.
      - [~] **19d. Open action by host OS.** Spawns `open` (macOS), `xdg-open`
            (Linux/FreeDesktop), or `cmd /C start` (Windows) on a background
            thread and logs spawn failures. Non-web schemes are classified and
            logged; `open_uri_with` provides a confirmation/policy hook. Mod+click
            prompts for risky/non-web schemes with a modal confirmation; safe web
            and mailto schemes open directly.

## Planned (design drafted)

These items extend the original plan; designs are drafted, implementation not
started. Order follows `profiles-ipc-cli.md` and `multiplexer-sessions.md`.

- [~] **20. Profiles and creation.** Named/default profiles and “open with
      profile”, applied at session/tab/pane creation (settings, shell/cwd/env),
      then a versioned, capability-validated `backend`/target stanza as the
      uniform creation/attach template. Config supports named `[profiles.*]`
      with default-profile resolution/fallback/persistence, and `Profile`
      carries `font_family`/`font_size`/`theme` plus `backend` (default
      `local`), `shell` (program/args), and `cwd` wired into local
      `SurfaceDesc` creation. Tabs record a profile; `open_with_profile`
      (and pane context menu entries) creates a tab for a selected profile,
      falling back to the default for unknown names. Settings includes a
      Profiles tab to view profiles, set the default, duplicate, create, and
      delete (persisted). Per-tab/per-pane setting application (font/theme are
      currently model-global), richer profile editing, and full creation-time
      backend semantics remain. Design:
      [`settings-profiles.md`](./settings-profiles.md),
      [`profiles-ipc-cli.md`](./profiles-ipc-cli.md).
- [ ] **21. IPC and automation.** Owner-scoped local control plane
      (versioned, capability-gated): query/events, then create-from-profile /
      focus / close / resize, then input/capture, targeting `it2`-like
      scripting. Design: [`profiles-ipc-cli.md`](./profiles-ipc-cli.md).
- [ ] **22. CLI, docs, and tooling.** CLI aligned with IPC using `winnow-args`,
      with generated man pages/reference docs; `justfile` for build/test/run and
      install across lean/fat/helper compositions. Design:
      [`profiles-ipc-cli.md`](./profiles-ipc-cli.md).
- [ ] **23. Multiplexer-backed sessions.** Explicit session concept and
      backend abstraction (local `librio` vs multiplexer), covering create /
      attach / detach / resume, multi-client policy, and remoting/share
      capabilities. rmux-first, applicable to tmux-class backends. Design:
      [`multiplexer-sessions.md`](./multiplexer-sessions.md).
- [ ] **24. Backend adapters and composition.** Fat/lean packaging (features
      and/or `rune-term-*` helpers) with progressive adapters: local, rmux
      (local), ssh/tmux (wrapper or native), rmux (web-share client), osewa,
      and experimental rmux-roaming/predictive work. Design:
      [`multiplexer-sessions.md`](./multiplexer-sessions.md),
      [`profiles-ipc-cli.md`](./profiles-ipc-cli.md).

## Outside this repository

- [x] **4. Rune paired text.** `PhysicalInput::Key` carries `text` and
      `consumed_mods` on `lu-zero/rune` branch `physical-key-text`.
- [x] **`RenderState::update_rearm`.** On `lu-zero/rio` branch
      `librio-update-rearm`. This tree calls it.
- [~] **3. Reflow.** Join-then-split is on Rio's local branch
      `rio-vt-reflow`. This tree builds without that branch.
      `preserve_scrollback` is still false on macOS in the plan.

The design header still records an empty workspace. This page is the status.
