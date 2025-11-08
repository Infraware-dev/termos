use anyhow::Result;
/// Event handling for keyboard input
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use std::time::Duration;

/// Event handler for terminal input
pub struct EventHandler {
    // Configuration can be added here later
}

impl EventHandler {
    pub fn new() -> Self {
        Self {}
    }

    /// Poll for an event with a timeout
    pub fn poll_event(&self, timeout: Duration) -> Result<Option<TerminalEvent>> {
        if event::poll(timeout)? {
            let event = event::read()?;
            Ok(Some(self.map_event(event)))
        } else {
            Ok(None)
        }
    }

    /// Map crossterm events to our custom events
    fn map_event(&self, event: Event) -> TerminalEvent {
        match event {
            Event::Key(key_event) => self.map_key_event(key_event),
            Event::Resize(width, height) => TerminalEvent::Resize(width, height),
            _ => TerminalEvent::Unknown,
        }
    }

    /// Map key events to terminal events
    fn map_key_event(&self, key: KeyEvent) -> TerminalEvent {
        match (key.code, key.modifiers) {
            // Ctrl+C - Quit
            (KeyCode::Char('c'), KeyModifiers::CONTROL) => TerminalEvent::Quit,
            // Ctrl+D - Quit
            (KeyCode::Char('d'), KeyModifiers::CONTROL) => TerminalEvent::Quit,
            // Ctrl+L - Clear screen
            (KeyCode::Char('l'), KeyModifiers::CONTROL) => TerminalEvent::ClearScreen,

            // Enter - Submit input
            (KeyCode::Enter, _) => TerminalEvent::Submit,

            // Backspace - Delete character
            (KeyCode::Backspace, _) => TerminalEvent::DeleteChar,

            // Arrow keys
            (KeyCode::Up, _) => TerminalEvent::HistoryPrevious,
            (KeyCode::Down, _) => TerminalEvent::HistoryNext,
            (KeyCode::Left, _) => TerminalEvent::MoveCursorLeft,
            (KeyCode::Right, _) => TerminalEvent::MoveCursorRight,

            // Page Up/Down - Scroll output
            (KeyCode::PageUp, _) => TerminalEvent::ScrollUp,
            (KeyCode::PageDown, _) => TerminalEvent::ScrollDown,

            // Tab - Tab completion (for later implementation)
            (KeyCode::Tab, _) => TerminalEvent::TabComplete,

            // Character input (only without modifiers or with SHIFT only)
            (KeyCode::Char(c), KeyModifiers::NONE) => TerminalEvent::InputChar(c),
            (KeyCode::Char(c), KeyModifiers::SHIFT) => TerminalEvent::InputChar(c),

            _ => TerminalEvent::Unknown,
        }
    }
}

impl Default for EventHandler {
    fn default() -> Self {
        Self::new()
    }
}

/// Custom terminal events
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum TerminalEvent {
    /// User input character
    InputChar(char),
    /// Submit current input
    Submit,
    /// Delete character before cursor
    DeleteChar,
    /// Move cursor left
    MoveCursorLeft,
    /// Move cursor right
    MoveCursorRight,
    /// Navigate to previous command in history
    HistoryPrevious,
    /// Navigate to next command in history
    HistoryNext,
    /// Scroll output up
    ScrollUp,
    /// Scroll output down
    ScrollDown,
    /// Tab completion
    TabComplete,
    /// Clear screen
    ClearScreen,
    /// Quit application
    Quit,
    /// Terminal resized (width, height) - M2/M3 feature
    Resize(u16, u16),
    /// Unknown event
    Unknown,
}
