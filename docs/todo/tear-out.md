# More than one window, and tearing a tab out

Status: plan, not built. Reorder within one strip is built (`status.md`, item 20).

## Goal

- Cmd+N opens a second terminal window, as the key table in `rune-term.md`
  already lists.
- A tab dragged off the strip opens in a window of its own, under the pointer,
  with its shell, scrollback and size intact. A tab group (a tmux session)
  goes as a whole.
- Later: a tab dropped on another window's strip joins that window.

## What stands in the way

One `Model` is one window, and it owns everything:

- `engine: librio::Engine`. librio fixes the delegate per engine
  (`Engine::new(delegate)`), and numbers surfaces per engine. A surface cannot
  move to another engine.
- `shared: Arc<Shared>`, the delegate. It holds the window's waker, the event
  queue, the PTY links (`links`) and tmux input routes (`mux_input`) by
  surface, and the IPC control queue.
- `terminal`: the `HostSurface` of each shell, each with its `RenderState`.
- `muxes`: one `Mux` per attached tmux session. Its reader thread pushes
  updates to `shared` for its group.
- `_ipc`: the control socket server. A second `Model` would start a second
  one on the same path.
- The view state: tabs, groups, the selected tab, search, prompts and drags.

Rune is ready for the window itself: `rune::open_window(WindowSpec)` opens one
at the end of the turn, and `WindowSpec::state(..)` hands it a state that is
already built. Each window has its own app, so its own `GridCompositor`.
Headless runs open nothing.

## Proposed split

**The process** (`Hub`, one, built before the first window):

- The `Engine` and the `Shared` delegate it was made with.
- Per surface: which window shows it. A wakeup, a close or an action is
  queued for that window and wakes that window's waker.
- PTY links and tmux input routes stay keyed by surface, as now.
- The IPC server, answering for every window (`ListTabs` lists them all).
- The config. An applied setting reaches every window, not only the one
  that holds the settings window's handle.

**Each window** (`Model`, as now, minus the above):

- Tabs, groups, the selected tab, view state.
- `terminal`: the `HostSurface`s it shows. A `HostSurface` moves between
  windows with its tab. Its GPU grid does not: the new window's compositor
  builds one on its first frame.
- `muxes`: a session's `Mux` lives in the window that shows its group, and
  moves with the group.

All windows run on Rune's one UI thread, so the hub needs no lock for what
only the UI thread touches. The reader threads already go through `Shared`'s
mutexes.

## Moving a tab

On the UI thread, in the drop handler:

1. Take the tab out of the source `Model`: its `Tab`, its `HostSurface`s,
   and, for a group, the `Mux` and the group entry.
2. Point those surfaces at the new window in the hub's routing table.
3. Build the new `Model` around them and open it with
   `open_window(WindowSpec::from_app(key, app()).state(model))`, placed at
   the drop's screen point.
4. The source window shows its next tab, or closes if that was its last.

A tab whose window does not exist yet must not lose output. The routing table
points at the new window's queue before the window starts, and its first
`poll` drains it.

A lone tmux tab does not tear out by itself in this plan: its `Mux` serves
the whole session. The tab drags its group with it, or the drag is refused.
Splitting a session across windows would need the `Mux` in the hub.

## Phases

1. **Hub.** Move `Engine`, `Shared`, IPC and config out of `Model`. One
   window still. Tests: every existing test, unchanged in behaviour.
2. **Cmd+N.** A second window with a fresh shell. Tests: output and closes
   reach the right window, IPC lists both, and closing one window leaves the
   other's shells running.
3. **Move to New Window.** A tab menu entry, no drag yet. Tests: the shell
   keeps its scrollback and size, and a tmux group moves with its `Mux`.
4. **Drag tear-out.** Our strip's drag gets a detach distance across the
   strip (rune-kit uses 36 points) and opens the window at the release
   point. Tests at the decision points: past the distance it tears out,
   within it it reorders.
5. **Drop onto another window's strip.** Needs a cross-window drop from
   Rune (its `TabDragState::hand_off` is the start of one). Report to Rune
   first.

## To confirm before phase 1

- Splitting `Model` changes a type most modules use. Confirm the shape (a
  `Hub` beside `Model`, reached through `Model`) before the mechanical part.
- Whether a tmux tab may leave its session's window alone. The plan says no.
- What closing a window does to a tmux group in it: detach, as closing the
  app does now, or hide its windows.
