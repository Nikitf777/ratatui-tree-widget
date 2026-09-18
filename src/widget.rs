use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, StatefulWidget, Widget},
};

use crate::state::FlattenedItem;
use crate::TreeNode;
use crate::TreeState;

// ---------------------------------------------------------------------------
// TreeWidget
// ---------------------------------------------------------------------------

/// A Ratatui widget that renders a tree of [`TreeNode`]s with expand/collapse
/// and keyboard / mouse interaction.
///
/// The widget borrows the tree root and reads the current [`TreeState`] to
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
///     TreeWidget::new(&tree).highlight_style(my_style),
///     area,
///     &mut state,
/// );
/// ```
pub struct TreeWidget<'a> {
    /// Reference to the root of the tree to render.
    root: &'a TreeNode,
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
    /// Create a new tree widget that renders `root`.
    pub fn new(root: &'a TreeNode) -> Self {
        Self {
            root,
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
                base
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

        for (i, item) in items.iter().enumerate() {
            if i as u16 >= area.height {
                break;
            }

            let is_row_selected = i == selected_row;
            let row_y = area.y + i as u16;

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

        let items = state.flatten(self.root);
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
