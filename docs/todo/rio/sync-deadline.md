# librio: nothing tells a host that feeds `inject_output` when a held synchronized update gives up

## Problem

Since #2000, `RenderState::update` keeps the last snapshot while a synchronized update (mode 2026) is active, and ends the update once `SYNC_UPDATE_TIMEOUT` (1 s) has passed. The PR notes that "output injected with inject_output (which has no event loop timer) cannot freeze the snapshot".

That holds only if something calls `update` after the deadline. With a PTY, the IO loop caches the deadline and wakes the renderer when it passes. A host that owns its transport (built without `pty`, as librio-wasm is) gets no wakeup for ordinary output at all — only full damage such as `CSI 2 J` wakes it — so it calls `update` after each feed. The update right after a feed that starts an update returns the held frame. If the program then goes quiet without ending the update, the host has nothing to schedule the next update by: the deadline is on `Crosswords`, and `Surface::terminal()` is `pub(crate)`.

## Reproduction

Built without `pty`, public API only:

```rust
let mut state = RenderState::new(&surface);
surface.inject_output(b"\x1b[1;1Ha");
state.update();
surface.inject_output(b"\x1b[?2026h\x1b[1;1Hb"); // BSU, never an ESU
state.update();
assert_eq!(state.square(0, 0).map(|sq| sq.c()), Some('a')); // held, as intended
// ...the program sends nothing more. The host cannot tell when the hold ends.
std::thread::sleep(Duration::from_millis(1200));
state.update();
assert_eq!(state.square(0, 0).map(|sq| sq.c()), Some('b')); // only an update nobody had a reason to make
```

## Proposed fix

Expose how long the hold has left, so the host can schedule one update for it:

```rust
impl Surface {
    /// Time until the active synchronized update gives up, if one is active.
    /// Every BSU re-arms it, and a resize or a reset ends the update, so read it
    /// again after each feed.
    pub fn sync_update_remaining(&self) -> Option<Duration>;
}
```

A `Duration` rather than an `Instant`: `rio_vt::time` is `web_time` on wasm and is not re-exported by librio, and the C ABI and librio-wasm can carry it as milliseconds (`rio_surface_sync_update_remaining_ms`, `sync_update_remaining_ms()`), so a JS host can `setTimeout` for it.

Related: librio-wasm's `update` says "call once per animation frame (after a wakeup)", but without a PTY ordinary output queues no wakeup.
