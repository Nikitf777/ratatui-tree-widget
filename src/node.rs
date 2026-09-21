use std::borrow::Cow;
use std::fmt;

use ratatui::style::Style;

/// A single node in the tree.
///
/// Each node holds some text and an ordered list of children.
#[derive(Debug, Clone)]
pub struct TreeNode {
    pub text: String,
    pub children: Vec<TreeNode>,
    /// Style used for this node's text when it is not the focused node.
    /// `None` falls back to the widget's base style.
    pub style: Option<Style>,
    /// Separator drawn immediately to the left of this node's text, when the
    /// node has a preceding node on the same line.
    pub separator_left: Option<String>,
    /// Separator drawn immediately to the right of this node's text, when the
    /// node has a following node on the same line.
    pub separator_right: Option<String>,
}

impl TreeNode {
    /// Create a leaf node with the given text.
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            children: Vec::new(),
            style: None,
            separator_left: None,
            separator_right: None,
        }
    }

    /// Create a node with children.
    pub fn with_children(text: impl Into<String>, children: Vec<TreeNode>) -> Self {
        Self {
            text: text.into(),
            children,
            style: None,
            separator_left: None,
            separator_right: None,
        }
    }

    /// Set the style used for this node's text.
    pub fn styled(mut self, style: Style) -> Self {
        self.style = Some(style);
        self
    }

    /// Set the separator drawn before this node's text.
    pub fn left_separator(mut self, separator: impl Into<String>) -> Self {
        self.separator_left = Some(separator.into());
        self
    }

    /// Set the separator drawn after this node's text.
    pub fn right_separator(mut self, separator: impl Into<String>) -> Self {
        self.separator_right = Some(separator.into());
        self
    }

    /// Add a child node.
    pub fn add_child(&mut self, child: TreeNode) {
        self.children.push(child);
    }

    /// Return `true` if this node has no children.
    pub fn is_leaf(&self) -> bool {
        self.children.is_empty()
    }

    /// Walk the tree following `path` (a sequence of child indices) and return
    /// a reference to the descendant, or `None` if the path is invalid.
    pub fn get_at_path(&self, path: &[usize]) -> Option<&TreeNode> {
        let mut node = self;
        for &idx in path {
            node = node.children.get(idx)?;
        }
        Some(node)
    }
}

impl fmt::Display for TreeNode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.text)
    }
}

/// Separator rendered between two adjacent nodes sharing a line.
///
/// The left node's [`separator_right`](TreeNode::separator_right) and the right
/// node's [`separator_left`](TreeNode::separator_left) are concatenated, so a
/// caller can attach the separator to whichever side reads better. When neither
/// node declares one, `default` (the global separator) is used instead.
///
/// An empty separator is a value like any other: `Some("")` suppresses the
/// global separator rather than falling back to it.
pub fn separator_between<'a>(
    left: &'a TreeNode,
    right: &'a TreeNode,
    default: &'a str,
) -> Cow<'a, str> {
    match (&left.separator_right, &right.separator_left) {
        (None, None) => Cow::Borrowed(default),
        (right_sep, left_sep) => {
            let mut separator = String::new();
            if let Some(value) = right_sep {
                separator.push_str(value);
            }
            if let Some(value) = left_sep {
                separator.push_str(value);
            }
            Cow::Owned(separator)
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers used by the flattening logic
// ---------------------------------------------------------------------------

/// Internal helper: collect the "chain" of single-child nodes starting from
/// `node`.  The returned slice always contains at least `node` itself; it
/// continues as long as the current node has exactly one child.
pub(crate) fn single_child_chain(node: &TreeNode) -> Vec<&TreeNode> {
    let mut chain = vec![node];
    loop {
        let tail = chain.last().copied().unwrap();
        if tail.children.len() == 1 {
            chain.push(&tail.children[0]);
        } else {
            break;
        }
    }
    chain
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_node_is_leaf() {
        let n = TreeNode::new("foo");
        assert!(n.is_leaf());
    }

    #[test]
    fn node_with_children_is_not_leaf() {
        let n = TreeNode::with_children("root", vec![TreeNode::new("a")]);
        assert!(!n.is_leaf());
    }

    #[test]
    fn get_at_path() {
        let tree = TreeNode::with_children(
            "root",
            vec![
                TreeNode::with_children("a", vec![TreeNode::new("a1")]),
                TreeNode::new("b"),
            ],
        );
        assert_eq!(tree.get_at_path(&[]).unwrap().text, "root");
        assert_eq!(tree.get_at_path(&[0]).unwrap().text, "a");
        assert_eq!(tree.get_at_path(&[0, 0]).unwrap().text, "a1");
        assert_eq!(tree.get_at_path(&[1]).unwrap().text, "b");
        assert!(tree.get_at_path(&[0, 1]).is_none());
        assert!(tree.get_at_path(&[2]).is_none());
    }

    #[test]
    fn single_child_chain_leaf() {
        assert_eq!(single_child_chain(&TreeNode::new("x")).len(), 1);
    }

    #[test]
    fn single_child_chain_multi() {
        // a -> b -> c  (c has two children)
        let c = TreeNode::with_children("c", vec![TreeNode::new("c1"), TreeNode::new("c2")]);
        let b = TreeNode::with_children("b", vec![c]);
        let a = TreeNode::with_children("a", vec![b]);
        let chain = single_child_chain(&a);
        assert_eq!(chain.len(), 3);
        assert_eq!(chain[0].text, "a");
        assert_eq!(chain[1].text, "b");
        assert_eq!(chain[2].text, "c");
    }

    #[test]
    fn single_child_chain_branches() {
        // a -> b (b has two children) – chain should stop at a
        let b = TreeNode::with_children("b", vec![TreeNode::new("b1"), TreeNode::new("b2")]);
        let a = TreeNode::with_children("a", vec![b]);
        let chain = single_child_chain(&a);
        assert_eq!(chain.len(), 2);
    }

    #[test]
    fn separator_defaults_to_the_global_one() {
        let left = TreeNode::new("a");
        let right = TreeNode::new("b");
        assert_eq!(separator_between(&left, &right, " -> "), " -> ");
    }

    #[test]
    fn separator_combines_both_sides() {
        let left = TreeNode::new("a").right_separator("/");
        let right = TreeNode::new("b").left_separator("/");
        // Both sides are concatenated: the caller asked for both.
        assert_eq!(separator_between(&left, &right, " -> "), "//");
    }

    #[test]
    fn separator_uses_whichever_side_is_set() {
        let left = TreeNode::new("a");
        let right = TreeNode::new("b").left_separator("/");
        assert_eq!(separator_between(&left, &right, " -> "), "/");

        let left = TreeNode::new("a").right_separator(" :: ");
        let right = TreeNode::new("b");
        assert_eq!(separator_between(&left, &right, " -> "), " :: ");
    }

    #[test]
    fn empty_separator_suppresses_the_global_one() {
        let left = TreeNode::new("a");
        let right = TreeNode::new("b").left_separator("");
        assert_eq!(separator_between(&left, &right, " -> "), "");
    }
}
