use anyhow::{Context, Result};
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
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
    widgets::{Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState},
    Frame, Terminal,
};
use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use super::state::{ScrollbarInfo, TerminalMode, TerminalState};

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
        // Enable mouse capture for scroll wheel support
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
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
    pub fn render(&mut self, state: &mut TerminalState) -> Result<()> {
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
        // Disable mouse capture before leaving alternate screen
        execute!(
            self.terminal.backend_mut(),
            DisableMouseCapture,
            LeaveAlternateScreen
        )?;
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

/// Render unified content area with inline prompt and optional scrollbar
///
/// Architecture: Linux shell style - output and prompt in SAME scrollable area
/// - Prompt is the last line of content (not a separate fixed area)
/// - After clear, prompt appears at TOP (not bottom)
/// - Content starts at top, grows downward
/// - Scrollbar only when content exceeds viewport
fn render_unified_content(
    frame: &mut Frame,
    area: ratatui::layout::Rect,
    state: &mut TerminalState,
) {
    // === BUILD ALL CONTENT LINES (output + interaction + prompt) ===
    let mut all_lines: Vec<Line> = state.output.parsed_lines().to_vec();
    let output_line_count = all_lines.len();

    // Add approval flow lines if pending
    if let Some(interaction) = &state.pending_interaction {
        match interaction {
            crate::terminal::PendingInteraction::CommandApproval {
                command, message, ..
            } => {
                if !message.is_empty() {
                    all_lines.push(Line::from(message.as_str()));
                }
                all_lines.push(Line::from(Span::styled(
                    format!("command: {}", command),
                    Style::default().fg(Color::Yellow),
                )));
            }
            crate::terminal::PendingInteraction::Question { question, options } => {
                if !question.contains("[sudo] password") {
                    all_lines.push(Line::from(question.as_str()));
                    if let Some(opts) = options {
                        for opt in opts {
                            all_lines.push(Line::from(format!("  - {}", opt)));
                        }
                    }
                }
            }
        }
    }

    // Check if we're in password input mode
    let is_password_mode =
        if let Some(crate::terminal::PendingInteraction::Question { question, .. }) =
            &state.pending_interaction
        {
            question.contains("[sudo] password")
        } else {
            false
        };

    // Build prompt text
    let prompt = if state.pending_interaction.is_some() {
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
        "> ".to_string()
    } else {
        let base_prompt = state.get_prompt();
        let prefix = state.get_prompt_prefix();
        if base_prompt.starts_with("|~|") {
            format!("{}{}", prefix, &base_prompt[5..])
        } else {
            base_prompt
        }
    };

    let input = if is_password_mode {
        "*".repeat(state.input.text().len())
    } else {
        state.input.text().to_string()
    };

    let prompt_color = get_prompt_color(&state.mode);

    // Pre-calculate widths for cursor positioning (avoids cloning prompt/input)
    let prompt_width = prompt.width();
    let char_idx = state.input.cursor_position();
    // Calculate input width WITHOUT allocating a String (O(N) iteration but no allocation)
    let input_width = if is_password_mode {
        char_idx
    } else {
        state
            .input
            .text()
            .chars()
            .take(char_idx)
            .map(|c| c.width().unwrap_or(0))
            .sum()
    };

    // Prompt is the LAST line of all_lines (part of scrollable content)
    let prompt_line = Line::from(vec![
        Span::styled(
            prompt,
            Style::default()
                .fg(prompt_color)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(input),
    ]);
    all_lines.push(prompt_line);

    // === CALCULATE SCROLL ===
    let total_lines = all_lines.len();
    let visible_height = area.height as usize;

    // Calculate extra lines (everything added after output: interaction + prompt)
    let extra_lines = total_lines.saturating_sub(output_line_count);

    // Update OutputBuffer for scroll calculations
    state.output.set_extra_lines(extra_lines);
    state.output.set_visible_lines(visible_height);

    // Calculate scroll position
    // Linux shell behavior: content starts at top, scrolls when exceeds viewport
    let scroll_position = state.output.scroll_position();
    let max_scroll = total_lines.saturating_sub(visible_height);
    let effective_scroll = scroll_position.min(max_scroll);

    // Sync clamped scroll position back to buffer
    // This is needed because scroll_to_end() sets usize::MAX
    // Use set_scroll_position_exact to avoid double-clamping
    if scroll_position != effective_scroll {
        state.output.set_scroll_position_exact(effective_scroll);
    }

    let needs_scrollbar = total_lines > visible_height;

    // === RENDER CONTENT WITH SCROLL ===
    let content_paragraph = Paragraph::new(all_lines)
        .scroll((effective_scroll as u16, 0));

    frame.render_widget(content_paragraph, area);

    // === RENDER SCROLLBAR ===
    if needs_scrollbar {
        state.scrollbar_info = Some(ScrollbarInfo {
            column: area.x + area.width.saturating_sub(1),
            height: area.height,
            total_lines,
            visible_lines: visible_height,
        });

        // ScrollbarState configuration:
        // Ratatui calculates thumb position as: position / content_length
        // To get correct behavior (thumb at bottom when scrolled to end):
        // - content_length = max_scroll + 1 (total scrollable positions: 0..=max_scroll)
        // - position = effective_scroll
        // This ensures: when effective_scroll == max_scroll, position/content_length ≈ 100%
        //
        // For thumb SIZE, we use viewport_content_length relative to total content
        // But since content_length is now max_scroll+1, we calculate thumb size separately
        let scrollbar_content_length = max_scroll.max(1); // Avoid division by zero
        let mut scrollbar_state = ScrollbarState::default()
            .content_length(scrollbar_content_length)
            .position(effective_scroll);

        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(Some("↑"))
            .end_symbol(Some("↓"));

        frame.render_stateful_widget(scrollbar, area, &mut scrollbar_state);
    } else {
        state.scrollbar_info = None;
    }

    // === POSITION CURSOR ===
    // Cursor is on the prompt line (last line of content)
    // Need to calculate where prompt line appears on screen

    let prompt_line_index = total_lines.saturating_sub(1); // 0-indexed
    let prompt_screen_row = prompt_line_index.saturating_sub(effective_scroll);

    // Only show cursor if prompt is visible
    // (prompt_width and input_width pre-calculated above to avoid cloning)
    if prompt_screen_row < visible_height {
        let total_width = prompt_width + input_width;
        let max_x = area.width.saturating_sub(1) as usize;
        let safe_x = total_width.min(max_x);

        let cursor_x = area.x.saturating_add(safe_x as u16);
        let cursor_y = area.y.saturating_add(prompt_screen_row as u16);

        frame.set_cursor_position((cursor_x, cursor_y));
    }
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
