use std::collections::BTreeSet;

use crate::node::single_child_chain;
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
#[derive(Debug, Clone)]
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

    /// Return a reference to the currently selected [`TreeNode`] within the
    /// flattened view, or `None` if there are no items.
    pub fn selected_node_ref<'a>(&self, items: &'a [FlattenedItem<'a>]) -> Option<&'a TreeNode> {
        items
            .get(self.selected_row)
            .and_then(|item| item.nodes.get(self.selected_node))
            .copied()
    }

    /// Handle keyboard input.
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
            KeyCode::Enter | KeyCode::Char(' ') => {
                if let Some(item) = flatten.get(self.selected_row) {
                    if item.has_children {
                        self.toggle(&item.path);
                    }
                }
            }
            _ => {}
        }
    }

    /// Handle a mouse click at the given row (relative to the tree content
    /// area).  Returns `true` if the state changed.
    ///
    /// Clicking a new row selects its first node.  Clicking the already
    /// selected row toggles expand/collapse if the last node has children.
    pub fn handle_click(&mut self, row: u16, flatten: &[FlattenedItem]) -> bool {
        if row as usize >= flatten.len() {
            return false;
        }
        let prev_row = self.selected_row;
        self.selected_row = row as usize;
        self.selected_node = 0;

        // Toggle expand/collapse if clicking the same row.
        if prev_row == self.selected_row {
            if let Some(item) = flatten.get(self.selected_row) {
                if item.has_children {
                    self.toggle(&item.path);
                    return true;
                }
            }
        }
        prev_row != self.selected_row
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
}
