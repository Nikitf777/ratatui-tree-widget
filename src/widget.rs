use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, StatefulWidget, Widget},
};
use unicode_width::UnicodeWidthStr;

use crate::state::FlattenedItem;
use crate::TreeNode;
use crate::TreeState;

// ---------------------------------------------------------------------------
// TreeWidget
// ---------------------------------------------------------------------------

/// A Ratatui widget that renders a forest of [`TreeNode`]s with expand/collapse
/// and keyboard / mouse interaction.
///
/// The widget borrows the tree roots and reads the current [`TreeState`] to
/// determine selection and expansion.  When a row contains multiple nodes (a
/// single-child chain), the highlight is applied **only** to the focused node
/// rather than the entire row.
///
/// # Example
///
/// ```ignore
/// let tree = TreeNode::with_children("root", vec![TreeNode::new("leaf")]);
/// let mut state = TreeState::new();
///
/// f.render_stateful_widget(
///     TreeWidget::new(&[tree]).highlight_style(my_style),
///     area,
///     &mut state,
/// );
/// ```
pub struct TreeWidget<'a> {
    /// Reference to the roots of the trees to render.
    roots: &'a [TreeNode],
    /// Optional block drawn around the tree.
    block: Option<Block<'a>>,
    /// Style for every non-selected element.
    style: Style,
    /// Style applied only to the focused node text.
    highlight_style: Style,
    /// Style for expand/collapse glyphs.
    glyph_style: Option<Style>,
    /// Characters used for tree branches.
    branch_chars: BranchChars,
}

/// Glyphs used to draw tree structure.
#[derive(Debug, Clone)]
pub struct BranchChars {
    pub indent: &'static str,
    pub middle: &'static str,
    pub last: &'static str,
    pub through: &'static str,
    pub collapsed: &'static str,
    pub expanded: &'static str,
    pub leaf: &'static str,
    pub separator: &'static str,
}

impl Default for BranchChars {
    fn default() -> Self {
        Self {
            indent: "  ",
            middle: "├─ ",
            last: "└─ ",
            through: "│ ",
            collapsed: "▶ ",
            expanded: "▼ ",
            leaf: "  ",
            separator: " → ",
        }
    }
}

impl<'a> TreeWidget<'a> {
    /// Create a new tree widget that renders `roots`.
    ///
    /// Each element of `roots` is a top-level tree rendered as its own
    /// depth-0 line.  Pass `std::slice::from_ref(&tree)` for a single tree.
    pub fn new(roots: &'a [TreeNode]) -> Self {
        Self {
            roots,
            block: None,
            style: Style::default(),
            highlight_style: Style::default()
                .fg(Color::Black)
                .bg(Color::LightGreen)
                .add_modifier(Modifier::BOLD),
            glyph_style: None,
            branch_chars: BranchChars::default(),
        }
    }

    /// Wrap the tree in a [`Block`].
    pub fn block(mut self, block: Block<'a>) -> Self {
        self.block = Some(block);
        self
    }

    /// Set the base style for all non-highlighted text.
    pub fn style(mut self, style: Style) -> Self {
        self.style = style;
        self
    }

    /// Set the style used for the focused node's text.
    pub fn highlight_style(mut self, style: Style) -> Self {
        self.highlight_style = style;
        self
    }

    /// Set the style for expand/collapse glyphs.
    pub fn glyph_style(mut self, style: Style) -> Self {
        self.glyph_style = Some(style);
        self
    }

    /// Set custom branch characters.
    pub fn branch_chars(mut self, chars: BranchChars) -> Self {
        self.branch_chars = chars;
        self
    }

    // -- Rendering ---------------------------------------------------------

    /// Build the styled spans for a single row.
    fn build_row_spans(
        &self,
        item: &FlattenedItem,
        is_row_selected: bool,
        selected_node: usize,
        items: &[FlattenedItem],
        row_idx: usize,
    ) -> Vec<Span<'static>> {
        let bc = &self.branch_chars;
        let base = self.style;
        let mut spans: Vec<Span<'static>> = Vec::new();

        // ---- Indentation and branch vertical lines ----
        for d in 0..item.depth {
            let has_sibling_below = items[row_idx..].iter().any(|it| it.depth == d);
            let text = if has_sibling_below {
                bc.through
            } else {
                bc.indent
            };
            spans.push(Span::styled(text.to_string(), base));
        }

        // ---- Branch connector ----
        if item.depth > 0 {
            let has_sibling_below = items[row_idx..]
                .iter()
                .skip(1)
                .any(|it| it.depth == item.depth);
            let text = if has_sibling_below {
                bc.middle
            } else {
                bc.last
            };
            spans.push(Span::styled(text.to_string(), base));
        }

        // ---- Expand / collapse glyph ----
        let glyph_text = if item.has_children {
            if item.expanded {
                bc.expanded
            } else {
                bc.collapsed
            }
        } else {
            bc.leaf
        };
        let g_style = self.glyph_style.unwrap_or_else(|| {
            if item.has_children {
                Style::default().fg(Color::Yellow)
            } else {
                base
            }
        });
        spans.push(Span::styled(glyph_text.to_string(), g_style));

        // ---- Nodes (each gets its own style) ----
        for (j, node) in item.nodes.iter().enumerate() {
            // Separator between nodes
            if j > 0 {
                spans.push(Span::styled(bc.separator.to_string(), base));
            }

            let is_focused = is_row_selected && j == selected_node;
            let node_style = if is_focused {
                self.highlight_style
            } else {
                node.style.unwrap_or(base)
            };
            spans.push(Span::styled(node.text.clone(), node_style));
        }

        spans
    }

    /// Render the tree into an area of the buffer.
    fn render_inner(
        &self,
        area: Rect,
        buf: &mut Buffer,
        items: &[FlattenedItem],
        state: &TreeState,
    ) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let selected_row = state.selected_row();
        let selected_node = state.selected_node();
        // Scroll so the selected line stays visible; the widget only owns the
        // viewport, the offset is derived from the state.
        let offset = state.scroll_offset(items, area.height);

        for (i, item) in items.iter().enumerate().skip(offset) {
            let row = i - offset;
            if row as u16 >= area.height {
                break;
            }

            let is_row_selected = i == selected_row;
            let row_y = area.y + row as u16;

            let spans = self.build_row_spans(item, is_row_selected, selected_node, items, i);

            let line = Line::from(spans);
            buf.set_line(area.x, row_y, &line, area.width);
        }
    }
}

impl StatefulWidget for TreeWidget<'_> {
    type State = TreeState;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        let inner = if let Some(block) = &self.block {
            let inner_area = block.inner(area);
            block.render(area, buf);
            inner_area
        } else {
            area
        };

        let items = state.flatten(self.roots);
        self.render_inner(inner, buf, &items, state);
    }
}

impl Widget for TreeWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if let Some(block) = self.block {
            block.render(area, buf);
        }
    }
}

/// The start column of each node's text within a rendered row, relative to the
/// tree content area (after block borders).
///
/// This is computed by [`node_columns`] and used by [`node_index_at_col`].
#[derive(Debug, Clone)]
pub struct NodeColumns {
    /// `start_cols[i]` is the column where the text of `nodes[i]` begins.
    pub start_cols: Vec<u16>,
}

/// Compute the start column of each node's text within a rendered row.
///
/// Mirrors the prefix layout produced by [`TreeWidget`]:
///
/// ```text
/// [indent…][branch][glyph] node0 [sep] node1 [sep] node2 …
/// ```
pub fn node_columns(item: &FlattenedItem, bc: &BranchChars) -> NodeColumns {
    let mut x: u16 = 0;

    // Indentation
    x += (item.depth as u16) * UnicodeWidthStr::width(bc.indent) as u16;

    // Branch connector
    if item.depth > 0 {
        x += UnicodeWidthStr::width(bc.middle) as u16;
    }

    // Glyph
    x += if item.has_children {
        UnicodeWidthStr::width(bc.collapsed) as u16
    } else {
        UnicodeWidthStr::width(bc.leaf) as u16
    };

    let mut start_cols = Vec::with_capacity(item.nodes.len());

    for (j, node) in item.nodes.iter().enumerate() {
        if j > 0 {
            x += UnicodeWidthStr::width(bc.separator) as u16;
        }
        start_cols.push(x);
        x += UnicodeWidthStr::width(node.text.as_str()) as u16;
    }

    NodeColumns { start_cols }
}

/// Determine which node in a flattened row is at the given column offset.
///
/// `col` is the 0-based column relative to the **start of the tree content**
/// (after block borders have been subtracted).
///
/// Returns the index into [`FlattenedItem::nodes`] whose text span contains
/// `col`, or the last node if `col` falls past the end.
///
/// Use [`node_columns`] to obtain the [`NodeColumns`] argument.
pub fn node_index_at_col(col: u16, nodecols: &NodeColumns) -> usize {
    // Walk backwards – find the last node whose text starts at or before `col`.
    // If `col` falls before the very first node, return 0.
    for (idx, &start) in nodecols.start_cols.iter().enumerate().rev() {
        if col >= start {
            return idx;
        }
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TreeNode;

    #[test]
    fn node_columns_single_node() {
        let node = TreeNode::new("hello");
        let item = FlattenedItem {
            path: vec![],
            nodes: vec![&node],
            depth: 0,
            has_children: false,
            expanded: false,
        };
        let bc = BranchChars::default();
        let nc = node_columns(&item, &bc);
        // depth 0, no branch, leaf glyph "  " → x starts at 2.
        assert_eq!(nc.start_cols, vec![2]);
    }

    #[test]
    fn node_columns_multi_node() {
        let a = TreeNode::new("a");
        let b = TreeNode::new("bb");
        let c = TreeNode::new("ccc");
        let item = FlattenedItem {
            path: vec![],
            nodes: vec![&a, &b, &c],
            depth: 0,
            has_children: false,
            expanded: false,
        };
        let bc = BranchChars::default();
        let nc = node_columns(&item, &bc);
        // leaf glyph "  " = 2 cols
        // "a" starts at col 2, "bb" at 2+1+3=6, "ccc" at 6+2+3=11
        assert_eq!(nc.start_cols, vec![2, 6, 11]);
    }

    #[test]
    fn node_columns_with_depth() {
        let node = TreeNode::new("x");
        let item = FlattenedItem {
            path: vec![],
            nodes: vec![&node],
            depth: 2,
            has_children: true,
            expanded: false,
        };
        let bc = BranchChars::default();
        let nc = node_columns(&item, &bc);
        // indent "  " per depth = 2*2=4, branch "├─ "=3, glyph "▶ "=2 → total 9
        assert_eq!(nc.start_cols, vec![9]);
    }

    #[test]
    fn node_index_at_col_basic() {
        let a = TreeNode::new("a");
        let b = TreeNode::new("bb");
        let item = FlattenedItem {
            path: vec![],
            nodes: vec![&a, &b],
            depth: 0,
            has_children: false,
            expanded: false,
        };
        let bc = BranchChars::default();
        let nc = node_columns(&item, &bc);
        // leaf "  "=2, "a" at 2, sep " → "=3, "bb" at 6
        assert_eq!(node_index_at_col(0, &nc), 0);
        assert_eq!(node_index_at_col(2, &nc), 0); // "a"
        assert_eq!(node_index_at_col(3, &nc), 0); // still "a"
        assert_eq!(node_index_at_col(5, &nc), 0); // still "a" (col 5 is last col of "a)
        assert_eq!(node_index_at_col(6, &nc), 1); // "bb"
        assert_eq!(node_index_at_col(7, &nc), 1); // "bb"
    }
}
