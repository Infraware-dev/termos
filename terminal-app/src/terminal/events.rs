use anyhow::Result;
/// Event handling for keyboard input
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use std::time::Duration;

/// Event handler for terminal input
#[derive(Debug)]
pub struct EventHandler {
    // Configuration can be added here later
}

impl EventHandler {
    pub const fn new() -> Self {
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
        // IMPORTANT: On Windows, crossterm generates multiple events per keystroke
        // (Press, Repeat, Release). We only want to handle Press events to avoid
        // duplicate input. This fixes the double-input issue on Windows.
        if key.kind != KeyEventKind::Press {
            return TerminalEvent::Unknown;
        }

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

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

    fn create_key_event(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, modifiers)
    }

    fn create_key_event_with_kind(
        code: KeyCode,
        modifiers: KeyModifiers,
        kind: KeyEventKind,
    ) -> KeyEvent {
        KeyEvent::new_with_kind(code, modifiers, kind)
    }

    #[test]
    fn test_input_char_lowercase() {
        let handler = EventHandler::new();
        let event = create_key_event(KeyCode::Char('a'), KeyModifiers::NONE);
        let result = handler.map_key_event(event);
        assert!(matches!(result, TerminalEvent::InputChar('a')));
    }

    #[test]
    fn test_input_char_uppercase() {
        let handler = EventHandler::new();
        let event = create_key_event(KeyCode::Char('A'), KeyModifiers::SHIFT);
        let result = handler.map_key_event(event);
        assert!(matches!(result, TerminalEvent::InputChar('A')));
    }

    #[test]
    fn test_input_char_number() {
        let handler = EventHandler::new();
        let event = create_key_event(KeyCode::Char('5'), KeyModifiers::NONE);
        let result = handler.map_key_event(event);
        assert!(matches!(result, TerminalEvent::InputChar('5')));
    }

    #[test]
    fn test_submit_on_enter() {
        let handler = EventHandler::new();
        let event = create_key_event(KeyCode::Enter, KeyModifiers::NONE);
        let result = handler.map_key_event(event);
        assert!(matches!(result, TerminalEvent::Submit));
    }

    #[test]
    fn test_delete_on_backspace() {
        let handler = EventHandler::new();
        let event = create_key_event(KeyCode::Backspace, KeyModifiers::NONE);
        let result = handler.map_key_event(event);
        assert!(matches!(result, TerminalEvent::DeleteChar));
    }

    #[test]
    fn test_tab_completion() {
        let handler = EventHandler::new();
        let event = create_key_event(KeyCode::Tab, KeyModifiers::NONE);
        let result = handler.map_key_event(event);
        assert!(matches!(result, TerminalEvent::TabComplete));
    }

    #[test]
    fn test_arrow_up() {
        let handler = EventHandler::new();
        let event = create_key_event(KeyCode::Up, KeyModifiers::NONE);
        let result = handler.map_key_event(event);
        assert!(matches!(result, TerminalEvent::HistoryPrevious));
    }

    #[test]
    fn test_arrow_down() {
        let handler = EventHandler::new();
        let event = create_key_event(KeyCode::Down, KeyModifiers::NONE);
        let result = handler.map_key_event(event);
        assert!(matches!(result, TerminalEvent::HistoryNext));
    }

    #[test]
    fn test_arrow_left() {
        let handler = EventHandler::new();
        let event = create_key_event(KeyCode::Left, KeyModifiers::NONE);
        let result = handler.map_key_event(event);
        assert!(matches!(result, TerminalEvent::MoveCursorLeft));
    }

    #[test]
    fn test_arrow_right() {
        let handler = EventHandler::new();
        let event = create_key_event(KeyCode::Right, KeyModifiers::NONE);
        let result = handler.map_key_event(event);
        assert!(matches!(result, TerminalEvent::MoveCursorRight));
    }

    #[test]
    fn test_page_up() {
        let handler = EventHandler::new();
        let event = create_key_event(KeyCode::PageUp, KeyModifiers::NONE);
        let result = handler.map_key_event(event);
        assert!(matches!(result, TerminalEvent::ScrollUp));
    }

    #[test]
    fn test_page_down() {
        let handler = EventHandler::new();
        let event = create_key_event(KeyCode::PageDown, KeyModifiers::NONE);
        let result = handler.map_key_event(event);
        assert!(matches!(result, TerminalEvent::ScrollDown));
    }

    #[test]
    fn test_ctrl_c_quit() {
        let handler = EventHandler::new();
        let event = create_key_event(KeyCode::Char('c'), KeyModifiers::CONTROL);
        let result = handler.map_key_event(event);
        assert!(matches!(result, TerminalEvent::Quit));
    }

    #[test]
    fn test_ctrl_d_quit() {
        let handler = EventHandler::new();
        let event = create_key_event(KeyCode::Char('d'), KeyModifiers::CONTROL);
        let result = handler.map_key_event(event);
        assert!(matches!(result, TerminalEvent::Quit));
    }

    #[test]
    fn test_ctrl_l_clear() {
        let handler = EventHandler::new();
        let event = create_key_event(KeyCode::Char('l'), KeyModifiers::CONTROL);
        let result = handler.map_key_event(event);
        assert!(matches!(result, TerminalEvent::ClearScreen));
    }

    #[test]
    fn test_windows_event_filter_release() {
        let handler = EventHandler::new();
        let event = create_key_event_with_kind(
            KeyCode::Char('a'),
            KeyModifiers::NONE,
            KeyEventKind::Release,
        );
        let result = handler.map_key_event(event);
        assert!(matches!(result, TerminalEvent::Unknown));
    }

    #[test]
    fn test_windows_event_filter_repeat() {
        let handler = EventHandler::new();
        let event = create_key_event_with_kind(
            KeyCode::Char('a'),
            KeyModifiers::NONE,
            KeyEventKind::Repeat,
        );
        let result = handler.map_key_event(event);
        assert!(matches!(result, TerminalEvent::Unknown));
    }

    #[test]
    fn test_windows_event_filter_press() {
        let handler = EventHandler::new();
        let event =
            create_key_event_with_kind(KeyCode::Char('a'), KeyModifiers::NONE, KeyEventKind::Press);
        let result = handler.map_key_event(event);
        assert!(matches!(result, TerminalEvent::InputChar('a')));
    }

    #[test]
    fn test_map_event_key() {
        let handler = EventHandler::new();
        let key_event = create_key_event(KeyCode::Enter, KeyModifiers::NONE);
        let event = Event::Key(key_event);
        let result = handler.map_event(event);
        assert!(matches!(result, TerminalEvent::Submit));
    }

    #[test]
    fn test_map_event_resize() {
        let handler = EventHandler::new();
        let event = Event::Resize(80, 24);
        let result = handler.map_event(event);
        assert!(matches!(result, TerminalEvent::Resize(80, 24)));
    }

    #[test]
    fn test_map_event_unknown() {
        let handler = EventHandler::new();
        // Mouse events should be mapped to Unknown
        let event = Event::Mouse(crossterm::event::MouseEvent {
            kind: crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left),
            column: 0,
            row: 0,
            modifiers: KeyModifiers::NONE,
        });
        let result = handler.map_event(event);
        assert!(matches!(result, TerminalEvent::Unknown));
    }

    #[test]
    fn test_default_trait() {
        let handler = EventHandler::default();
        let event = create_key_event(KeyCode::Enter, KeyModifiers::NONE);
        let result = handler.map_key_event(event);
        assert!(matches!(result, TerminalEvent::Submit));
    }

    #[test]
    fn test_char_with_ctrl_modifier_unknown() {
        let handler = EventHandler::new();
        // Ctrl+A should not produce InputChar (not in our mapping)
        let event = create_key_event(KeyCode::Char('a'), KeyModifiers::CONTROL);
        let result = handler.map_key_event(event);
        assert!(matches!(result, TerminalEvent::Unknown));
    }

    #[test]
    fn test_special_characters() {
        let handler = EventHandler::new();

        // Test space
        let event = create_key_event(KeyCode::Char(' '), KeyModifiers::NONE);
        let result = handler.map_key_event(event);
        assert!(matches!(result, TerminalEvent::InputChar(' ')));

        // Test symbols with SHIFT
        let event = create_key_event(KeyCode::Char('!'), KeyModifiers::SHIFT);
        let result = handler.map_key_event(event);
        assert!(matches!(result, TerminalEvent::InputChar('!')));
    }
}
