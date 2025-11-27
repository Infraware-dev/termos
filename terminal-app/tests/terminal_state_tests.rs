/// Tests for terminal state management using the public API
use infraware_terminal::terminal::state::{TerminalMode, TerminalState};

// =============================================================================
// TerminalState Tests
// =============================================================================

#[test]
fn test_terminal_state_creation() {
    let state = TerminalState::new();

    // Initial mode should be Normal
    assert_eq!(state.mode, TerminalMode::Normal);

    // Input should be empty
    assert!(state.input.text().is_empty());

    // Output should be empty
    assert!(state.output.lines().is_empty());
}

#[test]
fn test_terminal_state_default() {
    let state = TerminalState::default();
    assert_eq!(state.mode, TerminalMode::Normal);
}

#[test]
fn test_terminal_state_add_output() {
    let mut state = TerminalState::new();

    state.add_output("Line 1".to_string());
    assert_eq!(state.output.lines().len(), 1);
    assert_eq!(state.output.lines()[0], "Line 1");

    state.add_output("Line 2".to_string());
    assert_eq!(state.output.lines().len(), 2);
}

#[test]
fn test_terminal_state_add_output_lines() {
    let mut state = TerminalState::new();

    state.add_output_lines(vec![
        "First".to_string(),
        "Second".to_string(),
        "Third".to_string(),
    ]);

    assert_eq!(state.output.lines().len(), 3);
    assert_eq!(state.output.lines()[0], "First");
    assert_eq!(state.output.lines()[2], "Third");
}

#[test]
fn test_terminal_state_input_operations() {
    let mut state = TerminalState::new();

    // Insert characters
    state.insert_char('h');
    state.insert_char('e');
    state.insert_char('l');
    state.insert_char('l');
    state.insert_char('o');

    assert_eq!(state.input.text(), "hello");

    // Delete character
    state.delete_char();
    assert_eq!(state.input.text(), "hell");

    // Clear input
    state.clear_input();
    assert!(state.input.text().is_empty());
}

#[test]
fn test_terminal_state_cursor_movement() {
    let mut state = TerminalState::new();

    state.insert_char('a');
    state.insert_char('b');
    state.insert_char('c');

    // Cursor should be at the end (position 3)
    assert_eq!(state.input.cursor_position(), 3);

    // Move left
    state.move_cursor_left();
    assert_eq!(state.input.cursor_position(), 2);

    // Move left again
    state.move_cursor_left();
    assert_eq!(state.input.cursor_position(), 1);

    // Move right
    state.move_cursor_right();
    assert_eq!(state.input.cursor_position(), 2);
}

#[test]
fn test_terminal_state_submit_input() {
    let mut state = TerminalState::new();

    state.insert_char('l');
    state.insert_char('s');

    let submitted = state.submit_input();

    assert_eq!(submitted, "ls");
    assert!(state.input.text().is_empty()); // Input cleared after submit
}

#[test]
fn test_terminal_state_history_navigation() {
    let mut state = TerminalState::new();

    // Add some commands to history
    state.insert_char('l');
    state.insert_char('s');
    state.submit_input();

    state.insert_char('p');
    state.insert_char('w');
    state.insert_char('d');
    state.submit_input();

    state.insert_char('c');
    state.insert_char('d');
    state.submit_input();

    // Now navigate history
    state.history_previous();
    assert_eq!(state.input.text(), "cd");

    state.history_previous();
    assert_eq!(state.input.text(), "pwd");

    state.history_previous();
    assert_eq!(state.input.text(), "ls");

    // Go forward
    state.history_next();
    assert_eq!(state.input.text(), "pwd");

    state.history_next();
    assert_eq!(state.input.text(), "cd");

    // Going next from end should clear input
    state.history_next();
    assert!(state.input.text().is_empty());
}

#[test]
fn test_terminal_state_scroll() {
    let mut state = TerminalState::new();

    // Add many lines
    for i in 0..100 {
        state.add_output(format!("Line {}", i));
    }

    // Auto-scrolls to bottom, position should be 99
    assert_eq!(state.output.scroll_position(), 99);

    // Scroll up
    state.scroll_up();
    assert_eq!(state.output.scroll_position(), 98);

    state.scroll_up();
    assert_eq!(state.output.scroll_position(), 97);

    // Scroll down
    state.scroll_down();
    assert_eq!(state.output.scroll_position(), 98);
}

#[test]
fn test_terminal_state_mode_changes() {
    let mut state = TerminalState::new();

    assert_eq!(state.mode, TerminalMode::Normal);

    state.mode = TerminalMode::ExecutingCommand;
    assert_eq!(state.mode, TerminalMode::ExecutingCommand);

    state.mode = TerminalMode::WaitingLLM;
    assert_eq!(state.mode, TerminalMode::WaitingLLM);

    state.mode = TerminalMode::PromptingInstall;
    assert_eq!(state.mode, TerminalMode::PromptingInstall);
}

// =============================================================================
// TerminalMode Tests
// =============================================================================

#[test]
fn test_terminal_mode_debug() {
    assert_eq!(format!("{:?}", TerminalMode::Normal), "Normal");
    assert_eq!(
        format!("{:?}", TerminalMode::ExecutingCommand),
        "ExecutingCommand"
    );
    assert_eq!(format!("{:?}", TerminalMode::WaitingLLM), "WaitingLLM");
    assert_eq!(
        format!("{:?}", TerminalMode::PromptingInstall),
        "PromptingInstall"
    );
}

#[test]
fn test_terminal_mode_clone() {
    let mode = TerminalMode::ExecutingCommand;
    let cloned = mode.clone();
    assert_eq!(mode, cloned);
}

#[test]
fn test_terminal_mode_equality() {
    assert_eq!(TerminalMode::Normal, TerminalMode::Normal);
    assert_ne!(TerminalMode::Normal, TerminalMode::WaitingLLM);
}

// =============================================================================
// Integration Tests - Simulating User Interactions
// =============================================================================

#[test]
fn test_typical_user_workflow() {
    let mut state = TerminalState::new();

    // User types a command
    for c in "docker ps".chars() {
        state.insert_char(c);
    }
    assert_eq!(state.input.text(), "docker ps");

    // User submits command
    let cmd = state.submit_input();
    assert_eq!(cmd, "docker ps");
    assert!(state.input.text().is_empty());

    // System shows output
    state.add_output("CONTAINER ID   IMAGE    COMMAND".to_string());
    state.add_output("abc123         nginx    nginx -g".to_string());
    assert_eq!(state.output.lines().len(), 2);

    // User types another command
    for c in "docker images".chars() {
        state.insert_char(c);
    }

    // User makes a typo and corrects it
    state.delete_char(); // remove 's'
    state.insert_char('s');

    // User navigates history
    state.history_previous();
    assert_eq!(state.input.text(), "docker ps");

    state.history_next();
    assert!(state.input.text().is_empty());
}

#[test]
fn test_cursor_editing() {
    let mut state = TerminalState::new();

    // Type "hello world"
    for c in "hello world".chars() {
        state.insert_char(c);
    }

    // Move cursor to the middle
    for _ in 0..5 {
        state.move_cursor_left();
    }

    // Insert text at cursor position
    state.insert_char('!');
    assert_eq!(state.input.text(), "hello !world");

    // Delete character before cursor
    state.delete_char();
    assert_eq!(state.input.text(), "hello world");
}

#[test]
fn test_scroll_boundary_conditions() {
    let mut state = TerminalState::new();

    // Add a few lines
    state.add_output("Line 0".to_string());
    state.add_output("Line 1".to_string());
    state.add_output("Line 2".to_string());

    // Auto-scrolls to bottom (position 2)
    assert_eq!(state.output.scroll_position(), 2);

    // Scroll up to top
    state.scroll_up();
    state.scroll_up();
    assert_eq!(state.output.scroll_position(), 0);

    // Try scrolling past the top (should stay at 0)
    state.scroll_up();
    assert_eq!(state.output.scroll_position(), 0);
}

#[test]
fn test_history_empty_commands_ignored() {
    let mut state = TerminalState::new();

    // Submit empty command (should not be added to history)
    state.submit_input();

    // Add a real command
    state.insert_char('l');
    state.insert_char('s');
    state.submit_input();

    // Navigate history - should only find "ls"
    state.history_previous();
    assert_eq!(state.input.text(), "ls");

    // Going back further should stay at "ls"
    state.history_previous();
    assert_eq!(state.input.text(), "ls");
}

#[test]
fn test_unicode_input() {
    let mut state = TerminalState::new();

    // Type unicode characters
    for c in "你好世界".chars() {
        state.insert_char(c);
    }

    assert_eq!(state.input.text(), "你好世界");
    assert_eq!(state.input.cursor_position(), 4); // 4 characters

    // Move cursor and delete
    state.move_cursor_left();
    state.delete_char();
    assert_eq!(state.input.text(), "你好界");
    assert_eq!(state.input.cursor_position(), 2);
}

#[test]
fn test_emoji_input() {
    let mut state = TerminalState::new();

    // Type emoji
    state.insert_char('😀');
    state.insert_char('🎉');
    state.insert_char('✨');

    assert_eq!(state.input.text(), "😀🎉✨");
    assert_eq!(state.input.cursor_position(), 3);

    state.delete_char();
    assert_eq!(state.input.text(), "😀🎉");
}
