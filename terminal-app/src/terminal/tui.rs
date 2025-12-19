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
    let visible_height = area.height as usize;

    // === PRE-CALCULATE SCROLL (before building lines) ===
    // Count interaction lines that will be added
    let interaction_line_count = count_interaction_lines(&state.pending_interaction);

    // Calculate prompt width for wrapped line calculation
    let prompt_for_wrap = build_prompt_text(state);
    let prompt_width_for_wrap = prompt_for_wrap.width();
    let terminal_width = area.width as usize;

    // Calculate how many visual lines the prompt+input will take when wrapped
    let (prompt_lines, _, _) = state
        .input
        .calculate_wrapped_cursor(prompt_width_for_wrap, terminal_width);
    let extra_lines = interaction_line_count + prompt_lines;

    // Update OutputBuffer with layout info for scroll calculations
    let output_line_count = state.output.total_lines();
    state.output.set_extra_lines(extra_lines);
    state.output.set_visible_lines(visible_height);

    // Calculate total content and scroll
    let total_lines = output_line_count + extra_lines;
    let max_scroll = total_lines.saturating_sub(visible_height);
    let scroll_position = state.output.scroll_position();
    let effective_scroll = scroll_position.min(max_scroll);

    // Sync clamped scroll position back to buffer
    if scroll_position != effective_scroll {
        state.output.set_scroll_position_exact(effective_scroll);
    }

    // === BUILD ONLY VISIBLE LINES (optimization: no full clone) ===
    // Calculate which output lines are visible after scroll
    let output_start = effective_scroll.min(output_line_count);
    let output_end = (effective_scroll + visible_height).min(output_line_count);

    // Clone only the visible output lines (not the entire buffer!)
    let mut all_lines: Vec<Line> = state.output.parsed_lines()[output_start..output_end].to_vec();

    // Add approval flow lines if pending (only show command, not message)
    if let Some(interaction) = &state.pending_interaction {
        match interaction {
            crate::terminal::PendingInteraction::CommandApproval { command, .. } => {
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

    // Build prompt text (reuse the helper function)
    let prompt = build_prompt_text(state);
    let prompt_color = get_prompt_color(&state.mode);
    let prompt_style = Style::default()
        .fg(prompt_color)
        .add_modifier(Modifier::BOLD);

    let input = if is_password_mode {
        "*".repeat(state.input.text().len())
    } else {
        state.input.text().to_string()
    };

    // Pre-calculate prompt width for cursor positioning
    let prompt_width = prompt.width();

    // Build wrapped lines for prompt + input
    let wrapped_lines = build_wrapped_input_lines(&prompt, &input, terminal_width, prompt_style);
    all_lines.extend(wrapped_lines);

    let needs_scrollbar = total_lines > visible_height;

    // === RENDER CONTENT (no scroll needed - already sliced) ===
    let content_paragraph = Paragraph::new(all_lines);
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
    // Calculate cursor position considering line wrapping
    let (total_input_lines, cursor_row_in_input, cursor_col) = state
        .input
        .calculate_wrapped_cursor(prompt_width, terminal_width);

    // Calculate where the first prompt line is in total content
    let prompt_start_in_total = total_lines.saturating_sub(total_input_lines);

    // Calculate which line of the prompt the cursor is on
    let cursor_line_in_total = prompt_start_in_total + cursor_row_in_input;

    // Check if cursor line is visible
    let cursor_is_visible = cursor_line_in_total >= effective_scroll
        && cursor_line_in_total < effective_scroll + visible_height;

    if cursor_is_visible {
        let cursor_screen_row = cursor_line_in_total - effective_scroll;

        let cursor_x = area.x.saturating_add(cursor_col as u16);
        let cursor_y = area.y.saturating_add(cursor_screen_row as u16);

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

/// Build prompt text based on terminal state (extracted for reuse)
fn build_prompt_text(state: &TerminalState) -> String {
    let is_password_mode =
        if let Some(crate::terminal::PendingInteraction::Question { question, .. }) =
            &state.pending_interaction
        {
            question.contains("[sudo] password")
        } else {
            false
        };

    if state.pending_interaction.is_some() {
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
            // Skip "|~| " (4 chars: pipe, tilde, pipe, space)
            format!("{} {}", prefix, &base_prompt[4..])
        } else {
            base_prompt
        }
    }
}

/// Build wrapped input lines for prompt + input that exceed terminal width
fn build_wrapped_input_lines(
    prompt: &str,
    input: &str,
    terminal_width: usize,
    prompt_style: Style,
) -> Vec<Line<'static>> {
    let prompt_width = prompt.width();
    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut current_spans: Vec<Span<'static>> = Vec::new();
    let mut current_width: usize;

    // Add prompt to first line
    current_spans.push(Span::styled(prompt.to_string(), prompt_style));
    current_width = prompt_width;

    // Accumulate input characters, building spans
    let mut current_text = String::new();

    for c in input.chars() {
        let char_width = c.width().unwrap_or(1);

        // Check if we need to wrap
        if current_width + char_width > terminal_width && terminal_width > 0 {
            // Flush current text span if any
            if !current_text.is_empty() {
                current_spans.push(Span::raw(current_text.clone()));
                current_text.clear();
            }
            // Push current line and start new one
            lines.push(Line::from(std::mem::take(&mut current_spans)));
            current_width = 0;
        }

        current_text.push(c);
        current_width += char_width;
    }

    // Flush remaining text
    if !current_text.is_empty() {
        current_spans.push(Span::raw(current_text));
    }

    // Push final line (even if empty, to show cursor position)
    if !current_spans.is_empty() || lines.is_empty() {
        lines.push(Line::from(current_spans));
    }

    lines
}

/// Count how many lines the interaction display will take
fn count_interaction_lines(
    pending_interaction: &Option<crate::terminal::PendingInteraction>,
) -> usize {
    match pending_interaction {
        None => 0,
        Some(crate::terminal::PendingInteraction::CommandApproval { .. }) => {
            // Only command line (message is not displayed)
            1
        }
        Some(crate::terminal::PendingInteraction::Question { question, options }) => {
            // Skip display for password prompts
            if question.contains("[sudo] password") {
                return 0;
            }
            // question line + option lines
            let option_count = options.as_ref().map_or(0, |opts| opts.len());
            1 + option_count
        }
    }
}
