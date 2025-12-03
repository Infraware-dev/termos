use anyhow::{Context, Result};
use crossterm::{
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
/// TUI rendering logic using ratatui
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Margin, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Wrap},
    Frame, Terminal,
};
use std::io;

use super::state::{TerminalMode, TerminalState};

// Layout constants for terminal areas
/// Height of the status bar (1 line)
const STATUS_BAR_HEIGHT: u16 = 1;
/// Height of the input area (3 lines including borders)
const INPUT_AREA_HEIGHT: u16 = 3;
/// Height of output area borders (top + bottom)
const OUTPUT_BORDER_HEIGHT: u16 = 2;

/// TUI wrapper for the terminal
pub struct TerminalUI {
    terminal: Terminal<CrosstermBackend<io::Stdout>>,
}

impl std::fmt::Debug for TerminalUI {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TerminalUI")
            .field("terminal", &"<Terminal>")
            .finish()
    }
}

impl TerminalUI {
    /// Create a new TUI instance
    pub fn new() -> Result<Self> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(stdout);
        let terminal = Terminal::new(backend)?;

        Ok(Self { terminal })
    }

    /// Render the terminal UI
    /// Updates state.visible_lines based on actual terminal size
    pub fn render(&mut self, state: &mut TerminalState) -> Result<()> {
        // Calculate visible lines from terminal size before rendering
        let size = self.terminal.size()?;

        // Output area height = total - status bar - input area
        let output_height = size
            .height
            .saturating_sub(STATUS_BAR_HEIGHT + INPUT_AREA_HEIGHT);
        // Visible content lines = output area minus borders
        let visible_lines = output_height.saturating_sub(OUTPUT_BORDER_HEIGHT) as usize;
        state.set_visible_lines(visible_lines);

        self.terminal.draw(|frame| {
            render_frame(frame, state);
        })?;
        Ok(())
    }

    /// Clear the terminal screen completely
    pub fn clear(&mut self) -> Result<()> {
        self.terminal.clear()?;
        Ok(())
    }

    /// Get mutable reference to inner terminal for splash screen
    pub fn inner_terminal(&mut self) -> &mut Terminal<CrosstermBackend<io::Stdout>> {
        &mut self.terminal
    }

    /// Clean up the terminal on exit
    pub fn cleanup(&mut self) -> Result<()> {
        disable_raw_mode()?;
        execute!(self.terminal.backend_mut(), LeaveAlternateScreen)?;
        self.terminal.show_cursor()?;
        Ok(())
    }

    /// Suspend TUI mode for interactive command execution
    ///
    /// Disables raw mode, leaves alternate screen, shows cursor.
    /// Terminal returns to normal state for interactive commands like vim, less, etc.
    pub fn suspend(&mut self) -> Result<()> {
        // Show cursor before leaving
        self.terminal.show_cursor()?;

        // Flush any pending draws to prevent artifacts
        use std::io::Write;
        self.terminal
            .backend_mut()
            .flush()
            .context("Failed to flush terminal before suspension")?;

        // Leave alternate screen
        execute!(self.terminal.backend_mut(), LeaveAlternateScreen)?;

        // Disable raw mode
        disable_raw_mode()?;

        Ok(())
    }

    /// Resume TUI mode after interactive command completes
    ///
    /// Re-enables raw mode, enters alternate screen, clears screen.
    pub fn resume(&mut self) -> Result<()> {
        // Enable raw mode
        enable_raw_mode()?;

        // Enter alternate screen
        execute!(self.terminal.backend_mut(), EnterAlternateScreen)?;

        // Clear screen to prevent artifacts
        self.terminal.clear()?;

        Ok(())
    }
}

impl Drop for TerminalUI {
    fn drop(&mut self) {
        let _ = self.cleanup();
    }
}

/// Render a single frame
fn render_frame(frame: &mut Frame, state: &TerminalState) {
    let size = frame.area();

    // Create layout: output area + status bar + input area
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),    // Output area
            Constraint::Length(1), // Status bar
            Constraint::Length(3), // Input area
        ])
        .split(size);

    // Render output area
    render_output(frame, chunks[0], state);

    // Render status bar
    render_status_bar(frame, chunks[1], state);

    // Render input area
    render_input(frame, chunks[2], state);
}

/// Render the output buffer with scrollbar
fn render_output(frame: &mut Frame, area: Rect, state: &TerminalState) {
    let total_lines = state.output.lines().len();
    // Use pre-calculated visible_lines from state (set in render())
    let visible_lines = state.visible_lines();

    // Calculate scroll position once for both content and scrollbar
    let scroll_pos = state.output.scroll_position();
    let max_scroll = total_lines.saturating_sub(visible_lines);
    let effective_scroll = scroll_pos.min(max_scroll);

    let output_text = if total_lines == 0 {
        vec![Line::from(Span::styled(
            "Infraware Terminal - Type a command or ask a question",
            Style::default().fg(Color::Gray),
        ))]
    } else {
        // Calculate start and end indices for visible window
        let start = effective_scroll;
        let end = (start + visible_lines).min(total_lines);

        state.output.lines()[start..end]
            .iter()
            .map(|line| {
                // Parse ANSI codes and convert to ratatui spans with proper styling
                use ansi_to_tui::IntoText;
                match line.into_text() {
                    Ok(text) => text
                        .lines
                        .into_iter()
                        .next()
                        .unwrap_or_else(|| Line::from(line.clone())),
                    Err(_) => Line::from(line.clone()),
                }
            })
            .collect()
    };

    let output_widget = Paragraph::new(output_text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Output ")
                .title_style(Style::default().fg(Color::Cyan)),
        )
        .wrap(Wrap { trim: false });

    frame.render_widget(output_widget, area);

    // Render scrollbar only if content exceeds visible area
    if total_lines > visible_lines {
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(Some("▲"))
            .end_symbol(Some("▼"))
            .track_symbol(Some("│"))
            .thumb_symbol("█");

        let mut scrollbar_state = ScrollbarState::new(max_scroll).position(effective_scroll);

        // Render scrollbar in the inner area (inside the border)
        let inner_area = area.inner(Margin {
            horizontal: 0,
            vertical: 1,
        });
        frame.render_stateful_widget(scrollbar, inner_area, &mut scrollbar_state);
    }
}

/// Render the status bar
fn render_status_bar(frame: &mut Frame, area: Rect, state: &TerminalState) {
    let mode_text = match state.mode {
        TerminalMode::Normal => "READY",
        TerminalMode::ExecutingCommand => "EXECUTING...",
        TerminalMode::WaitingLLM => "WAITING FOR LLM...",
        TerminalMode::PromptingInstall => "INSTALL PROMPT",
        TerminalMode::AwaitingCommandApproval => "APPROVE? [y/n]",
        TerminalMode::AwaitingAnswer => "ANSWER?",
    };

    let mode_color = match state.mode {
        TerminalMode::Normal => Color::Green,
        TerminalMode::ExecutingCommand => Color::Yellow,
        TerminalMode::WaitingLLM => Color::Blue,
        TerminalMode::PromptingInstall => Color::Magenta,
        TerminalMode::AwaitingCommandApproval => Color::Cyan,
        TerminalMode::AwaitingAnswer => Color::Yellow,
    };

    let status_text = Line::from(vec![
        Span::styled(
            format!(" {mode_text} "),
            Style::default()
                .fg(Color::Black)
                .bg(mode_color)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" | "),
        Span::styled(
            format!("History: {} ", state.history.all().len()),
            Style::default().fg(Color::Gray),
        ),
    ]);

    let status_widget = Paragraph::new(status_text);
    frame.render_widget(status_widget, area);
}

/// Render the input area
fn render_input(frame: &mut Frame, area: Rect, state: &TerminalState) {
    let input_text = Line::from(vec![
        Span::styled(
            "❯ ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(state.input.text()),
    ]);

    let input_widget = Paragraph::new(input_text).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Input ")
            .title_style(Style::default().fg(Color::Cyan)),
    );

    frame.render_widget(input_widget, area);

    // Set cursor position (2 accounts for "❯ " prefix and border)
    frame.set_cursor_position((
        area.x + state.input.cursor_position() as u16 + 3,
        area.y + 1,
    ));
}
