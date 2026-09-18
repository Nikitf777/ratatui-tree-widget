use std::io;

use crossterm::event::{self, Event, KeyCode, KeyEventKind, MouseEvent, MouseEventKind};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use crossterm::ExecutableCommand;
use ratatui::layout::{Alignment, Constraint, Layout};
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::Line;
use ratatui::widgets::{Block, Paragraph};
use ratatui::Frame;
use ratatui_tree_widget::{TreeNode, TreeState, TreeWidget};

fn main() -> io::Result<()> {
    // ---- Build a sample tree ------------------------------------------------
    let tree = build_sample_tree();

    // ---- Terminal setup -----------------------------------------------------
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    stdout.execute(EnterAlternateScreen)?;
    stdout.execute(crossterm::event::EnableMouseCapture)?;
    let mut terminal = ratatui::init();
    let mut state = TreeState::new();

    // Expand root by default
    state.expand(&[]);

    // ---- Main loop ----------------------------------------------------------
    loop {
        terminal.draw(|f| draw(f, &tree, &mut state))?;

        match event::read()? {
            Event::Key(key) if key.kind == KeyEventKind::Press => match key.code {
                KeyCode::Esc | KeyCode::Char('q') => break,
                _ => {
                    let items = state.flatten(&tree);
                    state.handle_key(key, &items);
                }
            },
            Event::Mouse(mouse) => {
                handle_mouse(mouse, &tree, &mut state);
            }
            _ => {}
        }
    }

    // ---- Cleanup ------------------------------------------------------------
    ratatui::restore();
    stdout.execute(crossterm::event::DisableMouseCapture)?;
    disable_raw_mode()?;
    stdout.execute(LeaveAlternateScreen)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Drawing
// ---------------------------------------------------------------------------

fn draw(f: &mut Frame, tree: &TreeNode, state: &mut TreeState) {
    let area = f.area();

    let [title_area, tree_area, help_area] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(3),
    ])
    .areas(area);

    // Title
    f.render_widget(
        Line::from(" Tree Widget Demo ")
            .style(Style::default().bold().fg(Color::Cyan))
            .alignment(Alignment::Center),
        title_area,
    );

    // The tree
    let tree_widget = TreeWidget::new(tree)
        .block(
            Block::bordered()
                .title(" Tree ")
                .title_alignment(Alignment::Left)
                .border_style(Style::default().fg(Color::DarkGray)),
        )
        .highlight_style(
            Style::default()
                .fg(Color::Black)
                .bg(Color::LightGreen)
                .add_modifier(Modifier::BOLD),
        )
        .glyph_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )
        .style(Style::default().fg(Color::White));

    f.render_stateful_widget(tree_widget, tree_area, state);

    // Help text
    f.render_widget(
        Paragraph::new(
            " ↑/↓ or j/k  move between rows  |  ←/h →/l  move between nodes\n\
             Enter/Space  expand/collapse  |  Click to toggle  |  q/Esc  quit",
        )
        .style(Style::default().fg(Color::DarkGray))
        .alignment(Alignment::Center),
        help_area,
    );
}

// ---------------------------------------------------------------------------
// Mouse handling
// ---------------------------------------------------------------------------

fn handle_mouse(mouse: MouseEvent, tree: &TreeNode, state: &mut TreeState) {
    if !matches!(mouse.kind, MouseEventKind::Down(_)) {
        return;
    }

    // The tree widget is laid out as:
    //   row 0: title
    //   row 1+: tree widget with bordered block
    // The block's left border occupies column 0, content starts at column 1.
    let tree_content_y = 1u16 + 1; // title + top border
    let tree_content_x = 1u16; // left border
    let row = mouse.row.saturating_sub(tree_content_y);
    let col = mouse.column.saturating_sub(tree_content_x);

    let items = state.flatten(tree);
    state.handle_click(row, &items);

    // Determine which node within the row was clicked using the same layout
    // math the widget uses.
    if let Some(item) = items.get(state.selected_row()) {
        let nc = ratatui_tree_widget::node_columns(item, &Default::default());
        let node_idx = ratatui_tree_widget::node_index_at_col(col, &nc);
        state.set_selected_node(node_idx);
    }
}

// ---------------------------------------------------------------------------
// Sample tree
// ---------------------------------------------------------------------------

fn build_sample_tree() -> TreeNode {
    // A rich tree that demonstrates:
    //   - Single-child chains rendered inline
    //   - Multiple children
    //   - Deep nesting
    //   - Many leaves

    TreeNode::with_children(
        "Project Root",
        vec![
            // ── Single-child chain ──────────────────────────────────────
            TreeNode::with_children(
                "README.md",
                vec![TreeNode::with_children(
                    "Draft",
                    vec![TreeNode::new("v1.0")],
                )],
            ),
            // ─── Source directory ────────────────────────────────────────
            TreeNode::with_children(
                "src",
                vec![
                    TreeNode::with_children(
                        "main.rs",
                        vec![
                            TreeNode::new("fn main()"),
                            TreeNode::new("parse_args()"),
                            TreeNode::new("run()"),
                        ],
                    ),
                    TreeNode::with_children(
                        "lib.rs",
                        vec![
                            TreeNode::new("pub fn init()"),
                            TreeNode::new("pub fn handle()"),
                            TreeNode::new("pub fn cleanup()"),
                        ],
                    ),
                    TreeNode::with_children(
                        "utils.rs",
                        vec![TreeNode::new("fn hash()"), TreeNode::new("fn validate()")],
                    ),
                ],
            ),
            // ─── Config ──────────────────────────────────────────────────
            TreeNode::with_children(
                "config",
                vec![
                    TreeNode::with_children(
                        "settings.toml",
                        vec![TreeNode::with_children(
                            "User",
                            vec![TreeNode::new("theme = \"dark\"")],
                        )],
                    ),
                    TreeNode::new("defaults.toml"),
                ],
            ),
            // ─── Nested single-child chain ──────────────────────────────
            TreeNode::with_children(
                "docs",
                vec![TreeNode::with_children(
                    "guide",
                    vec![TreeNode::with_children(
                        "getting-started.md",
                        vec![TreeNode::with_children(
                            "chapter-1.md",
                            vec![TreeNode::new("Installation")],
                        )],
                    )],
                )],
            ),
            // ─── Assets (leaf) ───────────────────────────────────────────
            TreeNode::new("assets/"),
            // ─── Tests ───────────────────────────────────────────────────
            TreeNode::with_children(
                "tests",
                vec![
                    TreeNode::new("integration_test.rs"),
                    TreeNode::new("unit_test.rs"),
                ],
            ),
        ],
    )
}
