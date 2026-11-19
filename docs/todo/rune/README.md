# Pending in Rune

What rune-term would like from `anomalyco/rune`. Rune does not take pull
requests: a change is asked for in an issue, with the commit on the fork
linked. Nothing here is committed to Rune from this tree.

## Sent

- [#250](https://github.com/anomalyco/rune/issues/250): `PhysicalInput::Key`
  should carry the platform text and the modifiers it consumed. The fork has
  it as `13cd5229` and `e6cd73e9`.
- [#1169](https://github.com/anomalyco/rune/issues/1169): Option as Alt should
  name the key, left, right or both. The fork has it as `2f925ff5`.
- [#1173](https://github.com/anomalyco/rune/issues/1173): menu bar submenus,
  and an action that knows which item fired, so the Session menu can list
  sessions.
- [#393](https://github.com/anomalyco/rune/issues/393): modifier changes while
  the pointer is still. `App::on_modifiers` (#240) answers it; the issue is
  still open.

## Following

- [#106](https://github.com/anomalyco/rune/issues/106): `El::on_focus_change`,
  so a pane sees focus that Rune moves. Not ours. The fork's `3265158e` (a
  text field that runs on blur) is related and has not been offered there.

## Drafted, not sent

- [`tab-strip-groups.md`](./tab-strip-groups.md): rune-kit's `TabStrip` has no
  tab groups, so rune-term keeps its own strip for tmux sessions.

## On the fork

rune-term builds against `lu-zero/rune` branch `physical-key-text`: Rune `dev`
and the four commits above.
