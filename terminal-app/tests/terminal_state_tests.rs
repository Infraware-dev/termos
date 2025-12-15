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

    let submitted = state.submit_input(true);

    assert_eq!(submitted, "ls");
    assert!(state.input.text().is_empty()); // Input cleared after submit
}

#[test]
fn test_terminal_state_history_navigation() {
    let mut state = TerminalState::new();

    // Add some commands to history
    state.insert_char('l');
    state.insert_char('s');
    state.submit_input(true);

    state.insert_char('p');
    state.insert_char('w');
    state.insert_char('d');
    state.submit_input(true);

    state.insert_char('c');
    state.insert_char('d');
    state.submit_input(true);

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

    // Set visible lines first (simulating terminal size)
    state.set_visible_lines(20);
    state.output.set_extra_lines(0); // No prompt in unit test

    // Add many lines
    for i in 0..100 {
        state.add_output(format!("Line {}", i));
    }

    // With 100 lines and 20 visible, max_scroll = 100-20 = 80
    // Auto-scrolls to bottom (scroll_position = max_scroll = 80)
    assert_eq!(state.output.scroll_position(), 80);

    // Scroll up
    state.scroll_up();
    assert_eq!(state.output.scroll_position(), 79);

    state.scroll_up();
    assert_eq!(state.output.scroll_position(), 78);

    // Scroll down back toward max
    state.scroll_down();
    assert_eq!(state.output.scroll_position(), 79);

    state.scroll_down();
    assert_eq!(state.output.scroll_position(), 80);

    // Try scrolling past max - should stay at max
    state.scroll_down();
    assert_eq!(state.output.scroll_position(), 80);
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
    let cmd = state.submit_input(true);
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

    // Set visible lines (simulating terminal size)
    state.set_visible_lines(10);

    // Add a few lines (less than visible_lines)
    state.add_output("Line 0".to_string());
    state.add_output("Line 1".to_string());
    state.add_output("Line 2".to_string());

    // With 3 lines and 10 visible, max_scroll = max(0, 3-10) = 0
    // Auto-scrolls to bottom (scroll_position = 0)
    assert_eq!(state.output.scroll_position(), 0);

    // Try scrolling up (already at 0, should stay)
    state.scroll_up();
    assert_eq!(state.output.scroll_position(), 0);

    // Try scrolling down (already at max=0, should stay)
    state.scroll_down();
    assert_eq!(state.output.scroll_position(), 0);
}

#[test]
fn test_history_empty_commands_ignored() {
    let mut state = TerminalState::new();

    // Submit empty command (should not be added to history)
    state.submit_input(true);

    // Add a real command
    state.insert_char('l');
    state.insert_char('s');
    state.submit_input(true);

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

// =============================================================================
// Root Mode Tests
// =============================================================================

#[test]
fn test_root_mode_initial_state() {
    let state = TerminalState::new();
    assert!(!state.is_root_mode());
}

#[test]
fn test_enter_root_mode() {
    let mut state = TerminalState::new();

    // Get prompt before entering root mode
    let prompt_before = state.get_prompt();
    assert!(prompt_before.contains('$')); // Normal user prompt

    // Enter root mode
    state.enter_root_mode();

    assert!(state.is_root_mode());
    let prompt_after = state.get_prompt();
    assert!(prompt_after.contains('#')); // Root prompt
}

#[test]
fn test_exit_root_mode() {
    let mut state = TerminalState::new();

    // Enter and then exit root mode
    state.enter_root_mode();
    assert!(state.is_root_mode());

    state.exit_root_mode();
    assert!(!state.is_root_mode());

    let prompt = state.get_prompt();
    assert!(prompt.contains('$')); // Back to normal user prompt
}

#[test]
fn test_root_mode_toggle() {
    let mut state = TerminalState::new();

    // Toggle root mode multiple times
    state.enter_root_mode();
    assert!(state.is_root_mode());

    state.exit_root_mode();
    assert!(!state.is_root_mode());

    state.enter_root_mode();
    assert!(state.is_root_mode());
}

// =============================================================================
// HITL (Human-in-the-Loop) Mode Tests
// =============================================================================

#[test]
fn test_hitl_mode_initial() {
    let state = TerminalState::new();
    assert!(!state.is_in_hitl_mode());
}

#[test]
fn test_hitl_mode_awaiting_command_approval() {
    let mut state = TerminalState::new();

    state.mode = TerminalMode::AwaitingCommandApproval;
    assert!(state.is_in_hitl_mode());
}

#[test]
fn test_hitl_mode_awaiting_answer() {
    let mut state = TerminalState::new();

    state.mode = TerminalMode::AwaitingAnswer;
    assert!(state.is_in_hitl_mode());
}

#[test]
fn test_hitl_mode_not_in_normal() {
    let mut state = TerminalState::new();

    state.mode = TerminalMode::Normal;
    assert!(!state.is_in_hitl_mode());
}

#[test]
fn test_hitl_mode_not_in_executing() {
    let mut state = TerminalState::new();

    state.mode = TerminalMode::ExecutingCommand;
    assert!(!state.is_in_hitl_mode());
}

// =============================================================================
// Multiline Mode Tests
// =============================================================================

#[test]
fn test_multiline_mode_initial() {
    let state = TerminalState::new();
    assert!(!state.is_in_multiline_mode());
    assert!(state.multiline_buffer.is_empty());
}

#[test]
fn test_multiline_mode_awaiting_more_input() {
    use infraware_terminal::input::IncompleteReason;

    let mut state = TerminalState::new();

    state.mode = TerminalMode::AwaitingMoreInput(IncompleteReason::TrailingBackslash);
    assert!(state.is_in_multiline_mode());
}

#[test]
fn test_cancel_multiline() {
    use infraware_terminal::input::IncompleteReason;

    let mut state = TerminalState::new();

    // Set up multiline state
    state.mode = TerminalMode::AwaitingMoreInput(IncompleteReason::UnclosedDoubleQuote);
    state.multiline_buffer.push("echo 'hello".to_string());
    state.pending_heredoc = Some("EOF".to_string());

    // Cancel multiline
    state.cancel_multiline();

    // Verify state is cleared
    assert_eq!(state.mode, TerminalMode::Normal);
    assert!(state.multiline_buffer.is_empty());
    assert!(state.pending_heredoc.is_none());
}

#[test]
fn test_get_multiline_input() {
    let mut state = TerminalState::new();

    state.multiline_buffer.push("echo \\".to_string());
    state.multiline_buffer.push("hello".to_string());

    let joined = state.get_multiline_input();
    // The join logic removes backslash continuation
    assert!(joined.contains("echo"));
    assert!(joined.contains("hello"));
}

// =============================================================================
// Prompt Tests
// =============================================================================

#[test]
fn test_get_prompt_contains_components() {
    let state = TerminalState::new();
    let prompt = state.get_prompt();

    // Prompt should contain: |~| user@host:path$
    assert!(prompt.contains("|~|"));
    assert!(prompt.contains('@'));
    assert!(prompt.contains(':'));
}

#[test]
fn test_refresh_prompt() {
    let mut state = TerminalState::new();

    let prompt_before = state.get_prompt();
    state.refresh_prompt();
    let prompt_after = state.get_prompt();

    // Prompts should be the same after refresh (in same directory)
    assert_eq!(prompt_before, prompt_after);
}

#[test]
fn test_get_prompt_prefix() {
    let state = TerminalState::new();
    let prefix = state.get_prompt_prefix();

    // Should be |~| when not animating
    assert!(prefix.starts_with('|'));
    assert!(prefix.ends_with('|'));
}

// =============================================================================
// Window Title Tests
// =============================================================================

#[test]
fn test_get_window_title() {
    let state = TerminalState::new();
    let title = state.get_window_title();

    // Title should contain current directory
    assert!(!title.is_empty());
}

// =============================================================================
// Visible Lines Tests
// =============================================================================

#[test]
fn test_set_visible_lines() {
    let mut state = TerminalState::new();

    state.set_visible_lines(50);
    assert_eq!(state.visible_lines(), 50);

    state.set_visible_lines(100);
    assert_eq!(state.visible_lines(), 100);
}

#[test]
fn test_visible_lines_default() {
    let state = TerminalState::new();
    // Default is 0 - set on first render from actual terminal height
    assert_eq!(state.visible_lines(), 0);
}

// =============================================================================
// Throbber Tests
// =============================================================================

#[test]
fn test_throbber_start_stop() {
    let state = TerminalState::new();

    // Start throbber
    state.start_throbber();

    // Stop throbber
    state.stop_throbber();

    // Should not panic
}

// =============================================================================
// TerminalMode Extended Tests
// =============================================================================

#[test]
fn test_terminal_mode_awaiting_more_input() {
    use infraware_terminal::input::IncompleteReason;

    let mode = TerminalMode::AwaitingMoreInput(IncompleteReason::TrailingBackslash);
    let debug_str = format!("{:?}", mode);
    assert!(debug_str.contains("AwaitingMoreInput"));
    assert!(debug_str.contains("TrailingBackslash"));
}

#[test]
fn test_terminal_mode_all_variants() {
    // Test all mode variants
    let modes = vec![
        TerminalMode::Normal,
        TerminalMode::ExecutingCommand,
        TerminalMode::WaitingLLM,
        TerminalMode::PromptingInstall,
        TerminalMode::AwaitingCommandApproval,
        TerminalMode::AwaitingAnswer,
    ];

    for mode in modes {
        let debug_str = format!("{:?}", mode);
        assert!(!debug_str.is_empty());
    }
}

// =============================================================================
// State Debug Tests
// =============================================================================

#[test]
fn test_terminal_state_debug() {
    let state = TerminalState::new();
    let debug_str = format!("{:?}", state);

    assert!(debug_str.contains("TerminalState"));
    assert!(debug_str.contains("output"));
    assert!(debug_str.contains("input"));
    assert!(debug_str.contains("mode"));
}

// =============================================================================
// Throbber Animation Mode-Awareness Tests
// =============================================================================

#[test]
fn test_prompt_prefix_static_in_normal_mode() {
    let state = TerminalState::new();
    // Even if throbber is running, Normal mode shows static tilde
    state.start_throbber();
    std::thread::sleep(std::time::Duration::from_millis(150));

    assert_eq!(state.get_prompt_prefix(), "|~|");
    state.stop_throbber();
}

#[test]
fn test_prompt_prefix_static_in_executing_command_mode() {
    let mut state = TerminalState::new();
    state.mode = TerminalMode::ExecutingCommand;
    state.start_throbber();
    std::thread::sleep(std::time::Duration::from_millis(150));

    // ExecutingCommand mode should show static tilde
    assert_eq!(state.get_prompt_prefix(), "|~|");
    state.stop_throbber();
}

#[test]
fn test_prompt_prefix_animates_in_waiting_llm_mode() {
    let mut state = TerminalState::new();
    state.mode = TerminalMode::WaitingLLM;
    state.start_throbber();
    std::thread::sleep(std::time::Duration::from_millis(150));

    // WaitingLLM mode should show animated symbol
    let prefix = state.get_prompt_prefix();
    assert_ne!(prefix, "|~|", "WaitingLLM mode should show animated symbol");
    assert!(prefix.starts_with('|'));
    assert!(prefix.ends_with('|'));
    state.stop_throbber();
}

#[test]
fn test_prompt_prefix_static_when_throbber_not_running() {
    let mut state = TerminalState::new();
    state.mode = TerminalMode::WaitingLLM;
    // Don't start throbber

    // Even in WaitingLLM mode, if throbber isn't running, show static
    assert_eq!(state.get_prompt_prefix(), "|~|");
}

#[test]
fn test_animation_only_during_waiting_llm() {
    let mut state = TerminalState::new();

    // Start throbber
    state.start_throbber();
    std::thread::sleep(std::time::Duration::from_millis(150));

    // Normal mode - static
    state.mode = TerminalMode::Normal;
    assert_eq!(state.get_prompt_prefix(), "|~|");

    // ExecutingCommand mode - static
    state.mode = TerminalMode::ExecutingCommand;
    assert_eq!(state.get_prompt_prefix(), "|~|");

    // PromptingInstall mode - static
    state.mode = TerminalMode::PromptingInstall;
    assert_eq!(state.get_prompt_prefix(), "|~|");

    // AwaitingCommandApproval mode - static
    state.mode = TerminalMode::AwaitingCommandApproval;
    assert_eq!(state.get_prompt_prefix(), "|~|");

    // AwaitingAnswer mode - static
    state.mode = TerminalMode::AwaitingAnswer;
    assert_eq!(state.get_prompt_prefix(), "|~|");

    // WaitingLLM mode - animated!
    state.mode = TerminalMode::WaitingLLM;
    assert_ne!(state.get_prompt_prefix(), "|~|");

    state.stop_throbber();
}
