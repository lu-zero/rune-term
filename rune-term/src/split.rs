//! Split tree in cells. A divider costs one cell. `distribute` spends a
//! resize one cell at a time, left or top first, and will not take a leaf's
//! last cell.

use librio::SurfaceId;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct LeafId(pub u32);

/// `Horizontal` is left beside right. `Vertical` is top above bottom.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    Horizontal,
    Vertical,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Leaf {
    pub id: LeafId,
    pub surface: SurfaceId,
    pub mark: u32,
    pub cols: u16,
    pub rows: u16,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Node {
    Leaf(Leaf),
    Split {
        axis: Axis,
        left: Box<Node>,
        right: Box<Node>,
    },
}

impl Node {
    /// Divide this node on `axis`. The divider takes one cell. The left or
    /// top child gets half of what remains, rounded up. Fewer than three
    /// cells cannot hold two leaves and the divider.
    pub fn split(self, axis: Axis, mut other: Leaf) -> Option<Node> {
        let (cols, rows) = size(&self);
        let n = along(axis, cols, rows);
        if n < 3 {
            return None;
        }
        let left_n = (n - 1).div_ceil(2);
        let right_n = n - 1 - left_n;
        let (left, other) = match axis {
            Axis::Horizontal => {
                other.cols = right_n;
                other.rows = rows;
                (distribute(left_n, rows, self), other)
            }
            Axis::Vertical => {
                other.cols = cols;
                other.rows = right_n;
                (distribute(cols, left_n, self), other)
            }
        };
        Some(Node::Split {
            axis,
            left: Box::new(left),
            right: Box::new(Node::Leaf(other)),
        })
    }
}

/// Resize `tree` so its leaves and dividers sum to `total_cols` by `total_rows`.
/// A tree that is already that size is returned unchanged. A request smaller
/// than the tree's minimum stops at one cell per leaf.
pub fn distribute(total_cols: u16, total_rows: u16, tree: Node) -> Node {
    let (min_cols, min_rows) = min_size(&tree);
    let cols = total_cols.max(min_cols);
    let rows = total_rows.max(min_rows);
    let (cur_cols, cur_rows) = size(&tree);
    let tree = adjust(tree, Axis::Horizontal, cols as i32 - cur_cols as i32);
    adjust(tree, Axis::Vertical, rows as i32 - cur_rows as i32)
}

fn along(axis: Axis, cols: u16, rows: u16) -> u16 {
    match axis {
        Axis::Horizontal => cols,
        Axis::Vertical => rows,
    }
}

fn size(node: &Node) -> (u16, u16) {
    match node {
        Node::Leaf(leaf) => (leaf.cols, leaf.rows),
        Node::Split {
            axis: Axis::Horizontal,
            left,
            right,
        } => {
            let (left_cols, left_rows) = size(left);
            let (right_cols, _) = size(right);
            (
                left_cols.saturating_add(1).saturating_add(right_cols),
                left_rows,
            )
        }
        Node::Split {
            axis: Axis::Vertical,
            left,
            right,
        } => {
            let (left_cols, left_rows) = size(left);
            let (_, right_rows) = size(right);
            (
                left_cols,
                left_rows.saturating_add(1).saturating_add(right_rows),
            )
        }
    }
}

fn min_size(node: &Node) -> (u16, u16) {
    match node {
        Node::Leaf(_) => (1, 1),
        Node::Split {
            axis: Axis::Horizontal,
            left,
            right,
        } => {
            let (left_cols, left_rows) = min_size(left);
            let (right_cols, right_rows) = min_size(right);
            (left_cols + right_cols + 1, left_rows.max(right_rows))
        }
        Node::Split {
            axis: Axis::Vertical,
            left,
            right,
        } => {
            let (left_cols, left_rows) = min_size(left);
            let (right_cols, right_rows) = min_size(right);
            (left_cols.max(right_cols), left_rows + right_rows + 1)
        }
    }
}

fn measure(node: &Node, axis: Axis) -> u16 {
    let (cols, rows) = size(node);
    along(axis, cols, rows)
}

fn min_along(node: &Node, axis: Axis) -> u16 {
    let (cols, rows) = min_size(node);
    along(axis, cols, rows)
}

fn set_along(leaf: &mut Leaf, axis: Axis, value: u16) {
    match axis {
        Axis::Horizontal => leaf.cols = value,
        Axis::Vertical => leaf.rows = value,
    }
}

fn adjust(node: Node, axis: Axis, mut delta: i32) -> Node {
    if delta == 0 {
        return node;
    }
    match node {
        Node::Leaf(mut leaf) => {
            let cur = match axis {
                Axis::Horizontal => leaf.cols,
                Axis::Vertical => leaf.rows,
            };
            let next = (i32::from(cur) + delta).clamp(1, i32::from(u16::MAX)) as u16;
            set_along(&mut leaf, axis, next);
            Node::Leaf(leaf)
        }
        Node::Split {
            axis: split_axis,
            left,
            right,
        } if split_axis == axis => {
            let mut left = *left;
            let mut right = *right;
            if delta > 0 {
                while delta > 0 {
                    left = adjust(left, axis, 1);
                    delta -= 1;
                    if delta == 0 {
                        break;
                    }
                    right = adjust(right, axis, 1);
                    delta -= 1;
                }
            } else {
                while delta < 0 {
                    let left_can = measure(&left, axis) > min_along(&left, axis);
                    let right_can = measure(&right, axis) > min_along(&right, axis);
                    if !left_can && !right_can {
                        break;
                    }
                    if left_can {
                        left = adjust(left, axis, -1);
                        delta += 1;
                    }
                    if delta < 0 && measure(&right, axis) > min_along(&right, axis) {
                        right = adjust(right, axis, -1);
                        delta += 1;
                    }
                }
            }
            Node::Split {
                axis: split_axis,
                left: Box::new(left),
                right: Box::new(right),
            }
        }
        Node::Split {
            axis: split_axis,
            left,
            right,
        } => {
            let cur = measure(&left, axis);
            let floor = min_along(&left, axis).max(min_along(&right, axis));
            let next = (cur as i32 + delta).clamp(i32::from(floor), i32::from(u16::MAX));
            let applied = next - i32::from(cur);
            Node::Split {
                axis: split_axis,
                left: Box::new(adjust(*left, axis, applied)),
                right: Box::new(adjust(*right, axis, applied)),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pane(id: u32, cols: u16, rows: u16) -> Leaf {
        Leaf {
            id: LeafId(id),
            surface: id as usize,
            mark: id,
            cols,
            rows,
        }
    }

    fn leaf(id: u32, cols: u16, rows: u16) -> Node {
        Node::Leaf(pane(id, cols, rows))
    }

    fn leaf_cols(node: &Node) -> u16 {
        match node {
            Node::Leaf(leaf) => leaf.cols,
            Node::Split { .. } => panic!("expected a leaf"),
        }
    }

    #[test]
    fn an_unsplit_grid_keeps_80_by_24() {
        let tree = leaf(1, 80, 24);
        assert_eq!(distribute(80, 24, tree.clone()), tree);
    }

    #[test]
    fn a_split_of_80_columns_is_40_and_39() {
        let tree = leaf(1, 80, 24)
            .split(Axis::Horizontal, pane(2, 1, 1))
            .expect("80 columns hold a divider");
        match &tree {
            Node::Split {
                axis: Axis::Horizontal,
                left,
                right,
            } => {
                assert_eq!(leaf_cols(left), 40);
                assert_eq!(leaf_cols(right), 39);
                assert_eq!(size(&tree), (80, 24));
            }
            _ => panic!("expected a horizontal split"),
        }
        assert!(leaf(1, 2, 24)
            .split(Axis::Horizontal, pane(2, 1, 1))
            .is_none());
    }

    #[test]
    fn a_shrink_refuses_the_last_cell() {
        let tree = leaf(1, 80, 24)
            .split(Axis::Horizontal, pane(2, 1, 1))
            .expect("split");
        let shrunk = distribute(3, 24, tree);
        match &shrunk {
            Node::Split { left, right, .. } => {
                assert_eq!(leaf_cols(left), 1);
                assert_eq!(leaf_cols(right), 1);
            }
            Node::Leaf(_) => panic!("expected a split"),
        }
        assert_eq!(distribute(2, 24, shrunk.clone()), shrunk);
    }

    #[test]
    fn a_vertical_child_takes_the_whole_column_delta_on_both_sides() {
        let vertical = leaf(1, 80, 24)
            .split(Axis::Vertical, pane(2, 1, 1))
            .expect("24 rows hold a divider");
        let tree = vertical
            .split(Axis::Horizontal, pane(3, 1, 1))
            .expect("80 columns hold a divider");
        let grown = distribute(82, 24, tree);
        match grown {
            Node::Split {
                axis: Axis::Horizontal,
                left,
                right,
            } => {
                assert_eq!(leaf_cols(&right), 40);
                match *left {
                    Node::Split {
                        axis: Axis::Vertical,
                        left: top,
                        right: bottom,
                    } => {
                        assert_eq!(leaf_cols(&top), 41);
                        assert_eq!(leaf_cols(&bottom), 41);
                        assert_eq!(size(&top).1, 12);
                        assert_eq!(size(&bottom).1, 11);
                    }
                    _ => panic!("expected the vertical child"),
                }
            }
            _ => panic!("expected a horizontal split"),
        }
    }
}
