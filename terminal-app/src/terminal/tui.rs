use anyhow::{Context, Result};
use crossterm::{
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
/// NEW TUI rendering logic using ratatui - Unified inline terminal design
use ratatui::{
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame, Terminal,
};
use std::io;

use super::state::{TerminalMode, TerminalState};

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

        // Header height = 1 line
        // Visible content lines = total height - header
        let visible_lines = size.height.saturating_sub(1) as usize;
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

/// Render a single frame - NEW DESIGN: 2 sections (Header + Unified Content)
fn render_frame(frame: &mut Frame, state: &TerminalState) {
    let size = frame.area();

    // Create layout: header bar + unified content area
    let chunks = Layout::vertical([
        Constraint::Length(1),  // Header bar
        Constraint::Min(1),     // Unified content (output + prompt inline)
    ])
    .split(size);

    // Render header bar
    render_header_bar(frame, chunks[0]);

    // Render unified content (output + prompt inline)
    render_unified_content(frame, chunks[1], state);
}

/// Render header bar with logo and icons
fn render_header_bar(frame: &mut Frame, area: ratatui::layout::Rect) {
    let layout = Layout::horizontal([
        Constraint::Length(4),  // "~ +"
        Constraint::Min(1),     // Spacer
        Constraint::Length(9),  // "⚙ − □ ×"
    ])
    .split(area);

    // Logo and "+" button (decorative for now)
    let logo = Paragraph::new("~ +")
        .style(Style::default().fg(Color::White).bg(Color::Black));

    // Icons on the right (decorative for now)
    let icons = Paragraph::new("⚙ − □ ×")
        .style(Style::default().fg(Color::White).bg(Color::Black))
        .alignment(Alignment::Right);

    frame.render_widget(logo, layout[0]);
    frame.render_widget(icons, layout[2]);
}

/// Render unified content area with inline prompt
fn render_unified_content(
    frame: &mut Frame,
    area: ratatui::layout::Rect,
    state: &TerminalState,
) {
    let mut lines = Vec::new();

    // 1. Add historical output from OutputBuffer
    for line in state.output.lines() {
        // Parse ANSI codes and convert to ratatui Line with proper styling
        use ansi_to_tui::IntoText;
        match line.into_text() {
            Ok(text) => {
                // Get the first line from parsed text, or fallback to raw
                let parsed_line = text
                    .lines
                    .into_iter()
                    .next()
                    .unwrap_or_else(|| Line::from(line.clone()));
                lines.push(parsed_line);
            }
            Err(_) => lines.push(Line::from(line.clone())),
        }
    }

    // 2. Add current prompt + input inline (with mode-based color)
    let prompt = format_prompt();
    let input = state.input.text();
    let prompt_color = get_prompt_color(&state.mode);
    let current_line = Line::from(vec![
        Span::styled(
            prompt.clone(),
            Style::default()
                .fg(prompt_color)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(input),
    ]);
    lines.push(current_line);

    // 3. Calculate visible window (auto-scroll to bottom)
    let visible_lines = area.height as usize;
    let start = lines.len().saturating_sub(visible_lines);
    let visible_window: Vec<Line> = lines[start..].to_vec();

    // 4. Render paragraph WITHOUT borders
    let paragraph = Paragraph::new(visible_window.clone());
    frame.render_widget(paragraph, area);

    // 5. Position cursor at end of current prompt line
    // The prompt is always the last line in visible_window
    let prompt_line_y = visible_window.len().saturating_sub(1) as u16;
    let cursor_x = area.x + (prompt.len() + state.input.cursor_position()) as u16;
    let cursor_y = area.y + prompt_line_y;
    frame.set_cursor_position((cursor_x, cursor_y));
}

/// Format prompt - DYNAMIC with real hostname, user, and path
fn format_prompt() -> String {
    // Get current user
    let user = std::env::var("USER").unwrap_or_else(|_| "user".to_string());

    // Get system hostname
    let hostname = hostname::get()
        .ok()
        .and_then(|h| h.into_string().ok())
        .unwrap_or_else(|| "hostname".to_string());

    // Get current working directory with ~ abbreviation for home
    let cwd = std::env::current_dir()
        .ok()
        .map(|p| {
            // Try to abbreviate home directory with ~
            if let Ok(home) = std::env::var("HOME") {
                if let Ok(stripped) = p.strip_prefix(&home) {
                    let stripped_str = stripped.display().to_string();
                    return if stripped_str.is_empty() {
                        "~".to_string()
                    } else {
                        format!("~/{}", stripped_str)
                    };
                }
            }
            p.display().to_string()
        })
        .unwrap_or_else(|| "~".to_string());

    // Root vs user prompt symbol
    let prompt_char = if user == "root" { "#" } else { "$" };

    format!("|~| {}@{}:{}{} ", user, hostname, cwd, prompt_char)
}

/// Get prompt color based on terminal mode
fn get_prompt_color(mode: &TerminalMode) -> Color {
    match mode {
        TerminalMode::Normal => Color::Green,
        TerminalMode::ExecutingCommand => Color::Yellow,
        TerminalMode::WaitingLLM => Color::Blue,
        TerminalMode::PromptingInstall => Color::Magenta,
        TerminalMode::AwaitingCommandApproval => Color::Cyan,
        TerminalMode::AwaitingAnswer => Color::Cyan,
        TerminalMode::AwaitingMoreInput(_) => Color::Magenta,
    }
}
