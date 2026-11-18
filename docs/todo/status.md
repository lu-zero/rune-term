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
      other. The compositor keeps a grid per pane on screen and draws
      each leaf's surface in its own rect. Search, hover, preedit, and
      images stay on the focused pane. Dragging a divider moves it by
      whole cells, and neither side gives up a leaf's last cell. A window
      resize scales the tree from the layout the user left, so each split
      keeps its proportion; a session's windows get the same through
      `select-layout`. Cmd+D splits right and Shift+Cmd+D down, as the pane
      menu's Split items do; the new pane takes focus. Cmd+W and Close
      Pane close the focused pane, focus goes across its divider, and the
      tab closes with its last pane. On a session tab they kill the pane in
      tmux, and the window with its last pane, as iTerm2 and WezTerm do;
      so does the tab's close button. A session profile set to detach on
      close hides the window instead, still running, as does General's
      default for a profile that does not choose. The group label's
      menu detaches the session. A close that would end a program other than a shell
      asks first: a local pane by its foreground process group, a session's
      by `#{pane_current_command}`.
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
- [~] **16. Settings.** Config loads from and saves to
      `$XDG_CONFIG_HOME/rune-term/` (`config.toml`, `profiles/<name>.toml`,
      `themes/<name>.toml` in WezTerm's color-scheme shape). An unreadable
      file is not overwritten. `settings.adaptive_theme` names a dark scheme
      and a light scheme; a profile theme wins, and the desktop appearance is
      sampled when the pair is used. Defaults/profile font size and theme are
      applied on init
      and Cascadia Code NF metrics scale with font size. A settings window
      (Cmd+,) opens on its own, built with rune-kit and drawn in the terminal's
      own palette. It has a profiles list and a themes list that behave the
      same: apply, save, revert, duplicate, delete and create, with the theme
      on screen the one the editor is showing. A General tab, with no list,
      holds what is the app's: asking before a close ends a program, and what
      closing a session tab does. The editor shows Name, Author,
      and Source, then Basic beside UI, then ANSI, Cube, and Gray. Those rows
      stack as the window narrows, and the cube flows by group. ANSI is the
      normal row, then the bright row. The cube and the
      gray ramp are the xterm indexes 16–231 and 232–255; an edit stores that
      index, and the xterm color removes it. UI is the window chrome. A swatch
      names itself
      in a tooltip, and clicking one on a custom theme opens the color editor.
      The config is written back and the change is pushed into the running
      terminal, where a font size change keeps the grid's cell count and snaps
      the window. The font family is
      stored but not yet applied to the atlas, and per-tab setting application
      remains. Pictures: [`docs/config.md`](../config.md). Design:
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
      uniform creation/attach template. Config supports named profile files
      with default-profile resolution/fallback/persistence, and `Profile`
      carries `font_family`/`font_size`/`theme` plus `backend` (default
      `local`), `shell` (program/args), and `cwd` wired into local
      `SurfaceDesc` creation. Tabs record a profile; `open_with_profile`
      (and pane context menu entries) creates a tab for a selected profile,
      falling back to the default for unknown names. The settings window's
      profiles tab views profiles, sets the default, renames, duplicates,
      creates, edits and deletes them, all persisted. A profile whose
      `backend` is `tmux` or `rmux` attaches its `session` (a new one when
      unset), on its ssh `host` when it has one; its tabs take the profile.
      A session tab's menu saves the group as such a profile, and
      `--profile NAME` opens one. The settings window edits the backend,
      and for a session its host and name in place of the shell fields; a
      profile a tab saves reaches its open copy of the config. Per-tab/per-pane setting application (font/theme
      are currently model-global) remains. A tab group (a labelled group of
      tabs in the strip) is in the tree, ready for a multiplexer session group.
      Pressing its label collapses it to `name | count` and the selected tab.
      Dragging a tab reorders the strip: a group's tab moves only among that
      group's tabs, and dragging the label moves the group as one block. The
      order is the strip's own, not tmux's. Tearing a tab off into a window
      of its own is not built: one `Model` owns the window's surfaces, PTY
      routes and muxes.
      Design:
      [`settings-profiles.md`](./settings-profiles.md),
      [`profiles-ipc-cli.md`](./profiles-ipc-cli.md),
      [`tear-out.md`](./tear-out.md).
- [ ] **21. IPC and automation.** Owner-scoped local control plane
      (versioned, capability-gated): query/events, then create-from-profile /
      focus / close / resize, then input/capture, targeting `it2`-like
      scripting. Design: [`profiles-ipc-cli.md`](./profiles-ipc-cli.md).
- [ ] **22. CLI, docs, and tooling.** CLI aligned with IPC using `winnow-args`,
      with generated man pages/reference docs; `justfile` for build/test/run and
      install across lean/fat/helper compositions. Design:
      [`profiles-ipc-cli.md`](./profiles-ipc-cli.md).
- [~] **23. Multiplexer-backed sessions.** Explicit session concept and
      backend abstraction (local `librio` vs multiplexer), covering create /
      attach / detach / resume, multi-client policy, and remoting/share
      capabilities. tmux first, then rmux; ssh and mosh are raw links. The
      layers, the mapping (session → group, window → tab, pane → leaf) and
      the P0–P4 order are decided. P0 is in: librio is built without its
      `pty` feature, and rune-term spawns the shell on a teletypewriter PTY,
      reads it into `inject_output` on a thread, and writes the delegate's
      `output` back to it. The winsize follows the grid. P1 is in:
      `term-mux` holds the layout in cells and the tmux control-mode parser
      (replies, `%output`, window, layout, session and exit notifications,
      and the layout string), tested on tmux 3.4's own output. P3 is
      partial: `rune-term attach SESSION` runs `tmux -C new-session -A` on
      pipes. The pane menu's tmux submenu starts a session, starts one under a name it
      prompts for (attaching it when the name exists), or attaches one the
      default server lists, other than those already attached here. The session is a tab group, each window a tab, each pane a
      surface fed in tmux's order, with panes that predate the client
      redrawn from `capture-pane`. Keys go back as `send-keys -H`. The
      size goes as `refresh-client -C`, and focus as `select-window` and
      `select-pane`. A new tab in the group is `new-window`, and the menu
      splits. Closing a tab hides its window, and the menu shows it again
      from a Hidden Windows submenu, by window index. A tab's own menu hides
      the window, kills it with `kill-window`, or detaches the session.
      tmux exiting closes the group, and a split window draws every
      pane. Dragging a divider sends `resize-pane`, and the tree follows
      the layout tmux reports. A pane takes tmux's size in stream order,
      never from layout, and a resized pane is redrawn from
      `capture-pane`, since this copy's reflow pulls back history tmux
      does not. The pane answers queries tmux
      already answered ([`rio/replies.md`](./rio/replies.md)). rmux speaks
      the same control mode: `attach --rmux SESSION` and an rmux submenu, shown
      when rmux is installed, attach it the same way. `attach --host HOST` runs
      either on a host as `ssh -T -o BatchMode=yes HOST tmux -C …`, each
      argument quoted for the far shell; the group is `session@host`.
      The menu lists the sessions on each host a profile or an open group
      names, from that host's last answer, asked again in the background
      each time the menu opens. A host's submenu starts a session there, named or not, and Connect to Host… names a host no profile does, which the menu then lists for the rest of the run. A profile's Server, tmux's `-L` name, picks a server other than the default, here or on its host; the menu lists each such server as it does a host, and `attach --server NAME` takes one. `rune-term sessions [HOST]` prints them, and `rune-term profiles` the profiles `--profile` opens. A link that drops after reaching its session
      greys its group; the tab menu's Reconnect attaches it again into the
      same tabs, and tmux redraws every pane. Why a link failed, ssh's
      last line when it was ssh, shows in a notice in the window. Design:
      [`multiplexer-sessions.md`](./multiplexer-sessions.md).
- [ ] **24. Backend adapters and composition.** Fat/lean packaging (features
      and/or `rune-term-*` helpers) with progressive adapters: local, rmux
      (local), ssh/tmux (wrapper or native), rmux (web-share client), osewa,
      and experimental rmux-roaming/predictive work. Design:
      [`multiplexer-sessions.md`](./multiplexer-sessions.md),
      [`profiles-ipc-cli.md`](./profiles-ipc-cli.md).
      Roaming, and shells that outlive the UI, are `rune-term-agent`: rmux
      control mode over QUIC first, then recovery streams, then a local PTY
      host. Phase 0, the rmux-sdk spike, passed: a keyframe rebuilds a librio
      surface exactly, and a stalled reader gets one rebase, not a backlog.
      Phase 1 is built: a profile's Transport, or `attach --roam`, starts
      `rune-term-agent serve` over ssh and reaches control mode over QUIC
      after. A link that drops retries on its own, waiting 1 s and doubling
      to 32 s, and starts at most one new agent until one answers. Tried
      against an aarch64 Linux host: ticket in 0.3 s, a command's round trip
      4 ms. Deployment: [`../agent.md`](../agent.md), `just agent-install`. Design:
      [`agent.md`](./agent.md).

## Outside this repository

What Rio still needs, sent and drafted, is [`rio/`](./rio/README.md).

Until these land upstream, `[patch]` builds against forks: Rune and the winit
it vendors from `lu-zero/rune` branch `physical-key-text`, and librio and
teletypewriter from `lu-zero/rio` branch `rune-term-on-main`. wgpu is
pinned at `=30.0.1`; Rune itself takes any 30.x.

- [x] **4. Rune paired text.** `PhysicalInput::Key` carries `text` and
      `consumed_mods` on `lu-zero/rune` branch `physical-key-text`.
- [x] **Wakeup order and focus reports.** Upstream: the host clears its
      flag before `RenderState::update` (#2053), and `Surface::set_focus`
      reports focus (#2051).
- [~] **3. Reflow.** `lu-zero/rio` branch `rune-term-on-main`: Rio main
      plus our prompt and resize commits. A multi-line bash prompt survives a width change only
      with OSC 133 marks, from `rune-term shell-integration bash` or
      `zsh`.
      `preserve_scrollback` is still false on macOS in the plan.

The design header still records an empty workspace. This page is the status.
