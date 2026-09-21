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
}

impl TreeNode {
    /// Create a leaf node with the given text.
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            children: Vec::new(),
            style: None,
        }
    }

    /// Create a node with children.
    pub fn with_children(text: impl Into<String>, children: Vec<TreeNode>) -> Self {
        Self {
            text: text.into(),
            children,
            style: None,
        }
    }

    /// Set the style used for this node's text.
    pub fn styled(mut self, style: Style) -> Self {
        self.style = Some(style);
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
}
