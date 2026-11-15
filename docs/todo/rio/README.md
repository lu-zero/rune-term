# Pending in Rio

What rune-term needs from `raphamorim/rio`. Each draft was confirmed against
upstream `main` before it was written. Nothing here is committed to Rio from
this tree.

## Sent

- [#2049](https://github.com/raphamorim/rio/pull/2049): document when a
  wakeup repeats, and to clear a host flag before `RenderState::update`.
  Landed as #2053. `update_rearm` on the fork is not needed.
- [#2050](https://github.com/raphamorim/rio/issues/2050): an embedder cannot
  report focus (DECSET 1004). Fixed by #2051: `Surface::set_focus(bool)`,
  which leaves the view and the selection alone.

## Drafted, not sent

- [`sync-deadline.md`](./sync-deadline.md): nothing tells an injecting host
  when a held synchronized update gives up. The patch in `patches/` exposes an
  `Instant`; the review asked for `sync_update_remaining() -> Option<Duration>`
  plus a milliseconds call for C and wasm. Revise it before sending.
  `patches/rune-term-sync-deadline.patch` is the rune-term side.
- [`replies.md`](./replies.md): the terminal's replies and user input share
  `SurfaceDelegate::output`. A tmux pane gets librio's answers on top of
  tmux's own.
- [`reflow.md`](./reflow.md): column reflow can leave the cursor's line in
  scrollback and the screen blank. #2058 (open) fixes it for a cursor after
  the line's last character, the shell prompt's usual place. A cursor in the
  middle of the line still ends in a blank screen. Trailing space cells still
  wrap, and a wide character still moves the cursor. Rewrite the draft as a
  follow-up to #2058.

## Not verified

- OSC 10/11/4 colour queries, `CSI 14 t`, and OSC 52 reads seem to go
  unanswered. Confirm before drafting.

## On the fork

rune-term builds against `lu-zero/rio` branch `librio-update-rearm-reflow`.
Upstream now covers all three of its commits:

- `update_rearm`: #2053 documents the order instead.
- `notify_focus`: #2051 adds `set_focus`. The fork's version goes through
  `Surface::write`, which drops the selection and scrolls on every tab switch.
- The reflow: it ships regressions upstream does not have (a column-0 cursor
  moves up a row, a cleared screen refills from scrollback, trailing spaces
  are dropped, `preserve_scrollback` crept in). #2058 fixes the common case
  without them.

Moving back to upstream `main` needs `set_focus` in place of `notify_focus`,
the update without `update_rearm`, and arms for `Action::ProgramStatus` and
`Action::CommandFinished` (#2039, #2043) in `rune-term/src/delegate.rs`.
