# librio: an embedder cannot tell the terminal's replies from user input

## Problem

Without `pty`, everything the terminal sends to the far end goes through one callback, `SurfaceDelegate::output`: keys, paste and mouse reports, and the terminal's own replies. Every reply is a `RioEvent::PtyWrite` from `Crosswords` — mode reports (DECRPM), DA1/DA2/XTVERSION, the kitty keyboard query, DSR/CPR, pixel and character size reports, XTSMGRAPHICS, kitty graphics responses, XTGETTCAP, the glyph protocol, OSC 7501 — and the Listener sends it to `output` like any key.

That matters when the far end is itself a terminal. A tmux control-mode client feeds each pane's `%output` into `inject_output`. `%output` carries the program's queries raw (with tmux 3.4: `\e[c\e[6n\e[>q\e]11;?\a\e[?u`), and tmux has already answered DA1, CPR, XTVERSION and OSC 11 itself. librio answers again, the host forwards the answer as input, and the program gets it twice, or as typed text. Some queries tmux leaves unanswered (here `\e[?u`), so a host cannot just drop every reply.

## Reproduction

Built without `pty`:

```rust
surface.inject_output(b"\x1b[c"); // the program asks for device attributes
surface.text("x");                // the user types
// output == [b"\x1b[?…c", b"x"]: two writes, indistinguishable
```

## Proposed fix

A separate delegate callback for replies, whose default forwards to `output` so every existing host is unchanged:

```rust
trait SurfaceDelegate {
    fn reply(&self, surface: SurfaceId, bytes: &[u8]) { self.output(surface, bytes) }
}
```

The host then decides per reply, as #2022 does inside rioterm for tmux control mode (`send_reply`, forwarding only what tmux leaves unanswered for its version). Ideally the callback says which query it answers, so a host need not parse the bytes back; input on that is welcome.
