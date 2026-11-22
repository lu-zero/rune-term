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

## Upstream since our fork

Rio `main` moved from `e7c05e43` (19 Nov) to `d2400983` (22 Nov). Triage
against the host layer rune-term actually uses:

- [#2121](https://github.com/raphamorim/rio/commit/d2400983c39be5715c85d69138047f78ba583dd7):
  a large kitty-keyboard rewrite (shifted alternates and associated text,
  keypad/DECKPAM, `modifyOtherKeys` levels and editing keys, releases).
  **Ported** into `term-input` (upstream tests included): shifted alternates,
  associated text, keypad/DECKPAM and `modifyOtherKeys` levels now work. This
  also carries the Ctrl/Shift+Enter work from #2105, so those fork patches are
  now obsolete.
- [#2115](https://github.com/raphamorim/rio/commit/bbd83b03b275d166aed4d7b37977f95c4959cb96)
  and [#2116](https://github.com/raphamorim/rio/commit/6ca11531e5ffac0fe2305dc1ee10b410398d9575):
  paste encoding moves to `rio_vt::paste` with xterm semantics and a
  `Surface::paste_is_safe` confirmation hook. **Ported** into `term-vt`, with
  a confirmation dialog in the app before an unsafe paste is sent.
- [#1747](https://github.com/raphamorim/rio/commit/d449cc273253a091649c5c40a74ee7a998f46c0e):
  `~` expansion and validation of `working_dir` moves into teletypewriter's
  shared module. **Free on rebase:** rune-term passes `SurfaceDesc.working_dir`
  straight to `create_pty_with_spawn`.
- #2107 (Glyph Protocol host support) is already in `term-vt`; our delegate
  leaves the protocol off by default. #2108 (`adopt_surface`) is present but
  unused: rune-term owns its PTY.

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

rune-term builds against `lu-zero/rio` branch `for-rune-term`, rebased on
`origin/main` at `d2400983` (fork head `993a7cb3`). Only two of its non-merge
changes are still
needed: `1b80f604` (prompt rows on resize) and `d8b4aad3` (PTY size after
resizes settle), both in `rio-vt` and listed in [`reflow.md`](./reflow.md).
Neither has an upstream issue or PR. The rest is obsolete: librio's key
encoding moved to `term-input`, `update_rearm` landed as #2053 and lives in
`term-vt`, #2084 made the empty-input patch unnecessary, and #2121 carried the
Ctrl/Shift+Enter changes upstream.
