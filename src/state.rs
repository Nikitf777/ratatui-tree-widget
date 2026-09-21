use std::collections::BTreeSet;

use crate::node::single_child_chain;
use crate::widget::{node_columns, node_index_at_col, BranchChars};
use crate::TreeNode;

// ---------------------------------------------------------------------------
// FlattenedItem – a single visible line in the rendered tree
// ---------------------------------------------------------------------------

/// A single line produced by flattening the tree for rendering.
///
/// When a chain of nodes each has exactly one child, they are collapsed into
/// a single visual line.  `nodes` contains every node in that chain; `path`
/// points to the *last* node in the chain (the one that could possibly have
/// >1 child and be toggled).
///
/// Paths have the form `[root_index, child_index, …]`.  The first element is
/// the index of the root the subtree belongs to.
#[derive(Debug, Clone)]
pub struct FlattenedItem<'a> {
    /// Path to the last (deepest) node in this visual line.
    pub path: Vec<usize>,
    /// Every node that appears on this line (chain of single-child nodes).
    pub nodes: Vec<&'a TreeNode>,
    /// Visual depth (number of multi-child ancestors above this line).
    pub depth: usize,
    /// Whether the last node in the chain has children.
    pub has_children: bool,
    /// Whether the last node is currently expanded.
    pub expanded: bool,
}

impl FlattenedItem<'_> {
    /// The display text for this line, joining single-child chains with ` → `.
    pub fn display_text(&self) -> String {
        self.nodes
            .iter()
            .map(|n| n.text.as_str())
            .collect::<Vec<_>>()
            .join(" → ")
    }

    /// The node that can be expanded/collapsed (the last in the chain).
    pub fn toggle_node(&self) -> &TreeNode {
        self.nodes.last().copied().unwrap()
    }

    /// Number of nodes on this line (length of the single-child chain).
    pub fn num_nodes(&self) -> usize {
        self.nodes.len()
    }
}

/// Path of the `index`-th node of a flattened line.
///
/// A line merges a chain of single-child nodes: `item.path` points at the last
/// node of the chain, and every earlier node sits one level up with a
/// trailing `0`.
fn node_path_in(item: &FlattenedItem, index: usize) -> Vec<usize> {
    let len = item.nodes.len();
    let index = index.min(len.saturating_sub(1));
    let base_len = item.path.len().saturating_sub(len.saturating_sub(1));
    let mut path = item.path[..base_len].to_vec();
    path.extend(std::iter::repeat_n(0, index));
    path
}

// ---------------------------------------------------------------------------
// TreeState
// ---------------------------------------------------------------------------

/// Tracks selection and expansion state for a forest of [`TreeNode`]s.
///
/// Selection is split into two components:
/// - `selected_row` – which visual line (flat index) is active
/// - `selected_node` – which node *within* that line has focus
///
/// This lets you navigate left/right through a chain of single-child nodes
/// that are collapsed onto one row (e.g. `src → main.rs → run()`).
///
/// Expansion paths have the form `[root_index, child_index, …]` where the
/// first element indexes into the forest of roots passed to [`TreeState::flatten`].
#[derive(Debug, Clone, PartialEq)]
pub struct TreeState {
    /// Index into the flat visible-items list of the currently selected line.
    selected_row: usize,
    /// Index of the focused node within that line's single-child chain.
    selected_node: usize,
    /// Set of paths (to the *last* node of a visual line) that are expanded.
    expanded: BTreeSet<Vec<usize>>,
}

impl Default for TreeState {
    fn default() -> Self {
        Self::new()
    }
}

impl TreeState {
    /// Create a new `TreeState` with no selection and nothing expanded.
    pub fn new() -> Self {
        Self {
            selected_row: 0,
            selected_node: 0,
            expanded: BTreeSet::new(),
        }
    }

    // -- Selection ---------------------------------------------------------

    /// Return the row index of the currently selected visible line.
    pub fn selected_row(&self) -> usize {
        self.selected_row
    }

    /// Return the node index within the selected line.
    pub fn selected_node(&self) -> usize {
        self.selected_node
    }

    /// Directly set the selected row (node resets to 0).
    pub fn select_row(&mut self, row: usize) {
        self.selected_row = row;
        self.selected_node = 0;
    }

    /// Set both row and node index.
    pub fn select(&mut self, row: usize, node: usize) {
        self.selected_row = row;
        self.selected_node = node;
    }

    /// Set only the node index within the current row.
    pub fn set_selected_node(&mut self, node: usize) {
        self.selected_node = node;
    }

    /// Move selection down by one row, keeping the same node index clamped
    /// to the number of nodes on the new row.
    pub fn key_down(&mut self, flatten: &[FlattenedItem]) {
        if flatten.is_empty() {
            return;
        }
        let new_row = self.selected_row.saturating_add(1).min(flatten.len() - 1);
        self.selected_row = new_row;
        self.selected_node = self
            .selected_node
            .min(flatten[new_row].num_nodes().saturating_sub(1));
    }

    /// Move selection up by one row, keeping the same node index clamped.
    pub fn key_up(&mut self, flatten: &[FlattenedItem]) {
        if self.selected_row == 0 || flatten.is_empty() {
            return;
        }
        let new_row = self.selected_row.saturating_sub(1);
        self.selected_row = new_row;
        self.selected_node = self
            .selected_node
            .min(flatten[new_row].num_nodes().saturating_sub(1));
    }

    /// Move focus to the next node to the right on the same row.
    /// If already at the last node and it has children, toggle expand/collapse.
    pub fn key_right(&mut self, flatten: &[FlattenedItem]) {
        if let Some(item) = flatten.get(self.selected_row) {
            let last = item.num_nodes().saturating_sub(1);
            if self.selected_node < last {
                self.selected_node += 1;
            } else if item.has_children {
                // Toggle the last node (the one with children).
                self.toggle(&item.path);
            }
        }
    }

    /// Move focus to the previous node to the left on the same row.
    /// If already at the first node and the row is expanded, collapse it.
    /// If at the first node and not expanded, move to the previous row's
    /// last node.
    pub fn key_left(&mut self, flatten: &[FlattenedItem]) {
        if let Some(item) = flatten.get(self.selected_row) {
            if self.selected_node > 0 {
                self.selected_node -= 1;
            } else if item.expanded {
                self.collapse(&item.path);
            } else if self.selected_row > 0 {
                self.selected_row -= 1;
                if let Some(prev) = flatten.get(self.selected_row) {
                    self.selected_node = prev.num_nodes().saturating_sub(1);
                }
            }
        }
    }

    // -- Expansion ---------------------------------------------------------

    /// Return `true` if the visual line at `path` is expanded.
    pub fn is_expanded(&self, path: &[usize]) -> bool {
        self.expanded.contains(path)
    }

    /// Toggle the expanded state for `path`.
    pub fn toggle(&mut self, path: &[usize]) {
        if self.expanded.contains(path) {
            self.expanded.remove(path);
        } else {
            self.expanded.insert(path.to_vec());
        }
    }

    pub fn toggle_selected(&mut self, items: &[FlattenedItem]) {
        if let Some(item) = items.get(self.selected_row) {
            if item.has_children {
                self.toggle(&item.path);
            }
        }
    }

    /// Expand the visual line at `path`.
    pub fn expand(&mut self, path: &[usize]) {
        self.expanded.insert(path.to_vec());
    }

    /// Collapse the visual line at `path`.
    pub fn collapse(&mut self, path: &[usize]) {
        self.expanded.remove(path);
    }

    // -- Flattening --------------------------------------------------------

    /// Flatten a forest of roots into a list of visible lines, taking
    /// expansion state and single-child chaining into account.
    ///
    /// `roots` holds every top-level tree; each root is rendered as its own
    /// depth-0 line.  The returned vector is what should be rendered.
    pub fn flatten<'a>(&self, roots: &'a [TreeNode]) -> Vec<FlattenedItem<'a>> {
        let mut items = Vec::new();
        for (i, root) in roots.iter().enumerate() {
            self.flatten_rec(root, &[i], 0, &mut items);
        }
        items
    }

    fn flatten_rec<'a>(
        &self,
        node: &'a TreeNode,
        prefix: &[usize],
        depth: usize,
        items: &mut Vec<FlattenedItem<'a>>,
    ) {
        // Build the chain of single-child nodes starting from `node`.
        let chain = single_child_chain(node);
        let last = chain.last().copied().unwrap();

        // Path to the *last* node in the chain (the one that might be toggled).
        let last_path: Vec<usize> = prefix
            .iter()
            .copied()
            .chain(vec![0; chain.len().saturating_sub(1)])
            .collect();

        let expanded = self.expanded.contains(&last_path);
        let has_children = !last.children.is_empty();

        items.push(FlattenedItem {
            path: last_path.clone(),
            nodes: chain,
            depth,
            has_children,
            expanded,
        });

        // If expanded, recurse into each child of the last node.
        if expanded {
            for (i, child) in last.children.iter().enumerate() {
                let mut child_prefix = last_path.clone();
                child_prefix.push(i);
                self.flatten_rec(child, &child_prefix, depth + 1, items);
            }
        }
    }

    /// Return the path (to the last node) of the currently selected line,
    /// given the flattened items.
    pub fn selected_path<'a>(&self, items: &'a [FlattenedItem<'a>]) -> Option<&'a [usize]> {
        items
            .get(self.selected_row)
            .map(|item| item.path.as_slice())
    }

    /// Return the path of the node that actually has focus, i.e. the
    /// `selected_node`-th node of the selected line.
    ///
    /// Unlike [`selected_path`](Self::selected_path) this accounts for lines
    /// that merge a chain of single-child nodes into a single row.
    pub fn focused_path(&self, items: &[FlattenedItem]) -> Option<Vec<usize>> {
        let item = items.get(self.selected_row)?;
        Some(node_path_in(item, self.selected_node))
    }

    /// Select the node at `path`, if it is currently visible.
    ///
    /// `path` is a full path to a node (`[root_index, child_index, …]`).
    /// Returns `true` when the path was found and selected.
    pub fn select_path(&mut self, items: &[FlattenedItem], path: &[usize]) -> bool {
        for (row, item) in items.iter().enumerate() {
            for node in 0..item.nodes.len() {
                if node_path_in(item, node) == path {
                    self.selected_row = row;
                    self.selected_node = node;
                    return true;
                }
            }
        }
        false
    }

    /// Expand every line that has children, so the whole forest becomes visible.
    pub fn expand_all(&mut self, roots: &[TreeNode]) {
        loop {
            let items = self.flatten(roots);
            let mut changed = false;
            for item in &items {
                if item.has_children && !item.expanded {
                    self.expand(&item.path);
                    changed = true;
                }
            }
            if !changed {
                return;
            }
        }
    }

    /// Number of leading lines to skip so that the selected line stays visible
    /// in a viewport `height` rows tall.
    ///
    /// The selected line is kept near the middle of the viewport.
    pub fn scroll_offset(&self, items: &[FlattenedItem], height: u16) -> usize {
        let height = height as usize;
        if height == 0 {
            return 0;
        }
        let max = items.len().saturating_sub(height);
        self.selected_row.saturating_sub(height / 2).min(max)
    }

    /// Return a reference to the currently selected [`TreeNode`] within the
    /// flattened view, or `None` if there are no items.
    pub fn selected_node_ref<'a>(&self, items: &'a [FlattenedItem<'a>]) -> Option<&'a TreeNode> {
        items
            .get(self.selected_row)
            .and_then(|item| item.nodes.get(self.selected_node))
            .copied()
    }

    /// Handle default keybindings.
    /// Don't use this method if you need custom keybindings.
    pub fn handle_key(
        &mut self,
        key: ratatui::crossterm::event::KeyEvent,
        flatten: &[FlattenedItem],
    ) {
        use ratatui::crossterm::event::KeyCode;
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => self.key_up(flatten),
            KeyCode::Down | KeyCode::Char('j') => self.key_down(flatten),
            KeyCode::Left | KeyCode::Char('h') => self.key_left(flatten),
            KeyCode::Right | KeyCode::Char('l') => self.key_right(flatten),
            KeyCode::Enter | KeyCode::Char(' ') => self.toggle_selected(flatten),
            _ => {}
        }
    }

    /// Handle a mouse click at `(row, col)`, where `col` is relative to the
    /// tree content area (after block borders).
    ///
    /// Resolves which node of the row was clicked using the same layout math
    /// [`TreeWidget`] renders with, then delegates to
    /// [`handle_click_node`](Self::handle_click_node). Returns `true` if the
    /// state changed.
    pub fn handle_click_at(
        &mut self,
        row: u16,
        col: u16,
        flatten: &[FlattenedItem],
        bc: &BranchChars,
    ) -> bool {
        let Some(item) = flatten.get(row as usize) else {
            return false;
        };
        let node = node_index_at_col(col, &node_columns(item, bc));
        self.handle_click_node(row as usize, node, flatten)
    }

    /// Handle a click that resolved to a specific `(row, node)` position.
    /// Returns `true` if the state changed.
    ///
    /// Focus moves to the clicked node. Expansion is toggled only when the
    /// click lands on the node that already has focus *and* that node is the
    /// last one on the row — the only node whose children the row can own.
    /// Clicking a sibling earlier in a merged chain therefore just moves
    /// focus instead of collapsing the row.
    pub fn handle_click_node(
        &mut self,
        row: usize,
        node: usize,
        flatten: &[FlattenedItem],
    ) -> bool {
        let Some(item) = flatten.get(row) else {
            return false;
        };
        let last = item.nodes.len().saturating_sub(1);
        let node = node.min(last);

        let prev_row = self.selected_row;
        let prev_node = self.selected_node;
        self.selected_row = row;
        self.selected_node = node;

        if prev_row == row && prev_node == node && node == last && item.has_children {
            self.toggle(&item.path);
            return true;
        }

        prev_row != row || prev_node != node
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_tree() -> TreeNode {
        TreeNode::with_children(
            "root",
            vec![
                TreeNode::with_children("a", vec![TreeNode::new("a1")]),
                TreeNode::with_children(
                    "b",
                    vec![
                        TreeNode::with_children("b1", vec![TreeNode::new("b1a")]),
                        TreeNode::new("b2"),
                    ],
                ),
                TreeNode::new("c"),
            ],
        )
    }

    /// A helper for the common single-root test case.
    fn flatten_one<'a>(state: &TreeState, tree: &'a TreeNode) -> Vec<FlattenedItem<'a>> {
        state.flatten(std::slice::from_ref(tree))
    }

    // -- flattening ------------------------------------------------------------

    #[test]
    fn flatten_initially_all_collapsed() {
        let tree = sample_tree();
        let state = TreeState::new();
        let items = flatten_one(&state, &tree);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].display_text(), "root");
        assert_eq!(items[0].path, vec![0]);
    }

    #[test]
    fn flatten_multiple_roots() {
        let tree_a = TreeNode::with_children(
            "A",
            vec![
                TreeNode::with_children("a1", vec![TreeNode::new("a1x")]),
                TreeNode::new("a2"),
            ],
        );
        let tree_b = TreeNode::new("B");

        let mut state = TreeState::new();
        state.expand(&[0]); // expand root A only

        let roots = [tree_a, tree_b];
        let items = state.flatten(&roots);
        assert_eq!(items.len(), 4);
        // 0: A (depth 0, root 0), 1: a1→a1x (depth 1), 2: a2 (depth 1), 3: B
        assert_eq!(items[0].display_text(), "A");
        assert_eq!(items[0].depth, 0);
        assert_eq!(items[0].path, vec![0]);
        assert_eq!(items[1].display_text(), "a1 → a1x");
        assert_eq!(items[1].depth, 1);
        // Path points to the last node in the chain (a1x).
        assert_eq!(items[1].path, vec![0, 0, 0]);
        assert_eq!(items[2].display_text(), "a2");
        assert_eq!(items[2].depth, 1);
        assert_eq!(items[2].path, vec![0, 1]);
        assert_eq!(items[3].display_text(), "B");
        assert_eq!(items[3].depth, 0);
        assert_eq!(items[3].path, vec![1]);
    }

    #[test]
    fn flatten_single_child_chain() {
        // a -> a1 -> a1a -> leaf  (all single children)
        let tree = TreeNode::with_children(
            "a",
            vec![TreeNode::with_children(
                "a1",
                vec![TreeNode::with_children("a1a", vec![TreeNode::new("leaf")])],
            )],
        );
        let mut state = TreeState::new();
        state.expand(&[0]); // expand root "a"

        let items = flatten_one(&state, &tree);
        assert_eq!(items.len(), 1, "single-child chain on one line");
        assert_eq!(items[0].display_text(), "a → a1 → a1a → leaf");
        assert_eq!(items[0].num_nodes(), 4);
    }

    #[test]
    fn flatten_expanded_branch() {
        let tree = sample_tree();
        let mut state = TreeState::new();

        state.expand(&[0]); // root
        state.expand(&[0, 1]); // b
        state.expand(&[0, 1, 0]); // b1

        let items = flatten_one(&state, &tree);
        // 0: root, 1: a→a1, 2: b, 3: b1→b1a, 4: b2, 5: c
        assert_eq!(items.len(), 6);
        assert_eq!(items[0].display_text(), "root");
        assert_eq!(items[1].display_text(), "a → a1");
        assert_eq!(items[2].display_text(), "b");
        assert_eq!(items[3].display_text(), "b1 → b1a");
        assert_eq!(items[4].display_text(), "b2");
        assert_eq!(items[5].display_text(), "c");
    }

    // -- vertical navigation ---------------------------------------------------

    #[test]
    fn up_down_basic() {
        let tree = sample_tree();
        let mut state = TreeState::new();
        state.expand(&[0]);

        let items = flatten_one(&state, &tree); // 4 rows
        assert_eq!(state.selected_row(), 0);
        assert_eq!(state.selected_node(), 0);

        state.key_down(&items);
        assert_eq!(state.selected_row(), 1);
        assert_eq!(state.selected_node(), 0);

        state.key_down(&items);
        assert_eq!(state.selected_row(), 2);

        state.key_down(&items);
        assert_eq!(state.selected_row(), 3);

        state.key_down(&items); // clamped
        assert_eq!(state.selected_row(), 3);

        state.key_up(&items);
        assert_eq!(state.selected_row(), 2);
    }

    #[test]
    fn up_stays_at_top() {
        let tree = sample_tree();
        let mut state = TreeState::new();
        let items = flatten_one(&state, &tree); // 1 row
        state.key_up(&items);
        assert_eq!(state.selected_row(), 0);
    }

    #[test]
    fn navigation_across_roots() {
        // Give each root two children so expansion reveals depth-1 rows.
        let tree_a = TreeNode::with_children("A", vec![TreeNode::new("a1"), TreeNode::new("a2")]);
        let tree_b = TreeNode::with_children("B", vec![TreeNode::new("b1"), TreeNode::new("b2")]);
        let roots = [tree_a, tree_b];

        let mut state = TreeState::new();
        state.expand(&[0]);
        state.expand(&[1]);

        let items = state.flatten(&roots);
        // 0: A, 1: a1, 2: a2, 3: B, 4: b1, 5: b2
        assert_eq!(items.len(), 6);

        state.select(0, 0);
        state.key_down(&items);
        assert_eq!(state.selected_row(), 1); // a1
        state.key_down(&items);
        assert_eq!(state.selected_row(), 2); // a2
        state.key_down(&items);
        assert_eq!(state.selected_row(), 3); // root B
        assert_eq!(items[state.selected_row()].path, vec![1]);
        state.key_down(&items);
        assert_eq!(state.selected_row(), 4); // b1 under root B
        state.key_up(&items);
        assert_eq!(state.selected_row(), 3);
    }

    #[test]
    fn node_index_clamped_when_moving_to_shorter_row() {
        // root has 1 node; a→a1 has 2 nodes; b has 1 node
        let tree = sample_tree();
        let mut state = TreeState::new();
        state.expand(&[0]);
        let items = flatten_one(&state, &tree);

        // move to row 1 (a→a1, 2 nodes) and select node 1
        state.select(1, 1);
        assert_eq!(state.selected_node(), 1);

        // move up to row 0 (root, 1 node) — node clamped to 0
        state.key_up(&items);
        assert_eq!(state.selected_row(), 0);
        assert_eq!(state.selected_node(), 0);
    }

    // -- horizontal navigation ------------------------------------------------

    #[test]
    fn right_moves_through_nodes_then_toggles() {
        let tree = sample_tree();
        let mut state = TreeState::new();
        state.expand(&[0]);
        let items = flatten_one(&state, &tree);

        // row 1 = a→a1 (2 nodes), select node 0
        state.select(1, 0);

        state.key_right(&items); // node 0 → 1
        assert_eq!(state.selected_node(), 1);

        state.key_right(&items); // node 1, no children → no-op
        assert_eq!(state.selected_node(), 1);
    }

    #[test]
    fn right_on_last_node_with_children_toggles() {
        let tree = sample_tree();
        let mut state = TreeState::new();
        state.expand(&[0]);

        // row 2 = "b" (1 node, has children, not expanded)
        state.select(2, 0);
        assert!(!state.is_expanded(&[0, 1]));

        state.key_right(&flatten_one(&state, &tree));
        // last node + has children → toggle
        let items = flatten_one(&state, &tree);
        assert!(state.is_expanded(&[0, 1]));
        // should now see b1→b1a and b2
        assert!(items.len() > 4);
    }

    #[test]
    fn left_moves_back_through_nodes() {
        let tree = sample_tree();
        let mut state = TreeState::new();
        state.expand(&[0]);
        let items = flatten_one(&state, &tree);

        state.select(1, 1); // a→a1, node 1
        assert_eq!(state.selected_node(), 1);

        state.key_left(&items);
        assert_eq!(state.selected_node(), 0);

        state.key_left(&items); // first node, not expanded → previous row's last
        assert_eq!(state.selected_row(), 0);
        assert_eq!(state.selected_node(), 0); // root has 1 node
    }

    #[test]
    fn left_on_first_node_collapses() {
        // root expanded, select root's first (only) node, press left → collapse
        let tree = sample_tree();
        let mut state = TreeState::new();
        state.expand(&[0]);
        let items = flatten_one(&state, &tree);
        assert!(items.len() > 1);

        state.select(0, 0);
        assert!(state.is_expanded(&[0]));
        state.key_left(&items); // first node + expanded → collapse
        let items = flatten_one(&state, &tree);
        assert_eq!(items.len(), 1);
        assert!(!state.is_expanded(&[0]));
    }

    // -- toggle ----------------------------------------------------------------

    #[test]
    fn toggle_expand_collapse() {
        let tree = sample_tree();
        let mut state = TreeState::new();

        let items = flatten_one(&state, &tree);
        assert_eq!(items.len(), 1);

        state.toggle(&[0]);
        let items = flatten_one(&state, &tree);
        assert_eq!(items.len(), 4);

        state.toggle(&[0]);
        let items = flatten_one(&state, &tree);
        assert_eq!(items.len(), 1);
    }

    // -- mouse clicks ----------------------------------------------------------

    #[test]
    fn click_same_row_different_node_moves_focus() {
        // a → a1 → a1a is a single-child chain collapsed onto one row, and the
        // chain ends in a leaf, so this row can never toggle.
        let tree = TreeNode::with_children(
            "a",
            vec![TreeNode::with_children("a1", vec![TreeNode::new("a1a")])],
        );
        let mut state = TreeState::new();
        let items = flatten_one(&state, &tree);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].nodes.len(), 3);
        assert!(items[0].nodes.last().unwrap().is_leaf());

        state.select(0, 0);
        assert_eq!(state.selected_node(), 0);

        // Click the second node on the same row — focus moves.
        assert!(state.handle_click_node(0, 1, &items));
        assert_eq!(state.selected_node(), 1);

        // Click the third node.
        assert!(state.handle_click_node(0, 2, &items));
        assert_eq!(state.selected_node(), 2);

        // Re-clicking the focused leaf node changes nothing.
        assert!(!state.handle_click_node(0, 2, &items));
        assert_eq!(state.selected_node(), 2);
    }

    #[test]
    fn click_focused_last_node_twice_toggles() {
        // root has one child, so "root → a" is one row, and `a` owns b1/b2.
        let tree = TreeNode::with_children(
            "root",
            vec![TreeNode::with_children(
                "a",
                vec![TreeNode::new("b1"), TreeNode::new("b2")],
            )],
        );
        let mut state = TreeState::new();
        state.expand(&[0, 0]); // expand `a`, the last node of the merged row
        let items = flatten_one(&state, &tree);
        assert_eq!(items.len(), 3); // root → a, b1, b2
        assert_eq!(items[0].nodes.len(), 2);

        state.select(0, 1);
        assert!(state.is_expanded(&[0, 0]));

        // Clicking the already-focused last node collapses.
        assert!(state.handle_click_node(0, 1, &items));
        assert!(!state.is_expanded(&[0, 0]));

        // ...and expands again.
        assert!(state.handle_click_node(0, 1, &items));
        assert!(state.is_expanded(&[0, 0]));
    }

    #[test]
    fn click_middle_of_chain_does_not_toggle_last_node() {
        let tree = TreeNode::with_children(
            "root",
            vec![
                TreeNode::with_children(
                    "docs",
                    vec![TreeNode::with_children(
                        "guide",
                        vec![TreeNode::new("a.md"), TreeNode::new("b.md")],
                    )],
                ),
                TreeNode::new("other"),
            ],
        );
        let mut state = TreeState::new();
        state.expand(&[0]); // root
        state.expand(&[0, 0, 0]); // guide — the last node of the "docs → guide" row
        let items = flatten_one(&state, &tree);
        // 0: root, 1: docs → guide, 2: a.md, 3: b.md, 4: other
        assert_eq!(items.len(), 5);
        assert_eq!(items[1].nodes.len(), 2);
        assert!(state.is_expanded(&[0, 0, 0]));

        // Focus "docs", then click "guide": focus moves, children stay visible.
        state.select(1, 0);
        assert!(state.handle_click_node(1, 1, &items));
        assert_eq!(state.selected_node(), 1);
        assert!(state.is_expanded(&[0, 0, 0]));

        // A second click on the now-focused last node does toggle.
        assert!(state.handle_click_node(1, 1, &items));
        assert!(!state.is_expanded(&[0, 0, 0]));
    }

    #[test]
    fn handle_click_at_resolves_column_to_node() {
        use crate::BranchChars;
        let tree = TreeNode::with_children(
            "a",
            vec![TreeNode::with_children("a1", vec![TreeNode::new("a1a")])],
        );
        let mut state = TreeState::new();
        state.expand(&[0]);
        let items = flatten_one(&state, &tree);
        assert_eq!(items.len(), 1);

        // Compute the column offsets for each node.
        let bc = BranchChars::default();
        let cols = crate::node_columns(&items[0], &bc);

        // Click at the start of node 1's text.
        let col1 = cols.start_cols[1];
        let changed = state.handle_click_at(0, col1, &items, &bc);
        assert!(changed);
        assert_eq!(state.selected_node(), 1);

        // Click at the start of node 2's text.
        let col2 = cols.start_cols[2];
        let changed = state.handle_click_at(0, col2, &items, &bc);
        assert!(changed);
        assert_eq!(state.selected_node(), 2);
    }

    // -- handle_key -----------------------------------------------------------

    #[test]
    fn handle_key_routes_correctly() {
        use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let tree = sample_tree();
        let mut state = TreeState::new();
        state.expand(&[0]);

        let make = |code| KeyEvent::new(code, KeyModifiers::NONE);

        let items = flatten_one(&state, &tree);
        assert_eq!(state.selected_row(), 0);

        state.handle_key(make(KeyCode::Down), &items);
        assert_eq!(state.selected_row(), 1);

        state.handle_key(make(KeyCode::Right), &items);
        assert_eq!(state.selected_row(), 1);
        assert_eq!(state.selected_node(), 1);

        state.handle_key(make(KeyCode::Left), &items);
        assert_eq!(state.selected_node(), 0);

        state.handle_key(make(KeyCode::Up), &items);
        assert_eq!(state.selected_row(), 0);

        state.handle_key(make(KeyCode::Enter), &flatten_one(&state, &tree));
        assert!(!state.is_expanded(&[0]));
    }

    // -- focused path / programmatic selection ---------------------------------

    #[test]
    fn focused_path_accounts_for_single_child_chains() {
        // a -> a1 -> a1a, all on one visual line.
        let tree = TreeNode::with_children(
            "a",
            vec![TreeNode::with_children("a1", vec![TreeNode::new("a1a")])],
        );
        let mut state = TreeState::new();
        state.expand(&[0]);
        let items = flatten_one(&state, &tree);
        assert_eq!(items.len(), 1);

        state.select(0, 0);
        assert_eq!(state.focused_path(&items).unwrap(), vec![0]);
        state.select(0, 1);
        assert_eq!(state.focused_path(&items).unwrap(), vec![0, 0]);
        state.select(0, 2);
        assert_eq!(state.focused_path(&items).unwrap(), vec![0, 0, 0]);

        // `selected_path` keeps reporting the end of the chain.
        assert_eq!(state.selected_path(&items).unwrap().to_vec(), vec![0, 0, 0]);
    }

    #[test]
    fn select_path_finds_nodes_inside_a_chain() {
        let tree = TreeNode::with_children(
            "a",
            vec![TreeNode::with_children("a1", vec![TreeNode::new("a1a")])],
        );
        let mut state = TreeState::new();
        state.expand(&[0]);
        let items = flatten_one(&state, &tree);

        assert!(state.select_path(&items, &[0, 0]));
        assert_eq!(state.selected_row(), 0);
        assert_eq!(state.selected_node(), 1);
        assert_eq!(state.focused_path(&items).unwrap(), vec![0, 0]);

        assert!(!state.select_path(&items, &[0, 7]));
    }

    #[test]
    fn select_path_across_multiple_roots() {
        let roots = vec![
            TreeNode::with_children("A", vec![TreeNode::new("a1")]),
            TreeNode::new("B"),
        ];
        let mut state = TreeState::new();
        let items = state.flatten(&roots);
        // "A → a1" collapses into a single line, "B" is the next one.
        assert_eq!(items.len(), 2);

        assert!(state.select_path(&items, &[1]));
        assert_eq!(state.selected_row(), 1);
        assert_eq!(state.focused_path(&items).unwrap(), vec![1]);

        // Nodes inside a collapsed chain are addressable too.
        assert!(state.select_path(&items, &[0, 0]));
        assert_eq!(state.selected_row(), 0);
        assert_eq!(state.selected_node(), 1);
        assert_eq!(state.focused_path(&items).unwrap(), vec![0, 0]);
    }

    // -- scrolling -------------------------------------------------------------

    #[test]
    fn scroll_offset_is_zero_when_everything_fits() {
        let tree = sample_tree();
        let mut state = TreeState::new();
        state.expand_all(std::slice::from_ref(&tree));
        let items = flatten_one(&state, &tree);
        assert_eq!(state.scroll_offset(&items, items.len() as u16 + 5), 0);
        assert_eq!(state.scroll_offset(&items, 0), 0);
    }

    #[test]
    fn scroll_offset_keeps_selection_visible() {
        let tree = sample_tree();
        let mut state = TreeState::new();
        state.expand_all(std::slice::from_ref(&tree));
        let items = flatten_one(&state, &tree);
        assert!(items.len() > 3);

        state.select_row(items.len() - 1);
        let offset = state.scroll_offset(&items, 3);
        assert!(offset + 3 >= items.len(), "last line must be visible");
        assert!(offset <= items.len() - 3, "offset must stay in range");

        // Selecting the first line scrolls back to the top.
        state.select_row(0);
        assert_eq!(state.scroll_offset(&items, 3), 0);
    }

    // -- expand all ------------------------------------------------------------

    #[test]
    fn expand_all_reveals_the_whole_forest() {
        let tree = sample_tree();
        let mut state = TreeState::new();
        assert_eq!(state.flatten(std::slice::from_ref(&tree)).len(), 1);

        state.expand_all(std::slice::from_ref(&tree));
        let items = state.flatten(std::slice::from_ref(&tree));
        assert_eq!(items.len(), 6);
        assert!(items.iter().all(|item| !item.has_children || item.expanded));
    }
}
