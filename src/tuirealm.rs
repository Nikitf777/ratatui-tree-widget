//! Optional [`tuirealm`](https://docs.rs/tuirealm) integration.
//!
//! This module is compiled only when the `tuirealm` cargo feature is enabled.
//! It provides [`TreeRealm`], a [`tuirealm::component::Component`] wrapper
//! around the base [`TreeWidget`], so the tree can be mounted inside a
//! tui-realm [`View`](tuirealm::view::View).

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::Block;
use ratatui::Frame;
use tuirealm::command::{Cmd, CmdResult, Direction, Position};
use tuirealm::component::Component;
use tuirealm::props::{AttrValue, Attribute, LineStatic, Props, QueryResult, Title};
use tuirealm::state::{State, StateValue};

use crate::{node_columns, node_index_at_col, TreeNode, TreeState, TreeWidget};

/// A tui-realm [`Component`] wrapper around [`TreeWidget`].
///
/// The component owns the tree data (`roots`) and the [`TreeState`], and
/// renders a [`TreeWidget`] into a bordered block when asked to.
///
/// # Supported attributes
///
/// | Attribute | `AttrValue` | Effect |
/// |---|---|---|
/// | `Attribute::Display` | `Flag(false)` | Renders nothing |
/// | `Attribute::Focus` | `Flag` | Set automatically by the `View`; switches between focused / unfocused highlight styles |
/// | `Attribute::Title` | `Title` | Bordered-block title |
/// | `Attribute::Style` | `Style` | Base style of every line |
/// | `Attribute::HighlightStyle` | `Style` | Selection highlight when focused |
/// | `Attribute::HighlightStyleUnfocused` | `Style` | Selection highlight when blurred |
/// | `Attribute::Scroll` | `Flag(false)` | Disables navigation commands |
///
/// # Supported commands
///
/// - [`Cmd::Move`] with [`Direction::Up`] / `Down` / `Left` / `Right`
/// - [`Cmd::Toggle`] — expand/collapse the selected row
/// - [`Cmd::Custom`] "expand" / "collapse" — explicit expand / collapse
/// - [`Cmd::GoTo`] with [`Position::Begin`] / `End` / `At(n)`
///
/// # State
///
/// [`TreeRealm::state`] returns [`State::Vec`] of [`StateValue::Usize`]
/// containing the path of the selected line (`[root_index, child_index, …]`).
pub struct TreeRealm {
    roots: Vec<TreeNode>,
    state: TreeState,
    props: Props,
}

impl TreeRealm {
    /// Create a new tree component holding the given roots.
    pub fn new(roots: Vec<TreeNode>) -> Self {
        Self {
            roots,
            state: TreeState::new(),
            props: Props::default(),
        }
    }

    /// The roots of the forest currently rendered by this component.
    pub fn roots(&self) -> &[TreeNode] {
        &self.roots
    }

    /// Replace the forest rendered by this component.
    pub fn set_roots(&mut self, roots: Vec<TreeNode>) {
        self.roots = roots;
    }

    /// Access the underlying [`TreeState`].
    pub fn tree_state(&self) -> &TreeState {
        &self.state
    }

    /// Mutably access the underlying [`TreeState`].
    pub fn tree_state_mut(&mut self) -> &mut TreeState {
        &mut self.state
    }

    /// Expand the visual line at `path` (e.g. `&[0]` expands the first root).
    pub fn expand(&mut self, path: &[usize]) {
        self.state.expand(path);
    }

    /// Display text of the currently selected row (single-child chains are
    /// joined with ` → `), or `None` if there are no rows.
    pub fn selected_text(&self) -> Option<String> {
        let items = self.state.flatten(&self.roots);
        items
            .get(self.state.selected_row())
            .map(|item| item.display_text())
    }

    // -- Builder helpers ---------------------------------------------------

    /// Set the block title of the tree (focused style is a bordered block).
    pub fn title(mut self, title: impl Into<LineStatic>) -> Self {
        let value = AttrValue::Title(Title::from(title));
        self.attr(Attribute::Title, value);
        self
    }

    /// Set the base style of every line.
    pub fn style(mut self, style: Style) -> Self {
        self.attr(Attribute::Style, AttrValue::Style(style));
        self
    }

    /// Set the highlight style applied to the focused node while focused.
    pub fn highlight_style(mut self, style: Style) -> Self {
        self.attr(Attribute::HighlightStyle, AttrValue::Style(style));
        self
    }

    /// Set the highlight style applied to the focused node while blurred.
    pub fn highlight_style_unfocused(mut self, style: Style) -> Self {
        self.attr(Attribute::HighlightStyleUnfocused, AttrValue::Style(style));
        self
    }

    /// Enable / disable keyboard navigation (defaults to enabled).
    pub fn interactive(mut self, flag: bool) -> Self {
        self.attr(Attribute::Scroll, AttrValue::Flag(flag));
        self
    }

    // -- Mouse -------------------------------------------------------------

    /// Handle a mouse click given terminal coordinates relative to the tree
    /// **content** area (i.e. after the block border has been subtracted).
    ///
    /// Returns [`CmdResult::Changed`] when the selection or expansion state
    /// changed.
    pub fn on_click(&mut self, col: u16, row: u16) -> CmdResult {
        let items = self.state.flatten(&self.roots);
        let changed = self.state.handle_click(row, &items);

        // Focus the node whose text span contains the clicked column.
        if let Some(item) = items.get(self.state.selected_row()) {
            let columns = node_columns(item, &Default::default());
            let node = node_index_at_col(col, &columns);
            self.state.set_selected_node(node);
        }

        if changed {
            CmdResult::Changed(self.state())
        } else {
            CmdResult::NoChange
        }
    }
}

impl Component for TreeRealm {
    fn view(&mut self, frame: &mut Frame, area: Rect) {
        // Check if visible
        if matches!(
            self.props.get(Attribute::Display),
            Some(AttrValue::Flag(false))
        ) {
            return;
        }

        let focused = self
            .props
            .get(Attribute::Focus)
            .and_then(AttrValue::as_flag)
            .unwrap_or(true);

        // Collect styling from props.
        let base = self
            .props
            .get(Attribute::Style)
            .and_then(AttrValue::as_style)
            .unwrap_or_default();

        let highlight = if focused {
            self.props
                .get(Attribute::HighlightStyle)
                .and_then(AttrValue::as_style)
                .unwrap_or_else(|| {
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::LightGreen)
                        .add_modifier(Modifier::BOLD)
                })
        } else {
            self.props
                .get(Attribute::HighlightStyleUnfocused)
                .and_then(AttrValue::as_style)
                .unwrap_or_else(|| Style::default().fg(Color::DarkGray))
        };

        let mut widget = TreeWidget::new(&self.roots)
            .style(base)
            .highlight_style(highlight);

        if let Some(title) = self
            .props
            .get(Attribute::Title)
            .and_then(AttrValue::as_title)
        {
            widget = widget.block(Block::bordered().title(title.content.clone()));
        }

        frame.render_stateful_widget(widget, area, &mut self.state);
    }

    fn query<'a>(&'a self, attr: Attribute) -> Option<QueryResult<'a>> {
        self.props.get_for_query(attr)
    }

    fn attr(&mut self, attr: Attribute, value: AttrValue) {
        self.props.set(attr, value);
    }

    fn state(&self) -> State {
        let items = self.state.flatten(&self.roots);
        match self.state.selected_path(&items) {
            Some(path) => State::Vec(path.iter().map(|&index| StateValue::Usize(index)).collect()),
            None => State::None,
        }
    }

    fn perform(&mut self, cmd: Cmd) -> CmdResult {
        // Whether navigation is enabled.
        let interactive = self
            .props
            .get(Attribute::Scroll)
            .and_then(AttrValue::as_flag)
            .unwrap_or(true);
        if !interactive {
            return CmdResult::Invalid(cmd);
        }

        let before = self.state.clone();
        let items = self.state.flatten(&self.roots);

        match cmd {
            Cmd::Move(Direction::Up) => self.state.key_up(&items),
            Cmd::Move(Direction::Down) => self.state.key_down(&items),
            Cmd::Move(Direction::Left) => self.state.key_left(&items),
            Cmd::Move(Direction::Right) => self.state.key_right(&items),
            Cmd::Toggle => {
                if let Some(item) = items.get(self.state.selected_row()) {
                    if item.has_children {
                        self.state.toggle(&item.path);
                    }
                }
            }
            Cmd::Custom("expand") => {
                if let Some(item) = items.get(self.state.selected_row()) {
                    self.state.expand(&item.path);
                }
            }
            Cmd::Custom("collapse") => {
                if let Some(item) = items.get(self.state.selected_row()) {
                    self.state.collapse(&item.path);
                }
            }
            Cmd::GoTo(Position::Begin) => self.state.select_row(0),
            Cmd::GoTo(Position::End) => {
                if !items.is_empty() {
                    self.state.select_row(items.len() - 1);
                }
            }
            Cmd::GoTo(Position::At(index)) => {
                let max = items.len().saturating_sub(1);
                self.state.select_row(index.min(max));
            }
            _ => return CmdResult::Invalid(cmd),
        }

        if self.state != before {
            CmdResult::Changed(self.state())
        } else {
            CmdResult::NoChange
        }
    }
}
