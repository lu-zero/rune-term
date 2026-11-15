# Reflow: our commits on top of #2058

`lu-zero/rio` branch `reflow-cursor-on-2058` is #2058 at `58b5568b` plus:

1. `1fe2ef9d` grid: a wide char that a shrink displaces counts in the cursor
   column. `abc中def` with the cursor on `f`, shrunk to 4 columns, left the
   cursor one cell past `f`.
2. `1ab70e17` grid: rows a shrink adds take the blank rows below the cursor
   before the top of the screen goes into history, and a grow on such a screen
   keeps its top row. Without it, a resize round trip after `clear` brought the
   cleared output back, and a cursor in the middle of a wrapped line on the
   top row was left on a later part of it. A full screen goes through history
   as before. This changes what three tests of #2058 expect: the line stays
   on screen instead of its head going to history.
3. `2a0d2f81` performer: an empty input no longer blocks the ones after it.
   Writing zero bytes returns `Ok(0)`, which `pty_write` read as a full PTY;
   the shell never got another key. This one stands apart from #2058.
4. `5bfcd869` performer: the PTY gets its new size once resizes settle
   (100 ms, kitty's default). The grid still reflows at once. A drag sent a
   SIGWINCH per frame, and bash piled a misplaced redraw on the line above
   for each.
5. `ff85480e` crosswords: between OSC 133 `A` and `C`, a column resize clears
   the input from the last prompt mark to the cursor and puts the cursor back
   at its old row offset below that mark. After a width change readline moves
   up by the rows its old layout put the cursor below the input's start, and
   redraws only the last prompt line; rewrapped, that landed on the line
   above and erased it. It needs a mark at the last prompt line: `P;k=s`.

## Shell integration

bash and starship send no OSC 133. rune-term has to add the marks to the
prompt itself, as kitty and Ghostty do. Without them, 5 does nothing and a
multi-line bash prompt loses a line per drag.

## Upstreaming

Decide with Raphael: 1, 3, 4 and 5 can go as they are. 2 changes #2058's stated
behaviour, so ask first whether he wants it.
