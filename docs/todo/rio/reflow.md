# rio-vt: column reflow can leave the cursor's line in scrollback and the screen blank

## Problem

`Grid::resize` reflows by shuffling cells between neighbouring rows, one pair at a time. When the cursor is on a wrapped line, a narrow-then-wide round trip rejoins the line into scrollback: the rejoined row is in history with a stale wrap flag on its last cell, and the screen is left blank. The shrink alone already moves the cursor off its character.

## Reproduction

```rust
// 3 lines, "abcdefghij" on line 0, cursor on 'd' at (0, 3)
grid.resize(true, 3, 4);
// [-2: abcd~] [-1: efgh~] [0: ij..]   cursor (0, 3): a blank cell past "ij", not on 'd'
grid.resize(true, 3, 10);
// [-1: abcdefghij~] [0..2: blank]     the line is in history, still marked wrapped;
//                                      the screen is blank, cursor (0, 1)
```

The same happens on a one-line screen with the cursor anywhere on the line. With the cursor on a later line, the round trip is fine.

Two smaller ones, in the same function:

- shrinking `"hello   "` (explicit space cells) from 8 to 6 columns wraps the spaces into a second row;
- with a wide character ahead of the cursor, a shrink misplaces it: on `"abc中def"` with the cursor on `f`, shrinking to 4 columns leaves the cursor on the blank cell after `f`.

## Direction

Reflow whole logical lines: join a line's wrapped rows, split at the new width, and place the cursor by its offset in the logical line, counting wide characters. I have a join-then-split implementation, but it is not ready, and these are the questions I'd like to settle first:

- where a cursor at column 0 lands (on its own row unless it continues a wrapped line);
- whether trailing space cells are content (a styled space, a prompt's trailing space) or padding;
- whether a screen cleared after output should stay blank, or refill from scrollback when it grows;
- keeping a fast path for a resize with no wrapped rows, and remapping `display_offset`;
- interaction with #1974 (the cursor over a wide character) and #1707 (resize artifacts).
