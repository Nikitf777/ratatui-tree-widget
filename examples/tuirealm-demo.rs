//! A [tui-realm](https://docs.rs/tuirealm) application demonstrating the
//! [`TreeRealm`] component wrapper.
//!
//! Run with:
//!
//! ```sh
//! cargo run --example tuirealm-demo --features tuirealm
//! ```

use std::error::Error;
use std::time::Duration;

use ratatui_tree_widget::{TreeNode, TreeRealm};
use tuirealm::application::{Application, PollStrategy};
use tuirealm::command::{Cmd, CmdResult, Direction};
use tuirealm::component::{AppComponent, Component};
use tuirealm::event::{Event, Key, KeyEvent, KeyModifiers, MouseEventKind, NoUserEvent};
use tuirealm::listener::EventListenerCfg;
use tuirealm::props::{AttrValue, Attribute, Color, Props, QueryResult, Style, TextModifiers};
use tuirealm::ratatui::layout::{Constraint, Direction as LayoutDirection, Layout, Rect};
use tuirealm::ratatui::widgets::Paragraph;
use tuirealm::ratatui::Frame;
use tuirealm::state::State;
use tuirealm::terminal::{CrosstermTerminalAdapter, TerminalAdapter};

// ---------------------------------------------------------------------------
// Messages and component ids
// ---------------------------------------------------------------------------

#[derive(Debug, PartialEq)]
pub enum Msg {
    AppClose,
    Err(String),
    Selected(String),
}

#[derive(Debug, Eq, PartialEq, Clone, Hash)]
pub enum Id {
    Tree,
    Status,
}

// ---------------------------------------------------------------------------
// Tree component (AppComponent wrapping TreeRealm)
// ---------------------------------------------------------------------------

/// The interactive tree: forwards keyboard/mouse events to [`TreeRealm`] and
/// reports selection changes as [`Msg::Selected`].
pub struct TreeApp {
    component: TreeRealm,
}

impl TreeApp {
    pub fn new(roots: Vec<TreeNode>) -> Self {
        let mut component = TreeRealm::new(roots)
            .title(" Tree ")
            .style(Style::default().fg(Color::White))
            .highlight_style(
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::LightGreen)
                    .add_modifier(TextModifiers::BOLD),
            )
            .highlight_style_unfocused(Style::default().fg(Color::DarkGray))
            .interactive(true);
        // Expand the first root by default.
        component.expand(&[0]);
        Self { component }
    }
}

impl AppComponent<Msg, NoUserEvent> for TreeApp {
    fn on(&mut self, ev: &Event<NoUserEvent>) -> Option<Msg> {
        match ev {
            // ---- Keyboard -------------------------------------------------
            Event::Keyboard(KeyEvent {
                code,
                modifiers: KeyModifiers::NONE,
            }) => {
                let cmd = match *code {
                    Key::Up | Key::Char('k') => Cmd::Move(Direction::Up),
                    Key::Down | Key::Char('j') => Cmd::Move(Direction::Down),
                    Key::Left | Key::Char('h') => Cmd::Move(Direction::Left),
                    Key::Right | Key::Char('l') => Cmd::Move(Direction::Right),
                    Key::Enter | Key::Char(' ') => Cmd::Toggle,
                    Key::Esc | Key::Char('q') => return Some(Msg::AppClose),
                    _ => Cmd::None,
                };
                match self.component.perform(cmd) {
                    CmdResult::Changed(_) => self.selected_msg(),
                    _ => None,
                }
            }
            // ---- Mouse ----------------------------------------------------
            // The tree area starts at row 1 (title); the bordered block adds
            // a 1-char top and left border.
            Event::Mouse(mouse) if matches!(mouse.kind, MouseEventKind::Down(_)) => {
                let row = mouse.row.saturating_sub(2);
                let col = mouse.column.saturating_sub(1);
                match self.component.on_click(col, row) {
                    CmdResult::Changed(_) => self.selected_msg(),
                    _ => None,
                }
            }
            _ => None,
        }
    }
}

/// Forward all other `Component` methods to the wrapped [`TreeRealm`].
impl Component for TreeApp {
    fn view(&mut self, frame: &mut Frame, area: Rect) {
        self.component.view(frame, area);
    }

    fn query<'a>(&'a self, attr: Attribute) -> Option<QueryResult<'a>> {
        self.component.query(attr)
    }

    fn attr(&mut self, attr: Attribute, value: AttrValue) {
        self.component.attr(attr, value);
    }

    fn state(&self) -> State {
        self.component.state()
    }

    fn perform(&mut self, cmd: Cmd) -> CmdResult {
        self.component.perform(cmd)
    }
}

impl TreeApp {
    #[inline]
    fn selected_msg(&self) -> Option<Msg> {
        self.component.selected_text().map(Msg::Selected)
    }
}

// ---------------------------------------------------------------------------
// Status label component (borrowed from the tui-realm demo)
// ---------------------------------------------------------------------------

#[derive(Default)]
pub struct Label {
    props: Props,
}

impl Label {
    pub fn text(mut self, text: &str) -> Self {
        self.attr(Attribute::Text, AttrValue::String(text.to_string()));
        self
    }

    pub fn color(mut self, color: Color) -> Self {
        self.attr(Attribute::Color, AttrValue::Color(color));
        self
    }
}

impl Component for Label {
    fn view(&mut self, frame: &mut Frame, area: Rect) {
        if matches!(
            self.props.get(Attribute::Display),
            Some(AttrValue::Flag(false))
        ) {
            return;
        }
        let text = self
            .props
            .get(Attribute::Text)
            .and_then(AttrValue::as_string)
            .map(String::as_str)
            .unwrap_or_default();
        let color = self
            .props
            .get(Attribute::Color)
            .and_then(AttrValue::as_color)
            .unwrap_or(Color::Reset);
        frame.render_widget(Paragraph::new(text).style(Style::default().fg(color)), area);
    }

    fn query<'a>(&'a self, attr: Attribute) -> Option<QueryResult<'a>> {
        self.props.get_for_query(attr)
    }

    fn attr(&mut self, attr: Attribute, value: AttrValue) {
        self.props.set(attr, value);
    }

    fn state(&self) -> State {
        State::None
    }

    fn perform(&mut self, _cmd: Cmd) -> CmdResult {
        CmdResult::Invalid(Cmd::None)
    }
}

impl AppComponent<Msg, NoUserEvent> for Label {
    fn on(&mut self, _: &Event<NoUserEvent>) -> Option<Msg> {
        // The status label does not react to events.
        None
    }
}

// ---------------------------------------------------------------------------
// Application model
// ---------------------------------------------------------------------------

struct Model {
    app: Application<Id, Msg, NoUserEvent>,
    quit: bool,
    redraw: bool,
    terminal: CrosstermTerminalAdapter,
}

impl Default for Model {
    fn default() -> Self {
        Self {
            app: Self::init_app().expect("failed to initialise application"),
            quit: false,
            redraw: true,
            terminal: Self::init_terminal().expect("failed to initialise terminal"),
        }
    }
}

impl Model {
    fn init_terminal() -> Result<CrosstermTerminalAdapter, Box<dyn Error>> {
        let mut adapter = CrosstermTerminalAdapter::new()?;
        adapter.enable_raw_mode()?;
        adapter.enter_alternate_screen()?;
        adapter.enable_mouse_capture()?;
        Ok(adapter)
    }

    fn init_app() -> Result<Application<Id, Msg, NoUserEvent>, Box<dyn Error>> {
        let mut app: Application<Id, Msg, NoUserEvent> = Application::init(
            EventListenerCfg::default().crossterm_input_listener(Duration::from_millis(16), 10),
        );

        app.mount(
            Id::Tree,
            Box::new(TreeApp::new(build_forest())),
            Vec::default(),
        )?;
        app.mount(
            Id::Status,
            Box::new(
                Label::default().text("←/→ move between nodes, ↑/↓ rows, Enter toggles, q quits"),
            ),
            Vec::default(),
        )?;

        app.active(&Id::Tree)?;
        Ok(app)
    }

    fn view(&mut self) {
        let _ = self.terminal.draw(|f| {
            let [title, tree, help, status] = Layout::default()
                .direction(LayoutDirection::Vertical)
                .constraints(
                    [
                        Constraint::Length(1),
                        Constraint::Min(5),
                        Constraint::Length(1),
                        Constraint::Length(1),
                    ]
                    .as_ref(),
                )
                .areas(f.area());

            f.render_widget(
                Paragraph::new(" TreeRealm — tui-realm demo ").style(
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(TextModifiers::BOLD),
                ),
                title,
            );
            self.app.view(&Id::Tree, f, tree);
            self.app.view(&Id::Status, f, status);

            // Click/drag is reported by tuirealm; show a legend while the
            // tree keeps its own interaction.
            f.render_widget(
                Paragraph::new(" Mouse: click selects a node, click again toggles ")
                    .style(Style::default().fg(Color::DarkGray)),
                help,
            );
        });
    }

    fn update(&mut self, msg: Msg) {
        match msg {
            Msg::AppClose => self.quit = true,
            Msg::Err(err) => {
                let _ = self.app.attr(
                    &Id::Status,
                    Attribute::Text,
                    AttrValue::String(format!("Error: {err}")),
                );
                self.redraw = true;
            }
            Msg::Selected(text) => {
                let _ = self.app.attr(
                    &Id::Status,
                    Attribute::Text,
                    AttrValue::String(format!("Selected: {text}")),
                );
                self.redraw = true;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

fn main() -> Result<(), Box<dyn Error>> {
    let mut model = Model::default();

    while !model.quit {
        match model
            .app
            .tick(PollStrategy::Once(Duration::from_millis(20)))
        {
            Err(err) => {
                model.update(Msg::Err(err.to_string()));
            }
            Ok(messages) if !messages.is_empty() => {
                for msg in messages {
                    model.update(msg);
                }
            }
            _ => {}
        }
        if model.redraw {
            model.view();
            model.redraw = false;
        }
    }

    model.terminal.leave_alternate_screen()?;
    model.terminal.disable_raw_mode()?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Sample forest
// ---------------------------------------------------------------------------

fn build_forest() -> Vec<TreeNode> {
    vec![
        TreeNode::with_children(
            "Project Alpha",
            vec![
                TreeNode::with_children(
                    "README.md",
                    vec![TreeNode::with_children(
                        "Draft",
                        vec![TreeNode::new("v1.0")],
                    )],
                ),
                TreeNode::with_children(
                    "src",
                    vec![
                        TreeNode::with_children("main.rs", vec![TreeNode::new("fn main()")]),
                        TreeNode::with_children("lib.rs", vec![TreeNode::new("pub fn init()")]),
                    ],
                ),
                TreeNode::with_children(
                    "docs",
                    vec![TreeNode::with_children(
                        "guide",
                        vec![TreeNode::with_children(
                            "getting-started.md",
                            vec![TreeNode::new("chapter-1.md")],
                        )],
                    )],
                ),
            ],
        ),
        TreeNode::with_children(
            "Project Beta",
            vec![
                TreeNode::new("assets/"),
                TreeNode::with_children(
                    "tests",
                    vec![
                        TreeNode::new("integration_test.rs"),
                        TreeNode::new("unit_test.rs"),
                    ],
                ),
            ],
        ),
    ]
}
