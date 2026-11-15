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

## Still wrong with bash

After a width change, readline moves up by the rows the input took at the old
width, and redraws only the last line of a multi-line prompt. Rio has already
rewrapped those rows, so readline lands too high and erases the line above
(starship's first line). A drag repeats it once per step. Kitty and Ghostty
avoid it with OSC 133 marks from their own shell integration: the prompt and
input are not rewrapped, and the shell redraws them.

## Upstreaming

Decide with Raphael: 1 and 3 can go as they are. 2 changes #2058's stated
behaviour, so ask first whether he wants it.
