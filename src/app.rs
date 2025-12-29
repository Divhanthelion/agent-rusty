use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Clear},
    Frame,
};

use crate::actions::Action;
use crate::tmux::{AgentStatus, TmuxSession};

/// Theme colors inspired by Claude Code
pub struct Theme {
    pub bg: Color,
    pub fg: Color,
    pub accent: Color,
    pub dim: Color,
    pub success: Color,
    pub warning: Color,
    pub error: Color,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            bg: Color::Rgb(30, 30, 30),
            fg: Color::Rgb(220, 220, 220),
            accent: Color::Rgb(217, 119, 87), // Claude orange
            dim: Color::Rgb(100, 100, 100),
            success: Color::Rgb(80, 200, 120),
            warning: Color::Rgb(255, 193, 7),
            error: Color::Rgb(220, 53, 69),
        }
    }
}

/// Input mode for the application
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputMode {
    Normal,
    Creating,
    Confirming,
}

/// Main application state
pub struct App {
    /// List of tmux sessions
    pub sessions: Vec<TmuxSession>,
    /// Currently selected session index
    pub list_state: ListState,
    /// Current message to display (info or error)
    pub error_message: Option<String>,
    /// Whether MCP mode is active
    pub mcp_mode: bool,
    /// Theme
    pub theme: Theme,
    /// Current input mode
    pub input_mode: InputMode,
    /// Text input buffer
    pub input_buffer: String,
    /// Pending action queue
    pub pending_actions: Vec<Action>,
}

impl App {
    pub fn new() -> Self {
        let mut list_state = ListState::default();
        list_state.select(Some(0));

        Self {
            sessions: Vec::new(),
            list_state,
            error_message: None,
            mcp_mode: false,
            theme: Theme::default(),
            input_mode: InputMode::Normal,
            input_buffer: String::new(),
            pending_actions: Vec::new(),
        }
    }

    /// Get the currently selected session
    pub fn selected_session(&self) -> Option<&TmuxSession> {
        self.list_state
            .selected()
            .and_then(|i| self.sessions.get(i))
    }

    /// Take pending actions (drains the queue)
    pub fn take_pending_actions(&mut self) -> Vec<Action> {
        std::mem::take(&mut self.pending_actions)
    }

    /// Handle an action and return whether to quit
    pub fn handle_action(&mut self, action: Action) -> Result<bool> {
        match action {
            Action::KeyPress(key) => self.handle_key(key),
            Action::SessionsUpdated(sessions) => {
                self.sessions = sessions;
                // Ensure selection is valid
                if let Some(selected) = self.list_state.selected() {
                    if selected >= self.sessions.len() && !self.sessions.is_empty() {
                        self.list_state.select(Some(self.sessions.len() - 1));
                    }
                }
                Ok(false)
            }
            Action::Error(msg) => {
                self.error_message = Some(msg);
                Ok(false)
            }
            Action::Quit => Ok(true),
            _ => Ok(false),
        }
    }

    fn handle_key(&mut self, key: KeyEvent) -> Result<bool> {
        // Clear error message on any key press
        if self.error_message.is_some() && self.input_mode == InputMode::Normal {
            self.error_message = None;
        }

        match self.input_mode {
            InputMode::Normal => self.handle_normal_key(key),
            InputMode::Creating => self.handle_creating_key(key),
            InputMode::Confirming => self.handle_confirming_key(key),
        }
    }

    fn handle_normal_key(&mut self, key: KeyEvent) -> Result<bool> {
        match key.code {
            KeyCode::Char('q') => return Ok(true),
            KeyCode::Char('j') | KeyCode::Down => self.next_session(),
            KeyCode::Char('k') | KeyCode::Up => self.previous_session(),
            KeyCode::Char('M') => self.mcp_mode = !self.mcp_mode,
            KeyCode::Enter => {
                if let Some(session) = self.selected_session() {
                    self.pending_actions
                        .push(Action::AttachSession(session.id.clone()));
                }
            }
            KeyCode::Char('n') => {
                self.input_mode = InputMode::Creating;
                self.input_buffer.clear();
            }
            KeyCode::Char('d') => {
                if self.selected_session().is_some() {
                    self.input_mode = InputMode::Confirming;
                }
            }
            KeyCode::Char('y') => {
                self.pending_actions.push(Action::CopySkeleton);
            }
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                return Ok(true);
            }
            _ => {}
        }
        Ok(false)
    }

    fn handle_creating_key(&mut self, key: KeyEvent) -> Result<bool> {
        match key.code {
            KeyCode::Enter => {
                if !self.input_buffer.is_empty() {
                    let name = self.input_buffer.clone();
                    self.pending_actions.push(Action::CreateSession(name));
                    self.input_buffer.clear();
                }
                self.input_mode = InputMode::Normal;
            }
            KeyCode::Esc => {
                self.input_buffer.clear();
                self.input_mode = InputMode::Normal;
            }
            KeyCode::Char(c) => {
                // Only allow valid session name characters
                if c.is_alphanumeric() || c == '-' || c == '_' {
                    self.input_buffer.push(c);
                }
            }
            KeyCode::Backspace => {
                self.input_buffer.pop();
            }
            _ => {}
        }
        Ok(false)
    }

    fn handle_confirming_key(&mut self, key: KeyEvent) -> Result<bool> {
        match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                if let Some(session) = self.selected_session() {
                    self.pending_actions
                        .push(Action::DeleteSession(session.id.clone()));
                }
                self.input_mode = InputMode::Normal;
            }
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                self.input_mode = InputMode::Normal;
            }
            _ => {}
        }
        Ok(false)
    }

    fn next_session(&mut self) {
        if self.sessions.is_empty() {
            return;
        }
        let i = match self.list_state.selected() {
            Some(i) => {
                if i >= self.sessions.len() - 1 {
                    0
                } else {
                    i + 1
                }
            }
            None => 0,
        };
        self.list_state.select(Some(i));
    }

    fn previous_session(&mut self) {
        if self.sessions.is_empty() {
            return;
        }
        let i = match self.list_state.selected() {
            Some(i) => {
                if i == 0 {
                    self.sessions.len() - 1
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.list_state.select(Some(i));
    }

    pub fn render(&mut self, frame: &mut Frame) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Header
                Constraint::Min(0),    // Main content
                Constraint::Length(3), // Footer/status
            ])
            .split(frame.area());

        self.render_header(frame, chunks[0]);
        self.render_main(frame, chunks[1]);
        self.render_footer(frame, chunks[2]);

        // Render modal dialogs on top
        match self.input_mode {
            InputMode::Creating => self.render_create_dialog(frame),
            InputMode::Confirming => self.render_confirm_dialog(frame),
            InputMode::Normal => {}
        }
    }

    fn render_header(&self, frame: &mut Frame, area: Rect) {
        let title = Paragraph::new(Line::from(vec![
            Span::styled(
                " AgentRusty ",
                Style::default()
                    .fg(self.theme.accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "│ Mission Control for AI Agents",
                Style::default().fg(self.theme.dim),
            ),
        ]))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(self.theme.dim)),
        );
        frame.render_widget(title, area);
    }

    fn render_main(&mut self, frame: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(40), // Session list
                Constraint::Percentage(60), // Detail pane
            ])
            .split(area);

        self.render_session_list(frame, chunks[0]);
        self.render_detail_pane(frame, chunks[1]);
    }

    fn render_session_list(&mut self, frame: &mut Frame, area: Rect) {
        let items: Vec<ListItem> = if self.sessions.is_empty() {
            vec![ListItem::new(Line::from(Span::styled(
                "  No sessions found. Press 'n' to create one.",
                Style::default().fg(self.theme.dim),
            )))]
        } else {
            self.sessions
                .iter()
                .map(|session| {
                    let status_icon = match session.status {
                        AgentStatus::Busy => {
                            Span::styled("● ", Style::default().fg(self.theme.warning))
                        }
                        AgentStatus::Idle => {
                            Span::styled("● ", Style::default().fg(self.theme.success))
                        }
                        AgentStatus::WaitingForInput => {
                            Span::styled("? ", Style::default().fg(self.theme.accent))
                        }
                        AgentStatus::Error => {
                            Span::styled("✗ ", Style::default().fg(self.theme.error))
                        }
                        AgentStatus::Unknown => {
                            Span::styled("○ ", Style::default().fg(self.theme.dim))
                        }
                    };

                    let name = Span::styled(&session.name, Style::default().fg(self.theme.fg));

                    ListItem::new(Line::from(vec![status_icon, name]))
                })
                .collect()
        };

        let list = List::new(items)
            .block(
                Block::default()
                    .title(" Sessions ")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(self.theme.dim)),
            )
            .highlight_style(
                Style::default()
                    .bg(Color::Rgb(50, 50, 50))
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("▶ ");

        frame.render_stateful_widget(list, area, &mut self.list_state);
    }

    fn render_detail_pane(&self, frame: &mut Frame, area: Rect) {
        let content = if let Some(session) = self.selected_session() {
            vec![
                Line::from(vec![
                    Span::styled("Name: ", Style::default().fg(self.theme.dim)),
                    Span::styled(&session.name, Style::default().fg(self.theme.fg)),
                ]),
                Line::from(vec![
                    Span::styled("ID: ", Style::default().fg(self.theme.dim)),
                    Span::styled(&session.id, Style::default().fg(self.theme.fg)),
                ]),
                Line::from(vec![
                    Span::styled("Status: ", Style::default().fg(self.theme.dim)),
                    Span::styled(
                        format!("{:?}", session.status),
                        Style::default().fg(match session.status {
                            AgentStatus::Busy => self.theme.warning,
                            AgentStatus::Idle => self.theme.success,
                            AgentStatus::WaitingForInput => self.theme.accent,
                            AgentStatus::Error => self.theme.error,
                            AgentStatus::Unknown => self.theme.dim,
                        }),
                    ),
                ]),
                Line::from(vec![
                    Span::styled("Clients: ", Style::default().fg(self.theme.dim)),
                    Span::styled(
                        session.attached_clients.to_string(),
                        Style::default().fg(self.theme.fg),
                    ),
                ]),
                Line::from(""),
                Line::from(Span::styled(
                    "Press Enter to attach, 'd' to delete",
                    Style::default().fg(self.theme.dim),
                )),
            ]
        } else {
            vec![
                Line::from(Span::styled(
                    "No session selected",
                    Style::default().fg(self.theme.dim),
                )),
                Line::from(""),
                Line::from(Span::styled(
                    "Press 'n' to create a new session",
                    Style::default().fg(self.theme.dim),
                )),
            ]
        };

        let detail = Paragraph::new(content).block(
            Block::default()
                .title(" Details ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(self.theme.dim)),
        );
        frame.render_widget(detail, area);
    }

    fn render_footer(&self, frame: &mut Frame, area: Rect) {
        let help_text = if self.mcp_mode {
            " MCP Mode │ Space: Toggle │ Esc: Exit "
        } else {
            " q: Quit │ j/k: Navigate │ Enter: Attach │ n: New │ d: Delete │ y: Copy skeleton │ M: MCP "
        };

        let content = if let Some(ref msg) = self.error_message {
            let style = if msg.contains("copied") || msg.contains("success") {
                Style::default().fg(self.theme.success)
            } else {
                Style::default().fg(self.theme.error)
            };
            Line::from(Span::styled(format!(" {} ", msg), style))
        } else {
            Line::from(Span::styled(help_text, Style::default().fg(self.theme.dim)))
        };

        let footer = Paragraph::new(content).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(self.theme.dim)),
        );
        frame.render_widget(footer, area);
    }

    fn render_create_dialog(&self, frame: &mut Frame) {
        let area = centered_rect(50, 20, frame.area());

        frame.render_widget(Clear, area);

        let block = Block::default()
            .title(" Create New Session ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(self.theme.accent));

        let inner = block.inner(area);
        frame.render_widget(block, area);

        let text = vec![
            Line::from(""),
            Line::from(Span::styled(
                "Enter session name:",
                Style::default().fg(self.theme.fg),
            )),
            Line::from(""),
            Line::from(Span::styled(
                format!("▶ {}_", self.input_buffer),
                Style::default()
                    .fg(self.theme.accent)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "Press Enter to create, Esc to cancel",
                Style::default().fg(self.theme.dim),
            )),
        ];

        let paragraph = Paragraph::new(text);
        frame.render_widget(paragraph, inner);
    }

    fn render_confirm_dialog(&self, frame: &mut Frame) {
        let area = centered_rect(50, 20, frame.area());

        frame.render_widget(Clear, area);

        let block = Block::default()
            .title(" Confirm Delete ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(self.theme.error));

        let inner = block.inner(area);
        frame.render_widget(block, area);

        let session_name = self
            .selected_session()
            .map(|s| s.name.as_str())
            .unwrap_or("unknown");

        let text = vec![
            Line::from(""),
            Line::from(Span::styled(
                format!("Delete session '{}'?", session_name),
                Style::default().fg(self.theme.fg),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "This action cannot be undone.",
                Style::default().fg(self.theme.warning),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "Press 'y' to confirm, 'n' or Esc to cancel",
                Style::default().fg(self.theme.dim),
            )),
        ];

        let paragraph = Paragraph::new(text);
        frame.render_widget(paragraph, inner);
    }
}

/// Helper function to create a centered rectangle
fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}
