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

impl Layout {
    /// Layout size in cells.
    pub fn size(&self) -> (u16, u16) {
        match self {
            Layout::Pane { cols, rows, .. } | Layout::Split { cols, rows, .. } => (*cols, *rows),
        }
    }

    /// One cell a pane, and one between neighbours.
    fn min_size(&self) -> (u16, u16) {
        match self {
            Layout::Pane { .. } => (1, 1),
            Layout::Split { axis, children, .. } => {
                let sizes: Vec<(u16, u16)> = children.iter().map(Layout::min_size).collect();
                let gaps = children.len() as u16 - 1;
                let (cols, rows) = sizes
                    .iter()
                    .fold((0, 0), |(c, r), (cols, rows)| match axis {
                        Axis::Horizontal => (c + cols, r.max(*rows)),
                        Axis::Vertical => (c.max(*cols), r + rows),
                    });
                match axis {
                    Axis::Horizontal => (cols + gaps, rows),
                    Axis::Vertical => (cols, rows + gaps),
                }
            }
        }
    }

    /// The layout at `cols` by `rows`, each split keeping its children's
    /// proportions. Scaled from the same layout, a resize a cell at a time does
    /// not drift.
    pub fn scaled(&self, cols: u16, rows: u16) -> Layout {
        let (min_cols, min_rows) = self.min_size();
        self.scale_to(cols.max(min_cols), rows.max(min_rows))
    }

    fn scale_to(&self, cols: u16, rows: u16) -> Layout {
        let (axis, children) = match self {
            Layout::Pane { id, .. } => {
                return Layout::Pane {
                    id: *id,
                    cols,
                    rows,
                }
            }
            Layout::Split { axis, children, .. } => (*axis, children),
        };
        let along = |(c, r): (u16, u16)| u32::from(if axis == Axis::Horizontal { c } else { r });
        let total = along((cols, rows)) - (children.len() as u32 - 1);
        let old: Vec<u32> = children.iter().map(|child| along(child.size())).collect();
        let sum: u32 = old.iter().sum::<u32>().max(1);
        let mut shares: Vec<u32> = old.iter().map(|o| o * total / sum).collect();
        // Largest remainder: the cells rounding left over go to the children
        // closest to their next one.
        let mut order: Vec<usize> = (0..children.len()).collect();
        order.sort_by_key(|&i| std::cmp::Reverse(old[i] * total % sum));
        let short = total - shares.iter().sum::<u32>();
        for &i in order.iter().take(short as usize) {
            shares[i] += 1;
        }
        let mins: Vec<u32> = children
            .iter()
            .map(|child| along(child.min_size()))
            .collect();
        while let Some(i) = (0..shares.len()).find(|&i| shares[i] < mins[i]) {
            let Some(donor) = (0..shares.len()).max_by_key(|&j| shares[j] as i64 - mins[j] as i64)
            else {
                break;
            };
            if shares[donor] <= mins[donor] {
                break;
            }
            shares[donor] -= 1;
            shares[i] += 1;
        }
        let children = children
            .iter()
            .zip(&shares)
            .map(|(child, &share)| match axis {
                Axis::Horizontal => child.scale_to(share as u16, rows),
                Axis::Vertical => child.scale_to(cols, share as u16),
            })
            .collect();
        Layout::Split {
            axis,
            cols,
            rows,
            children,
        }
    }
}
