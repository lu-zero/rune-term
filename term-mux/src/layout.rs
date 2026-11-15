/// `Horizontal` is side by side. `Vertical` is stacked. rune-term's split tree
/// uses the same words.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    Horizontal,
    Vertical,
}

/// A window's panes as the multiplexer laid them out. Neighbouring children
/// are divided by one cell, as rune-term's split tree is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Layout {
    Pane {
        id: u32,
        cols: u16,
        rows: u16,
    },
    Split {
        axis: Axis,
        cols: u16,
        rows: u16,
        children: Vec<Layout>,
    },
}
