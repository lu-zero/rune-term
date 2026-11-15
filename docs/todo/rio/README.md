# Pending in Rio

What rune-term needs from `raphamorim/rio`. Each draft was confirmed against
upstream `main` before it was written. Nothing here is committed to Rio from
this tree.

## Sent

- [#2049](https://github.com/raphamorim/rio/pull/2049): document when a
  wakeup repeats, and to clear a host flag before `RenderState::update`.
  With it, `update_rearm` on the fork is not needed.
- [#2050](https://github.com/raphamorim/rio/issues/2050): an embedder cannot
  report focus (DECSET 1004). A PR was offered: `Surface::notify_focus` that
  does not go through `Surface::write`.

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
  scrollback and the screen blank.

## Not verified

- OSC 10/11/4 colour queries, `CSI 14 t`, and OSC 52 reads seem to go
  unanswered. Confirm before drafting.

## On the fork

rune-term builds against `lu-zero/rio` branch `librio-update-rearm-reflow`.

- Its reflow ships regressions upstream does not have: a column-0 cursor moves
  up a row, a cleared screen refills from scrollback, the cursor is off next to
  a wide character, trailing spaces are dropped, and `preserve_scrollback`
  crept in. Fix them or drop the branch's reflow.
- `notify_focus` goes through `Surface::write`, which drops the selection and
  scrolls to the bottom on every tab switch. #2050 replaces it.
- Rebasing onto upstream brings `Action::ProgramStatus` and
  `Action::CommandFinished` (#2039, #2043); the match in
  `rune-term/src/delegate.rs` needs arms for them.
