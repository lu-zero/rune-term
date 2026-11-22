# For coding agents

[`docs/todo/rune-term.md`](./docs/todo/rune-term.md) is the design: crate
boundaries, the frame path, and the winsize. Read it before a change that
touches those. Leave it as it stands unless you were asked to edit it.
[`docs/todo/status.md`](./docs/todo/status.md) is how far that plan has got.
When a plan item lands or is dropped, update the status page in the same
change.

## Slop warning

This tree was largely written with an agent. Be skeptical of existing code,
comments, and docs: a pattern being there does not make it correct.
`term-grid/src/sixel.rs` is empty on purpose; the earlier empty seams for
images, IME, search, selection and sprites are now implemented.

## Working here

- Master stays linear: merge commits are forbidden, and `git merge` is
  blocked by the `pre-merge-commit` hook; use rebase or cherry-pick.
- A change that has to land in Rune or Rio is reported. Those trees are not
  committed from this repository.
- Do not bump versions or publish immediately after a fix. Wait for the user's
  go-ahead so more fixes and improvements can land in the same release.
- `docs/todo/` is committed. It is the design, so nothing cites it as
  scratch and nothing gitignores it.
- A commit you assisted carries `Assisted-by: AGENT:MODEL`
  (`Assisted-by: Grok:grok-4.7`). Never add `Co-Authored-By` or a
  `Signed-off-by`: the sign-off is the human's.
- `cargo run -p rune-term` opens a window. Run it when the user asked to see
  the shell.

## Unslop rules

**Comments**
- Default to no comment. Add one only when the *why* is non-obvious: a hidden
  constraint, a workaround for a specific bug, an invariant a reader would
  break without warning.
- One sentence beats a paragraph. A comment that needs several paragraphs to
  justify a few lines means the code needs simplifying.
- Never restate what the names already say.
- Never reference the task, a commit, a PR or issue number, or a session:
  that belongs in the commit message.
- No commented-out code and no `// removed: …` markers; git is the history.
- Hard limits: no comment line over **150 characters**, no paragraph over
  **5 lines** (3 is better). Hitting either means cut, not wrap. Longer
  justification goes in the commit message or the design doc.

**Dead weight**
- No speculative abstraction for a single call site: no knob, trait, or
  feature flag without a second caller that needs it today.
- No error handling, fallback, or validation for a case the caller's own
  guarantees rule out.
- `#[allow(dead_code)]` does not keep something "just in case": delete it.
- An empty seam stays empty until the work it stands for is asked for.
  `term-grid/src/sixel.rs` is the one left; filling it in passing is that work.

**Dependencies**
- Every dependency, external or in-tree, is declared once in the root
  `[workspace.dependencies]`. A crate names it as `name.workspace = true`,
  adding only `optional` or `features`, never its own version, path, or git source.
- Base directories (home, config, runtime) come from `etcetera`. Never build
  them from `HOME`, `XDG_*`, `APPDATA`, or a platform path such as
  `~/Library/Application Support`.

**Tests exercise the terminal, not the standard library**
- No test whose assertions would hold for any correct implementation of the
  primitive underneath.
- One test at the real decision point (a branch, an edge case, a regression)
  beats a test added so that "added a function, added a test" is true.

**Scope: stay inside what you were asked for**
- When work surfaces a different, deeper subsystem, stop and report before
  editing it. Agreement that an idea has merit is not authorization.
- A signature change that cascades through many call sites is a point to
  confirm the shape before the mechanical part. `resize::apply` is one of
  these: its signature stays.
- A change that was reverted once needs fresh, explicit confirmation to try
  again.

## Settled

- The winsize is `cols * cell_px` by `rows * cell_px`. The 2 point margin
  sits outside it. Layout is `live_only`, and the floored count is not
  written back into the slot.
- `map_key` copies platform `text` and `consumed_mods`. It does not
  synthesize a US-QWERTY string. `logo` maps to super. Command chords stay
  with the window.
- The cell is `face_cell()` of Maple Mono NF at 14 points. A cmap miss uses
  the bundled fallback tiers, then a system font `term-font` finds; CoreText
  is the last resort on macOS. Multi-codepoint emoji are shaped with rustybuzz
  against an installed emoji face or the bundled monochrome subset. Faces are
  Cargo features; a build with none uses any installed font, or reports where
  to download one if there is none. No color emoji file is embedded.
