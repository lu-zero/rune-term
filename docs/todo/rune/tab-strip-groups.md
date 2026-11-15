# rune-kit `TabStrip`: tab groups

Checked against `anomalyco/rune` `dev` at `7a92d24d`. No open or closed issue
asks for this.

## Title

rune-kit `TabStrip`: tab groups that reorder, collapse and tear out as one

## Body

rune-term shows a tmux session as a group of tabs, one per tmux window. Its
own strip draws:

- a label ahead of the group's tabs, in a colour taken from the group's name,
- the group's tabs outlined and washed in that colour,
- `name | count` and only the selected tab once the label is pressed
  (collapsed),
- and, on a drag, a group's tab moving only among that group's tabs, while a
  drag on the label moves the label and its tabs as one block.

`TabStrip` already does the reorder springs, the overflow and the tear-out
this strip would like to use. What it lacks is groups. Its reorder lanes are
pinned or not pinned (`Lane { lo, hi }`). A drop can put an ungrouped tab
inside a group or split one, and there is no element for the label.

A possible shape:

- `TabItem::group(id)`. A group's tabs are contiguous.
- `TabStrip::groups(..)` takes `TabGroup { id, label, colour, collapsed }`.
  The strip draws the label before the group's first tab and the colour on
  its tabs, or hands both to a closure as `decorate` does.
- A grouped tab reorders within its group. The label drags the whole group
  among the top-level items: ungrouped tabs and other groups. Its neighbours
  slide aside by the block's width.
- `on_group_reorder(GroupReorder { id, from, to })`, with indices in block
  order. `on_group_toggle(id)` fires on a press on the label. A collapsed
  group shows its label and its active tab.
- `on_group_tear_out`, so a whole group can go to a window of its own.

Open question: may a tab move from one group to another? rune-term says
no, because a tmux window belongs to its session. Browsers say yes.
