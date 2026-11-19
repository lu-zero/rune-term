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
- [#2082](https://github.com/raphamorim/rio/pull/2082): an empty input stopped
  later ones. Closed: #2084's writer thread never writes zero bytes.
- [#2105](https://github.com/raphamorim/rio/issues/2105): Ctrl+Enter encodes
  to `\r` like plain Enter. In legacy mode it should use Ctrl+J's byte (`\n`)
  so applications can bind the chord without kitty negotiation. PR:
  [#2110](https://github.com/raphamorim/rio/pull/2110).

## Drafted, not sent

- [`sync-deadline.md`](./sync-deadline.md): nothing tells an injecting host
  when a held synchronized update gives up. The patch in `patches/` exposes an
  `Instant`; the review asked for `sync_update_remaining() -> Option<Duration>`
  plus a milliseconds call for C and wasm. Revise it before sending.
  `patches/rune-term-sync-deadline.patch` is the rune-term side.
- [`replies.md`](./replies.md): the terminal's replies and user input share
  `SurfaceDelegate::output`. A tmux pane gets librio's answers on top of
  tmux's own.
- [`reflow.md`](./reflow.md): our prompt and resize commits on top of Rio
  main, and how to offer them.

## Not verified

- OSC 10/11/4 colour queries, `CSI 14 t`, and OSC 52 reads seem to go
  unanswered. Confirm before drafting.

## On the fork

rune-term builds against `lu-zero/rio` branch `rune-term-on-main`: Rio main
and two commits of ours on top, listed in [`reflow.md`](./reflow.md).
Everything the older `reflow-cursor-on-2058` and `librio-update-rearm-reflow`
branches carried is upstream or replaced.
