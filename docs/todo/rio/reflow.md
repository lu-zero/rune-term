# Comment on #2058: two shrink cases the PR leaves

Checked against #2058 at `58b5568b`. The patch is
[`patches/rio-2058-cursor-reflow.patch`](./patches/rio-2058-cursor-reflow.patch),
on top of the PR. Not sent.

## Comment

Thanks, this fixes the round trip we hit with the cursor after the line's last
character. Running our reproductions against the branch turned up two more,
both in `shrink_columns`:

**A cursor in the middle of its line leaves the screen.** Three rows,
`abcdefghij` on the top one, the cursor on `d`, and a shrink to 4 columns.
The cursor's char is now in `abcd` in history, but the cursor stays on
`Line(0)`, which holds `ij`: `max(row - 1, Line(0))` clamps it there. The grow
back then rejoins the line in history, with its wrap flag still set. The screen
is blank and the cursor is at `(0, 1)`. The two blank rows below the line
could have taken the wrap instead. A shell's line editor puts the cursor there
whenever you move back through a long command.

**A displaced wide char is not counted in the cursor column.** `abc中def`, the
cursor on `f` (column 7), and a shrink to 4 columns. The spacer pushes `中` to
the next row, so that push keeps 3 cells, not 4. The cursor moves back by 4
and lands at `(0, 3)`, after `f`.

A fix at the same level, on top of the branch:

- When the cursor is on the top row and a row would push it up, count the rows
  (`cursor_above`) instead of clamping. After the loop, drop that many clear
  rows from the bottom, while the grid still has more rows than the screen.
  That pulls the line back down under the cursor. With no blank rows below
  (a one-row screen, or a full one), nothing changes.
- Move the cursor back by `columns - displaced`, the cells the push kept,
  rather than by `columns`.

Two tests cover them: the mid-line round trip and the wide char.
`shrink_reflow_remap_tracks_displaced_wide_char` now expects `[0, 2, -1]`. Its
cursor sits at the default `(0, 0)`, on the line being wrapped, so the
trailing blank row is dropped. `cargo test -p rio-vt` (631) and
`cargo test -p librio` pass, and clippy with `-D warnings` and fmt are clean.

Happy to open it as a PR against your branch, or leave it for you to fold in.

## Not in the comment

- Trailing spaces wrap on a shrink (`"hello   "` at 8 to 6 columns). That
  follows rio-vt's rules: a blank cell is `'\0'`, and a written space is
  content.
- With a blank row under the line but not enough of them, the drop moves the
  screen down without bringing the cursor's char back on screen. The cursor is
  still on the wrong char, as it is today.
