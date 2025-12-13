use anyhow::{Context, Result};
use crossterm::{
    execute,
    terminal::{
        disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen, SetTitle,
    },
};
/// NEW TUI rendering logic using ratatui - Unified inline terminal design
use ratatui::{
    backend::CrosstermBackend,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame, Terminal,
};
use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use unicode_width::UnicodeWidthStr;

use super::state::{TerminalMode, TerminalState};

/// TUI wrapper for the terminal
pub struct TerminalUI {
    terminal: Terminal<CrosstermBackend<io::Stdout>>,
    /// Flag to pause event polling during interactive commands (vim, nano, etc.)
    /// When true, the event polling thread should sleep instead of polling.
    /// This prevents the poller from "stealing" keyboard input from vim/nano.
    event_polling_paused: Arc<AtomicBool>,
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

        Ok(Self {
            terminal,
            event_polling_paused: Arc::new(AtomicBool::new(false)),
        })
    }

    /// Get a clone of the event polling pause flag.
    ///
    /// Pass this to the event polling thread so it can check whether to pause.
    /// The polling thread should sleep instead of calling event::poll() when
    /// this flag is true, to avoid stealing keyboard input from vim/nano/etc.
    pub fn event_polling_pause_flag(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.event_polling_paused)
    }

    /// Render the terminal UI
    /// Updates state.visible_lines based on actual terminal size
    pub fn render(&mut self, state: &mut TerminalState) -> Result<()> {
        // Calculate visible lines from terminal size before rendering
        let size = self.terminal.size()?;

        // Full screen for content (no header bar)
        let visible_lines = size.height as usize;
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

    /// Set the terminal window title
    pub fn set_window_title(&mut self, title: &str) -> Result<()> {
        execute!(self.terminal.backend_mut(), SetTitle(title))?;
        Ok(())
    }

    /// Suspend TUI mode for interactive command execution
    ///
    /// Disables raw mode, leaves alternate screen, shows cursor.
    /// Terminal returns to normal state for interactive commands like vim, less, etc.
    /// Also pauses event polling to prevent the poller from stealing keyboard input.
    pub fn suspend(&mut self) -> Result<()> {
        // FIRST: Pause event polling to prevent poller from stealing keyboard input
        // The poller checks this flag and sleeps instead of calling event::poll()
        self.event_polling_paused.store(true, Ordering::SeqCst);

        // Give poller time to notice the flag and exit its current poll() call
        // event::poll() has a 50ms timeout, so 100ms is sufficient
        std::thread::sleep(std::time::Duration::from_millis(100));

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
    /// Also resumes event polling.
    pub fn resume(&mut self) -> Result<()> {
        // Enable raw mode
        enable_raw_mode()?;

        // Enter alternate screen
        execute!(self.terminal.backend_mut(), EnterAlternateScreen)?;

        // Clear screen to prevent artifacts
        self.terminal.clear()?;

        // LAST: Resume event polling after TUI is fully restored
        self.event_polling_paused.store(false, Ordering::SeqCst);

        Ok(())
    }
}

impl Drop for TerminalUI {
    fn drop(&mut self) {
        let _ = self.cleanup();
    }
}

/// Render a single frame - NEW DESIGN: 2 sections (Header + Unified Content)
fn render_frame(frame: &mut Frame, state: &mut TerminalState) {
    let size = frame.area();

    // Render unified content (output + prompt inline) - full screen
    render_unified_content(frame, size, state);
}

/// Render unified content area with inline prompt
fn render_unified_content(
    frame: &mut Frame,
    area: ratatui::layout::Rect,
    state: &mut TerminalState,
) {
    let mut lines = Vec::new();

    // 1. Add historical output from OutputBuffer (pre-parsed, O(N) not O(N²))
    // ANSI codes were parsed once when added to buffer, not on every render
    lines.extend(state.output.parsed_lines().iter().cloned());

    // 2. Add approval flow inline if pending
    if let Some(interaction) = &state.pending_interaction {
        match interaction {
            crate::terminal::PendingInteraction::CommandApproval {
                command, message, ..
            } => {
                // Show message if present
                if !message.is_empty() {
                    lines.push(Line::from(message.clone()));
                }
                // Show command to execute
                lines.push(Line::from(Span::styled(
                    format!("command: {}", command),
                    Style::default().fg(Color::Yellow),
                )));
            }
            crate::terminal::PendingInteraction::Question { question, options } => {
                // Don't show question text for password prompts (it's in the input prompt)
                if !question.contains("[sudo] password") {
                    // Show question
                    lines.push(Line::from(question.clone()));
                    // Show options if present
                    if let Some(opts) = options {
                        for opt in opts {
                            lines.push(Line::from(format!("  - {}", opt)));
                        }
                    }
                }
            }
        }
    }

    // Check if we're in password input mode (sudo password prompt)
    let is_password_mode =
        if let Some(crate::terminal::PendingInteraction::Question { question, .. }) =
            &state.pending_interaction
        {
            question.contains("[sudo] password")
        } else {
            false
        };

    // 2b. Add current prompt + input inline (with mode-based color)
    let prompt = if state.pending_interaction.is_some() {
        // Use simple approval prompt when pending interaction
        match state.mode {
            TerminalMode::AwaitingCommandApproval => {
                "Do you want to execute this command (y/n)? ".to_string()
            }
            TerminalMode::AwaitingAnswer => {
                if is_password_mode {
                    "[sudo] password: ".to_string()
                } else {
                    "Answer: ".to_string()
                }
            }
            _ => state.get_prompt(),
        }
    } else if matches!(state.mode, TerminalMode::AwaitingMoreInput(_)) {
        // Use continuation prompt for multiline input
        "> ".to_string()
    } else {
        // Normal prompt with dynamic throbber prefix for waiting states
        let base_prompt = state.get_prompt();
        let prefix = state.get_prompt_prefix();
        // Replace static |~| with dynamic prefix (throbber when animating)
        if base_prompt.starts_with("|~|") {
            format!("{}{}", prefix, &base_prompt[5..]) // Skip "|~| " (5 chars including space)
        } else {
            base_prompt
        }
    };

    // Hide input if in password mode, show asterisks instead
    let input = if is_password_mode {
        "*".repeat(state.input.text().len())
    } else {
        state.input.text().to_string()
    };

    let prompt_color = get_prompt_color(&state.mode);
    let current_line = Line::from(vec![
        Span::styled(
            prompt.clone(),
            Style::default()
                .fg(prompt_color)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(input.clone()),
    ]);
    lines.push(current_line);

    // Note: Loading animation is now shown via throbber in the prompt prefix
    // (|~| becomes |⠘| etc. when throbber is active)

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

    // Use Unicode-aware width calculation for proper cursor positioning
    // with emoji and wide characters
    let prompt_width = prompt.width();

    // Calculate visual width of input text up to cursor position
    // cursor_position() returns character index, but we need visual width
    // Example: "😀中a" at cursor position 2 = visual width 4 (emoji=2, CJK=2)
    // In password mode, each character is displayed as '*' (width 1)
    let char_idx = state.input.cursor_position();
    let input_width = if is_password_mode {
        // In password mode, each char is shown as '*' which has width 1
        char_idx
    } else {
        let text_before_cursor = state
            .input
            .text()
            .chars()
            .take(char_idx)
            .collect::<String>();
        text_before_cursor.width()
    };

    let total_width = prompt_width + input_width;

    // Ensure cursor stays within terminal bounds
    let max_x = area.width.saturating_sub(1) as usize;
    let safe_x = total_width.min(max_x);
    let cursor_x = area.x.saturating_add(safe_x as u16);
    let cursor_y = area.y + prompt_line_y;

    frame.set_cursor_position((cursor_x, cursor_y));
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
