/// Tests for command executor
use infraware_terminal::executor::CommandExecutor;
use tokio_util::sync::CancellationToken;

#[tokio::test]
async fn test_execute_simple_command() {
    let result = CommandExecutor::execute(
        "echo",
        &["hello".to_string()],
        None,
        CancellationToken::new(),
    )
    .wait()
    .await
    .unwrap();

    assert!(result.is_success());
    assert_eq!(result.stdout.trim(), "hello");
    assert_eq!(result.exit_code, 0);
}

#[tokio::test]
async fn test_execute_command_with_args() {
    let result = CommandExecutor::execute(
        "echo",
        &["hello".to_string(), "world".to_string()],
        None,
        CancellationToken::new(),
    )
    .wait()
    .await
    .unwrap();

    assert!(result.is_success());
    assert_eq!(result.stdout.trim(), "hello world");
}

#[tokio::test]
async fn test_command_not_found() {
    let result = CommandExecutor::execute(
        "nonexistentcommand12345",
        &[],
        None,
        CancellationToken::new(),
    )
    .wait()
    .await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_command_exists() {
    assert!(CommandExecutor::command_exists("echo"));
    assert!(CommandExecutor::command_exists("ls"));
    assert!(!CommandExecutor::command_exists("nonexistentcommand12345"));
}

#[tokio::test]
async fn test_command_with_failure() {
    // ls with invalid directory should fail
    let result = CommandExecutor::execute(
        "ls",
        &["/nonexistent/directory/path".to_string()],
        None,
        CancellationToken::new(),
    )
    .wait()
    .await
    .unwrap();

    assert!(!result.is_success());
    assert!(result.exit_code != 0);
}

#[test]
fn test_get_command_path() {
    let path = CommandExecutor::get_command_path("echo");
    assert!(path.is_some());
    assert!(path.unwrap().contains("echo"));
}

// =============================================================================
// Interactive Command Detection Tests
// =============================================================================

#[test]
fn test_requires_interactive_vim() {
    assert!(CommandExecutor::requires_interactive("vim"));
    assert!(CommandExecutor::requires_interactive("nvim"));
    assert!(CommandExecutor::requires_interactive("nano"));
}

#[test]
fn test_requires_interactive_pagers() {
    assert!(CommandExecutor::requires_interactive("less"));
    assert!(CommandExecutor::requires_interactive("more"));
    assert!(CommandExecutor::requires_interactive("man"));
}

#[test]
fn test_requires_interactive_system_monitors() {
    assert!(CommandExecutor::requires_interactive("top"));
    assert!(CommandExecutor::requires_interactive("htop"));
}

#[test]
fn test_requires_interactive_file_managers() {
    assert!(CommandExecutor::requires_interactive("mc"));
    assert!(CommandExecutor::requires_interactive("ranger"));
}

#[test]
fn test_requires_interactive_sudo() {
    // sudo is handled via root mode wrapper, not as interactive command
    assert!(!CommandExecutor::requires_interactive("sudo"));
}

#[test]
fn test_requires_interactive_gh() {
    // gh (GitHub CLI) requires interactive for auth commands
    assert!(CommandExecutor::requires_interactive("gh"));
}

#[test]
fn test_not_interactive_common_commands() {
    assert!(!CommandExecutor::requires_interactive("ls"));
    assert!(!CommandExecutor::requires_interactive("cat"));
    assert!(!CommandExecutor::requires_interactive("grep"));
    assert!(!CommandExecutor::requires_interactive("echo"));
    assert!(!CommandExecutor::requires_interactive("docker"));
    assert!(!CommandExecutor::requires_interactive("kubectl"));
}

// =============================================================================
// Command Output Tests
// =============================================================================

#[tokio::test]
async fn test_execute_command_with_pipe() {
    // Test shell interpretation with pipes
    let result = CommandExecutor::execute(
        "sh",
        &[],
        Some("echo hello | cat"),
        CancellationToken::new(),
    )
    .wait()
    .await
    .unwrap();

    assert!(result.is_success());
    assert_eq!(result.stdout.trim(), "hello");
}

#[tokio::test]
async fn test_execute_command_with_redirect() {
    use std::fs;

    // Create temp file path
    let temp_dir = std::env::temp_dir();
    let temp_file = temp_dir.join("infraware_test_redirect.txt");
    let temp_path = temp_file.to_string_lossy().to_string();

    // Clean up any existing file
    let _ = fs::remove_file(&temp_file);

    // Execute with redirect
    let result = CommandExecutor::execute(
        "sh",
        &[],
        Some(&format!("echo test_output > {}", temp_path)),
        CancellationToken::new(),
    )
    .wait()
    .await
    .unwrap();

    assert!(result.is_success());

    // Verify file was created with correct content
    let content = fs::read_to_string(&temp_file).unwrap();
    assert_eq!(content.trim(), "test_output");

    // Clean up
    let _ = fs::remove_file(&temp_file);
}

#[tokio::test]
async fn test_execute_multiline_output() {
    let result = CommandExecutor::execute(
        "sh",
        &[
            "-c".to_string(),
            "echo line1; echo line2; echo line3".to_string(),
        ],
        None,
        CancellationToken::new(),
    )
    .wait()
    .await
    .unwrap();

    assert!(result.is_success());
    let lines: Vec<&str> = result.stdout.lines().collect();
    assert_eq!(lines.len(), 3);
    assert_eq!(lines[0], "line1");
    assert_eq!(lines[1], "line2");
    assert_eq!(lines[2], "line3");
}

#[tokio::test]
async fn test_execute_with_stderr() {
    let result = CommandExecutor::execute(
        "sh",
        &["-c".to_string(), "echo stdout; echo stderr >&2".to_string()],
        None,
        CancellationToken::new(),
    )
    .wait()
    .await
    .unwrap();

    assert!(result.is_success());
    assert_eq!(result.stdout.trim(), "stdout");
    assert_eq!(result.stderr.trim(), "stderr");
}

#[tokio::test]
async fn test_execute_exit_codes() {
    // Exit code 0
    let result = CommandExecutor::execute(
        "sh",
        &["-c".to_string(), "exit 0".to_string()],
        None,
        CancellationToken::new(),
    )
    .wait()
    .await
    .unwrap();
    assert_eq!(result.exit_code, 0);
    assert!(result.is_success());

    // Exit code 1
    let result = CommandExecutor::execute(
        "sh",
        &["-c".to_string(), "exit 1".to_string()],
        None,
        CancellationToken::new(),
    )
    .wait()
    .await
    .unwrap();
    assert_eq!(result.exit_code, 1);
    assert!(!result.is_success());

    // Exit code 42
    let result = CommandExecutor::execute(
        "sh",
        &["-c".to_string(), "exit 42".to_string()],
        None,
        CancellationToken::new(),
    )
    .wait()
    .await
    .unwrap();
    assert_eq!(result.exit_code, 42);
    assert!(!result.is_success());
}

// =============================================================================
// Infinite Output Command Truncation Tests
// =============================================================================

#[tokio::test]
async fn test_yes_command_truncated() {
    // yes is NOT blocked - it's truncated at 1000 lines
    let output = CommandExecutor::execute("yes", &[], None, CancellationToken::new())
        .wait()
        .await
        .unwrap();
    // Process was killed after reaching line limit
    assert!(!output.is_success());
    // Output should contain many "y" lines
    assert!(output.stdout.contains("y"));
    // Output should show truncation message
    assert!(
        output.stdout.contains("[Output truncated at 1000 lines]"),
        "Should show truncation message, got: {}",
        &output.stdout[output.stdout.len().saturating_sub(100)..]
    );
}

#[tokio::test]
async fn test_yes_command_output_limited() {
    let output = CommandExecutor::execute("yes", &[], None, CancellationToken::new())
        .wait()
        .await
        .unwrap();
    // Count actual output lines (excluding truncation message)
    let lines: Vec<&str> = output.stdout.lines().collect();
    // Should be around 1000 lines + truncation message
    assert!(
        lines.len() <= 1003,
        "Output should be limited to ~1000 lines, got {}",
        lines.len()
    );
    assert!(
        lines.len() >= 1000,
        "Output should have at least 1000 lines before truncation"
    );
}

#[tokio::test]
async fn test_cat_dev_zero_blocked() {
    let output = CommandExecutor::execute(
        "cat",
        &["/dev/zero".to_string()],
        None,
        CancellationToken::new(),
    )
    .wait()
    .await
    .unwrap();
    assert!(!output.is_success());
    assert!(
        output.stderr.contains("blocked") || output.stderr.contains("infinite"),
        "Error should mention blocking or infinite: {}",
        output.stderr
    );
}

#[tokio::test]
async fn test_cat_dev_urandom_blocked() {
    let output = CommandExecutor::execute(
        "cat",
        &["/dev/urandom".to_string()],
        None,
        CancellationToken::new(),
    )
    .wait()
    .await
    .unwrap();
    assert!(!output.is_success());
    assert!(output.stderr.contains("blocked") || output.stderr.contains("infinite"));
}

#[tokio::test]
async fn test_cat_normal_file_allowed() {
    // cat of a normal file should work
    let output = CommandExecutor::execute(
        "cat",
        &["/etc/hostname".to_string()],
        None,
        CancellationToken::new(),
    )
    .wait()
    .await
    .unwrap();
    // Should either succeed or fail with "No such file", but NOT be blocked
    assert!(
        !output.stderr.contains("blocked"),
        "Normal cat should not be blocked"
    );
}

#[tokio::test]
async fn test_dd_dev_zero_blocked() {
    let output = CommandExecutor::execute(
        "dd",
        &["if=/dev/zero".to_string()],
        None,
        CancellationToken::new(),
    )
    .wait()
    .await
    .unwrap();
    assert!(!output.is_success());
    assert!(
        output.stderr.contains("blocked") || output.stderr.contains("infinite"),
        "dd with /dev/zero should be blocked"
    );
}

#[tokio::test]
async fn test_dd_normal_usage_allowed() {
    // dd with normal file should not be blocked
    let output = CommandExecutor::execute(
        "dd",
        &[
            "if=/dev/null".to_string(),
            "of=/dev/null".to_string(),
            "count=1".to_string(),
        ],
        None,
        CancellationToken::new(),
    )
    .wait()
    .await
    .unwrap();
    // /dev/null is not in INFINITE_DEVICES, so it should be allowed
    assert!(
        !output.stderr.contains("blocked"),
        "dd with /dev/null should not be blocked"
    );
}

// NOTE: ping without -c is no longer blocked. It runs until:
// - 30 second timeout (LIMITED_COMMAND_TIMEOUT_SECS for non-whitelisted commands)
// - or user presses Ctrl+C
// This test is removed because it would take 30 seconds to complete.
// The behavior is verified by the streaming implementation.

#[tokio::test]
async fn test_ping_with_count_allowed() {
    let output = CommandExecutor::execute(
        "ping",
        &["-c".to_string(), "1".to_string(), "localhost".to_string()],
        None,
        CancellationToken::new(),
    )
    .wait()
    .await
    .unwrap();
    // Should not be blocked - may fail for network reasons, but not blocked
    assert!(
        !output.stderr.contains("blocked"),
        "ping with -c should not be blocked"
    );
}

#[tokio::test]
async fn test_ping_with_deadline_allowed() {
    // ping -w (deadline) should be allowed on Linux
    let output = CommandExecutor::execute(
        "ping",
        &["-w".to_string(), "1".to_string(), "localhost".to_string()],
        None,
        CancellationToken::new(),
    )
    .wait()
    .await
    .unwrap();
    assert!(
        !output.stderr.contains("blocked"),
        "ping with -w deadline should not be blocked"
    );
}

// =============================================================================
// Shell Bypass Prevention Tests
// =============================================================================

#[tokio::test]
async fn test_shell_command_with_infinite_device_blocked() {
    // sh -c "cat /dev/zero" should be blocked
    let output =
        CommandExecutor::execute("sh", &[], Some("cat /dev/zero"), CancellationToken::new())
            .wait()
            .await
            .unwrap();
    assert!(!output.is_success());
    assert!(
        output.stderr.contains("blocked") || output.stderr.contains("infinite"),
        "Shell command with infinite device should be blocked: {}",
        output.stderr
    );
}

#[tokio::test]
async fn test_shell_command_with_pipe_to_head_allowed() {
    // cat /dev/urandom | head -c 10 should be allowed
    let output = CommandExecutor::execute(
        "sh",
        &[],
        Some("cat /dev/urandom | head -c 10"),
        CancellationToken::new(),
    )
    .wait()
    .await
    .unwrap();
    assert!(
        !output.stderr.contains("blocked"),
        "Shell command with head pipe should be allowed"
    );
}

#[tokio::test]
async fn test_dd_with_count_allowed() {
    // dd if=/dev/zero count=1 should be allowed
    let output = CommandExecutor::execute(
        "dd",
        &[
            "if=/dev/zero".to_string(),
            "of=/dev/null".to_string(),
            "bs=1".to_string(),
            "count=1".to_string(),
        ],
        None,
        CancellationToken::new(),
    )
    .wait()
    .await
    .unwrap();
    assert!(
        !output.stderr.contains("blocked"),
        "dd with count= should not be blocked"
    );
}

#[tokio::test]
async fn test_yes_piped_to_head_via_shell_allowed() {
    // yes | head -5 should be allowed (output is limited)
    let output =
        CommandExecutor::execute("sh", &[], Some("yes | head -5"), CancellationToken::new())
            .wait()
            .await
            .unwrap();
    // yes is in INTERACTIVE_BLOCKED but when piped to head via shell it's safe
    // Note: The command may still be blocked due to "yes" being in INTERACTIVE_BLOCKED
    // This test documents the expected behavior
    assert!(
        output.is_success() || !output.stderr.contains("infinite"),
        "yes piped to head should not be blocked for infinite output reasons"
    );
}

// =============================================================================
// Brace Expansion Tests
// =============================================================================

#[tokio::test]
async fn test_brace_expansion_execution() {
    use std::fs;
    use std::path::Path;

    let temp_dir = std::env::temp_dir();
    let base_name = format!("infraware_brace_test_{}", std::process::id());
    let base = temp_dir.join(&base_name);

    // Clean up any previous test files
    for i in 1..=3 {
        let file = format!("{}_{}", base.display(), i);
        let _ = fs::remove_file(&file);
    }

    // Execute with brace expansion via original_input (triggers bash -c)
    let output = CommandExecutor::execute(
        "touch",
        &[],
        Some(&format!("touch {}{{1..3}}", base.display())),
        CancellationToken::new(),
    )
    .wait()
    .await
    .unwrap();

    assert!(
        output.is_success(),
        "Brace expansion command failed: stderr={}",
        output.stderr
    );

    // Verify files were created by bash's brace expansion
    for i in 1..=3 {
        let file = format!("{}{}", base.display(), i);
        assert!(
            Path::new(&file).exists(),
            "File {} should exist after brace expansion",
            file
        );
        // Clean up
        let _ = fs::remove_file(&file);
    }
}

#[tokio::test]
async fn test_comma_brace_expansion() {
    use std::fs;
    use std::path::Path;

    let temp_dir = std::env::temp_dir();
    let base_name = format!("infraware_comma_brace_{}", std::process::id());
    let base = temp_dir.join(&base_name);

    // Clean up any previous test files
    for suffix in ["a", "b", "c"] {
        let file = format!("{}_{}", base.display(), suffix);
        let _ = fs::remove_file(&file);
    }

    // Execute with comma brace expansion {a,b,c}
    let output = CommandExecutor::execute(
        "touch",
        &[],
        Some(&format!("touch {}_{{a,b,c}}", base.display())),
        CancellationToken::new(),
    )
    .wait()
    .await
    .unwrap();

    assert!(
        output.is_success(),
        "Comma brace expansion failed: stderr={}",
        output.stderr
    );

    // Verify files were created
    for suffix in ["a", "b", "c"] {
        let file = format!("{}_{}", base.display(), suffix);
        assert!(
            Path::new(&file).exists(),
            "File {} should exist after comma brace expansion",
            file
        );
        // Clean up
        let _ = fs::remove_file(&file);
    }
}

#[tokio::test]
async fn test_brace_expansion_letter_range() {
    use std::fs;
    use std::path::Path;

    let temp_dir = std::env::temp_dir();
    let base_name = format!("infraware_letter_range_{}", std::process::id());
    let base = temp_dir.join(&base_name);

    // Clean up any previous test files
    for c in 'a'..='c' {
        let file = format!("{}_{}", base.display(), c);
        let _ = fs::remove_file(&file);
    }

    // Execute with letter range brace expansion {a..c}
    let output = CommandExecutor::execute(
        "touch",
        &[],
        Some(&format!("touch {}_{{a..c}}", base.display())),
        CancellationToken::new(),
    )
    .wait()
    .await
    .unwrap();

    assert!(
        output.is_success(),
        "Letter range brace expansion failed: stderr={}",
        output.stderr
    );

    // Verify files were created: file_a, file_b, file_c
    for c in 'a'..='c' {
        let file = format!("{}_{}", base.display(), c);
        assert!(
            Path::new(&file).exists(),
            "File {} should exist after letter range expansion",
            file
        );
        let _ = fs::remove_file(&file);
    }
}

#[tokio::test]
async fn test_brace_expansion_reverse_range() {
    use std::fs;
    use std::path::Path;

    let temp_dir = std::env::temp_dir();
    let base_name = format!("infraware_reverse_range_{}", std::process::id());
    let base = temp_dir.join(&base_name);

    // Clean up any previous test files
    for i in 1..=3 {
        let file = format!("{}_{}", base.display(), i);
        let _ = fs::remove_file(&file);
    }

    // Execute with reverse range brace expansion {3..1}
    let output = CommandExecutor::execute(
        "touch",
        &[],
        Some(&format!("touch {}_{{3..1}}", base.display())),
        CancellationToken::new(),
    )
    .wait()
    .await
    .unwrap();

    assert!(
        output.is_success(),
        "Reverse range brace expansion failed: stderr={}",
        output.stderr
    );

    // Verify files were created: file_3, file_2, file_1 (order doesn't matter for files)
    for i in 1..=3 {
        let file = format!("{}_{}", base.display(), i);
        assert!(
            Path::new(&file).exists(),
            "File {} should exist after reverse range expansion",
            file
        );
        let _ = fs::remove_file(&file);
    }
}

/// Helper to check if bash supports advanced brace expansion (Bash 4.0+ features)
async fn bash_supports_advanced_brace_expansion() -> bool {
    // Test if bash supports zero-padding and step in brace expansion
    let output = CommandExecutor::execute(
        "bash",
        &[],
        Some("bash -c 'echo {01..02}'"),
        CancellationToken::new(),
    )
    .wait()
    .await
    .ok();

    match output {
        Some(out) => out.is_success() && out.stdout.trim() == "01 02",
        None => false,
    }
}

#[tokio::test]
async fn test_brace_expansion_zero_padding() {
    use std::fs;
    use std::path::Path;

    // Skip test if bash doesn't support advanced brace expansion (requires Bash 4.0+)
    if !bash_supports_advanced_brace_expansion().await {
        eprintln!("Skipping test: bash does not support zero-padded brace expansion");
        return;
    }

    let temp_dir = std::env::temp_dir();
    let base_name = format!("infraware_zero_pad_{}", std::process::id());
    let base = temp_dir.join(&base_name);

    // Clean up any previous test files
    for i in 1..=3 {
        let file = format!("{}_{:02}", base.display(), i);
        let _ = fs::remove_file(&file);
    }

    // Execute with zero-padded brace expansion {01..03}
    let output = CommandExecutor::execute(
        "touch",
        &[],
        Some(&format!("touch {}_{{01..03}}", base.display())),
        CancellationToken::new(),
    )
    .wait()
    .await
    .unwrap();

    assert!(
        output.is_success(),
        "Zero-padded brace expansion failed: stderr={}",
        output.stderr
    );

    // Verify files were created: file_01, file_02, file_03
    for i in 1..=3 {
        let file = format!("{}_{:02}", base.display(), i);
        assert!(
            Path::new(&file).exists(),
            "File {} should exist after zero-padded expansion",
            file
        );
        let _ = fs::remove_file(&file);
    }
}

#[tokio::test]
async fn test_brace_expansion_step() {
    use std::fs;
    use std::path::Path;

    // Skip test if bash doesn't support advanced brace expansion (requires Bash 4.0+)
    if !bash_supports_advanced_brace_expansion().await {
        eprintln!("Skipping test: bash does not support step brace expansion");
        return;
    }

    let temp_dir = std::env::temp_dir();
    let base_name = format!("infraware_step_{}", std::process::id());
    let base = temp_dir.join(&base_name);

    let expected = [0, 2, 4];

    // Clean up any previous test files
    for i in &expected {
        let file = format!("{}_{}", base.display(), i);
        let _ = fs::remove_file(&file);
    }

    // Execute with step brace expansion {0..4..2}
    let output = CommandExecutor::execute(
        "touch",
        &[],
        Some(&format!("touch {}_{{0..4..2}}", base.display())),
        CancellationToken::new(),
    )
    .wait()
    .await
    .unwrap();

    assert!(
        output.is_success(),
        "Step brace expansion failed: stderr={}",
        output.stderr
    );

    // Verify files were created: file_0, file_2, file_4
    for i in &expected {
        let file = format!("{}_{}", base.display(), i);
        assert!(
            Path::new(&file).exists(),
            "File {} should exist after step expansion",
            file
        );
        let _ = fs::remove_file(&file);
    }
}

#[tokio::test]
async fn test_brace_expansion_nested() {
    use std::fs;
    use std::path::Path;

    let temp_dir = std::env::temp_dir();
    let base_name = format!("infraware_nested_{}", std::process::id());
    let base = temp_dir.join(&base_name);

    let expected = ["a1", "a2", "b1", "b2"];

    // Clean up any previous test files
    for suffix in &expected {
        let file = format!("{}_{}", base.display(), suffix);
        let _ = fs::remove_file(&file);
    }

    // Execute with nested brace expansion {a,b}{1,2}
    let output = CommandExecutor::execute(
        "touch",
        &[],
        Some(&format!("touch {}_{{a,b}}{{1,2}}", base.display())),
        CancellationToken::new(),
    )
    .wait()
    .await
    .unwrap();

    assert!(
        output.is_success(),
        "Nested brace expansion failed: stderr={}",
        output.stderr
    );

    // Verify files were created: file_a1, file_a2, file_b1, file_b2
    for suffix in &expected {
        let file = format!("{}_{}", base.display(), suffix);
        assert!(
            Path::new(&file).exists(),
            "File {} should exist after nested expansion",
            file
        );
        let _ = fs::remove_file(&file);
    }
}

#[tokio::test]
async fn test_brace_expansion_preamble_postscript() {
    use std::fs;
    use std::path::Path;

    let temp_dir = std::env::temp_dir();
    let base_name = format!("infraware_preamble_{}", std::process::id());
    let base = temp_dir.join(&base_name);

    let expected = ["pre_A_post", "pre_B_post"];

    // Clean up any previous test files
    for suffix in &expected {
        let file = format!("{}_{}", base.display(), suffix);
        let _ = fs::remove_file(&file);
    }

    // Execute with preamble/postscript brace expansion pre_{A,B}_post
    let output = CommandExecutor::execute(
        "touch",
        &[],
        Some(&format!("touch {}_pre_{{A,B}}_post", base.display())),
        CancellationToken::new(),
    )
    .wait()
    .await
    .unwrap();

    assert!(
        output.is_success(),
        "Preamble/postscript brace expansion failed: stderr={}",
        output.stderr
    );

    // Verify files were created
    for suffix in &expected {
        let file = format!("{}_{}", base.display(), suffix);
        assert!(
            Path::new(&file).exists(),
            "File {} should exist after preamble/postscript expansion",
            file
        );
        let _ = fs::remove_file(&file);
    }
}

// =============================================================================
// Ctrl+C Cancellation Tests
// =============================================================================

#[tokio::test]
async fn test_execute_cancellation_stops_command() {
    let cancel = CancellationToken::new();
    let handle = CommandExecutor::execute(
        "sleep",
        &["10".to_string()], // Long sleep
        None,
        cancel.clone(),
    );

    // Cancel immediately
    cancel.cancel();

    // Should complete quickly (not wait 10 seconds)
    let start = std::time::Instant::now();
    let _ = handle.wait().await;
    let elapsed = start.elapsed();

    assert!(
        elapsed.as_secs() < 3,
        "Cancellation should stop command quickly, took {:?}",
        elapsed
    );
}

#[tokio::test]
async fn test_execute_cancellation_during_streaming() {
    let cancel = CancellationToken::new();
    let mut handle = CommandExecutor::execute(
        "yes",
        &[], // Infinite output
        None,
        cancel.clone(),
    );

    // Read a few lines
    let mut lines = 0;
    while lines < 10 {
        if handle.lines().recv().await.is_some() {
            lines += 1;
        }
    }

    // Cancel while streaming
    cancel.cancel();

    // Should stop and return result
    let result = handle.wait().await;
    assert!(result.is_ok(), "Should return Ok after cancellation");
}

#[tokio::test]
async fn test_execute_cancellation_with_sleep() {
    // Test that cancellation stops a long-running command
    let cancel = CancellationToken::new();

    let handle = CommandExecutor::execute("sleep", &["10".to_string()], None, cancel.clone());

    // Cancel after 100ms
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    cancel.cancel();

    let start = std::time::Instant::now();
    let _ = handle.wait().await;
    let elapsed = start.elapsed();

    // The command should finish quickly after cancellation
    // Note: There's a 500ms grace period for SIGINT in the implementation
    assert!(
        elapsed.as_secs() < 2,
        "Cancellation should stop sleep command, took {:?}",
        elapsed
    );
}

#[tokio::test]
async fn test_execute_cancellation_fast_output_command() {
    // Test that cancellation works for fast-outputting commands
    // This specifically tests the 500ms grace period after SIGINT
    let cancel = CancellationToken::new();
    let mut handle = CommandExecutor::execute(
        "seq",
        &["1".to_string(), "1000000".to_string()], // Produces output very fast
        None,
        cancel.clone(),
    );

    // Wait for some output to confirm command started
    let mut lines = 0;
    while lines < 5 {
        if handle.lines().recv().await.is_some() {
            lines += 1;
        }
    }

    // Cancel while command is outputting rapidly
    let start = std::time::Instant::now();
    cancel.cancel();

    // Should stop within ~500ms grace period + some overhead
    let result = handle.wait().await;
    let elapsed = start.elapsed();

    assert!(result.is_ok(), "Should return Ok after cancellation");
    assert!(
        elapsed.as_millis() < 1500,
        "Fast-output command should be killed within grace period, took {:?}",
        elapsed
    );
}

// =============================================================================
// Streaming Output Tests
// =============================================================================

#[tokio::test]
async fn test_execute_streams_echo_output() {
    let cancel = CancellationToken::new();
    let mut handle = CommandExecutor::execute(
        "sh",
        &[
            "-c".to_string(),
            "echo line1; echo line2; echo line3".to_string(),
        ],
        None,
        cancel,
    );

    let mut lines_received = Vec::new();
    while let Some(line) = handle.lines().recv().await {
        lines_received.push(line);
    }

    assert!(
        lines_received.len() >= 3,
        "Should receive at least 3 lines, got {}",
        lines_received.len()
    );

    let result = handle.wait().await.unwrap();
    assert!(result.is_success());
}

#[tokio::test]
async fn test_execute_streams_ping_output() {
    let cancel = CancellationToken::new();
    let mut handle = CommandExecutor::execute(
        "ping",
        &["-c".to_string(), "2".to_string(), "127.0.0.1".to_string()],
        None,
        cancel,
    );

    let mut lines_received = 0;
    while let Some(_line) = handle.lines().recv().await {
        lines_received += 1;
    }

    assert!(
        lines_received > 0,
        "Should receive streaming output from ping"
    );
    let result = handle.wait().await.unwrap();
    // ping may succeed or fail depending on network, but shouldn't be blocked
    assert!(
        !result.stderr.contains("blocked"),
        "ping should not be blocked"
    );
}

#[tokio::test]
async fn test_execute_streams_with_stderr() {
    let cancel = CancellationToken::new();
    let mut handle = CommandExecutor::execute(
        "sh",
        &[
            "-c".to_string(),
            "echo stdout_line; echo stderr_line >&2".to_string(),
        ],
        None,
        cancel,
    );

    let mut lines = Vec::new();
    while let Some(line) = handle.lines().recv().await {
        lines.push(line);
    }

    let result = handle.wait().await.unwrap();
    assert!(result.is_success());
    // Stderr should be in the result
    assert!(
        result.stderr.contains("stderr_line"),
        "Should capture stderr: {}",
        result.stderr
    );
}

// =============================================================================
// Interactive Command Blocking Tests
// =============================================================================

#[tokio::test]
async fn test_execute_blocks_python_repl() {
    // python3 without args enters REPL mode (interactive)
    let cancel = CancellationToken::new();
    let handle = CommandExecutor::execute("python3", &[], None, cancel);
    let result = handle.wait().await.unwrap();
    assert!(
        result.stderr.contains("not supported") || result.stderr.contains("interactive"),
        "python3 REPL should be blocked: {}",
        result.stderr
    );
}

#[tokio::test]
async fn test_execute_python_with_script_via_shell() {
    // python3 -c "print('hello')" via shell should work
    // Direct python3 command is blocked, but shell execution with -c flag works
    let cancel = CancellationToken::new();
    let handle = CommandExecutor::execute("sh", &[], Some("python3 -c 'print(\"hello\")'"), cancel);
    let result = handle.wait().await.unwrap();
    // Should work if python3 is installed (not blocked)
    // May fail if python3 not installed, but should not be blocked
    assert!(
        !result.stderr.contains("not supported"),
        "python3 -c via shell should not be blocked: {}",
        result.stderr
    );
}

#[tokio::test]
async fn test_execute_blocks_node_repl() {
    // node without args enters REPL mode
    let cancel = CancellationToken::new();
    let handle = CommandExecutor::execute("node", &[], None, cancel);
    let result = handle.wait().await.unwrap();
    assert!(
        result.stderr.contains("not supported") || result.stderr.contains("interactive"),
        "node REPL should be blocked: {}",
        result.stderr
    );
}

#[tokio::test]
async fn test_execute_blocks_gcloud_auth_login() {
    let cancel = CancellationToken::new();
    let handle = CommandExecutor::execute("sh", &[], Some("gcloud auth login"), cancel);
    let result = handle.wait().await.unwrap();
    assert!(
        result.stderr.contains("not supported")
            || result.stderr.contains("blocked")
            || result.stderr.contains("interactive"),
        "gcloud auth login should be blocked: {}",
        result.stderr
    );
}

#[tokio::test]
async fn test_execute_blocks_docker_run_it() {
    let cancel = CancellationToken::new();
    let handle = CommandExecutor::execute("sh", &[], Some("docker run -it ubuntu bash"), cancel);
    let result = handle.wait().await.unwrap();
    // Should either be blocked by our validation OR fail because no TTY
    // Both outcomes are acceptable - the command should not hang
    assert!(
        !result.is_success(),
        "docker run -it should not succeed in non-interactive context"
    );
}

// =============================================================================
// Wait vs Lines API Tests
// =============================================================================

#[tokio::test]
async fn test_execute_wait_collects_all_output() {
    let cancel = CancellationToken::new();
    let handle = CommandExecutor::execute(
        "sh",
        &[],
        Some("echo line1; echo line2; echo line3"),
        cancel,
    );

    // Skip streaming, just wait
    let result = handle.wait().await.unwrap();
    assert!(result.stdout.contains("line1"));
    assert!(result.stdout.contains("line2"));
    assert!(result.stdout.contains("line3"));
}

#[tokio::test]
async fn test_execute_wait_without_streaming() {
    // Verify that wait() works even if lines() is never called
    let cancel = CancellationToken::new();
    let handle = CommandExecutor::execute("echo", &["direct_output".to_string()], None, cancel);

    let result = handle.wait().await.unwrap();
    assert!(result.is_success());
    assert!(
        result.stdout.contains("direct_output"),
        "wait() should collect all output: {}",
        result.stdout
    );
}

#[tokio::test]
async fn test_execute_lines_and_wait_both_work() {
    let cancel = CancellationToken::new();
    let mut handle =
        CommandExecutor::execute("sh", &[], Some("echo streamed; echo collected"), cancel);

    // Read first line via streaming
    let first_line = handle.lines().recv().await;
    assert!(first_line.is_some(), "Should receive at least one line");

    // Get result via wait (should still work)
    let result = handle.wait().await.unwrap();
    assert!(result.is_success());
}

#[tokio::test]
async fn test_execute_blocked_command_returns_immediately() {
    // Blocked commands should return immediately without streaming
    let cancel = CancellationToken::new();
    let start = std::time::Instant::now();

    let handle = CommandExecutor::execute("cat", &["/dev/zero".to_string()], None, cancel);

    let result = handle.wait().await.unwrap();
    let elapsed = start.elapsed();

    assert!(!result.is_success());
    assert!(result.stderr.contains("blocked"));
    assert!(
        elapsed.as_millis() < 500,
        "Blocked command should return immediately, took {:?}",
        elapsed
    );
}

// =============================================================================
// Infinite Device Blocking - Additional Tests
// =============================================================================

#[tokio::test]
async fn test_execute_blocks_cat_dev_random() {
    let cancel = CancellationToken::new();
    let handle = CommandExecutor::execute("cat", &["/dev/random".to_string()], None, cancel);
    let result = handle.wait().await.unwrap();
    assert!(!result.is_success());
    assert!(
        result.stderr.contains("blocked") || result.stderr.contains("infinite"),
        "/dev/random should be blocked: {}",
        result.stderr
    );
}

#[tokio::test]
async fn test_execute_allows_dd_dev_zero_with_count() {
    let cancel = CancellationToken::new();
    let handle = CommandExecutor::execute(
        "dd",
        &[
            "if=/dev/zero".to_string(),
            "of=/dev/null".to_string(),
            "count=1".to_string(),
            "bs=10".to_string(),
        ],
        None,
        cancel,
    );
    let result = handle.wait().await.unwrap();
    assert!(
        !result.stderr.contains("blocked"),
        "dd with count should be allowed"
    );
    assert!(result.is_success());
}

#[tokio::test]
async fn test_execute_allows_dev_urandom_piped_to_head() {
    let cancel = CancellationToken::new();
    let handle = CommandExecutor::execute("sh", &[], Some("cat /dev/urandom | head -c 10"), cancel);
    let result = handle.wait().await.unwrap();
    assert!(
        !result.stderr.contains("blocked"),
        "Piped to head should be allowed"
    );
    assert!(result.is_success());
}

// =============================================================================
// Flood Command Protection Tests
// =============================================================================
// These tests verify that flood commands (commands that produce infinite output)
// do NOT block the terminal and can be interrupted via Ctrl+C (CancellationToken)

#[tokio::test]
async fn test_flood_yes_command_cancellable() {
    // yes produces infinite output - must be cancellable via Ctrl+C
    let cancel = CancellationToken::new();
    let handle = CommandExecutor::execute("yes", &[], None, cancel.clone());

    // Cancel after 100ms
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    cancel.cancel();

    let start = std::time::Instant::now();
    let result = handle.wait().await;
    let elapsed = start.elapsed();

    assert!(result.is_ok(), "yes should complete after cancellation");
    assert!(
        elapsed.as_secs() < 2,
        "yes should stop within 2 seconds after Ctrl+C, took {:?}",
        elapsed
    );
}

#[tokio::test]
async fn test_flood_seq_large_output_limited() {
    // seq with large number produces lots of output but is finite
    // Should be limited by MAX_OUTPUT_LINES
    let cancel = CancellationToken::new();
    let handle = CommandExecutor::execute(
        "seq",
        &["1".to_string(), "100000".to_string()],
        None,
        cancel,
    );
    let result = handle.wait().await.unwrap();

    // Output should be truncated to MAX_OUTPUT_LINES (1000)
    let line_count = result.stdout.lines().count();
    assert!(
        line_count <= 1010, // Allow some margin for truncation message
        "seq output should be limited, got {} lines",
        line_count
    );
}

#[tokio::test]
async fn test_flood_seq_cancellable() {
    // seq with very large range should be cancellable
    let cancel = CancellationToken::new();
    let handle = CommandExecutor::execute(
        "seq",
        &["1".to_string(), "999999999".to_string()],
        None,
        cancel.clone(),
    );

    // Cancel after 100ms
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    cancel.cancel();

    let start = std::time::Instant::now();
    let result = handle.wait().await;
    let elapsed = start.elapsed();

    assert!(result.is_ok(), "seq should complete after cancellation");
    assert!(
        elapsed.as_secs() < 2,
        "seq should stop within 2 seconds after Ctrl+C, took {:?}",
        elapsed
    );
}

#[tokio::test]
async fn test_flood_background_yes_blocked() {
    // yes & (background) should be blocked to prevent resource exhaustion
    use infraware_terminal::executor::job_manager::JobManager;
    use std::sync::{Arc, RwLock};
    let job_manager = Arc::new(RwLock::new(JobManager::new()));
    let result = CommandExecutor::execute_background("yes &", &job_manager).await;
    assert!(result.is_err(), "yes & should be blocked");
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("infinite") || err_msg.contains("cannot be run in background"),
        "Error should mention infinite output: {}",
        err_msg
    );
}

#[tokio::test]
async fn test_flood_background_cat_dev_zero_blocked() {
    // cat /dev/zero & should be blocked
    use infraware_terminal::executor::job_manager::JobManager;
    use std::sync::{Arc, RwLock};
    let job_manager = Arc::new(RwLock::new(JobManager::new()));
    let result = CommandExecutor::execute_background("cat /dev/zero &", &job_manager).await;
    assert!(result.is_err(), "cat /dev/zero & should be blocked");
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("infinite") || err_msg.contains("device"),
        "Error should mention infinite device: {}",
        err_msg
    );
}

#[tokio::test]
async fn test_flood_background_cat_dev_urandom_blocked() {
    // cat /dev/urandom & should be blocked
    use infraware_terminal::executor::job_manager::JobManager;
    use std::sync::{Arc, RwLock};
    let job_manager = Arc::new(RwLock::new(JobManager::new()));
    let result = CommandExecutor::execute_background("cat /dev/urandom &", &job_manager).await;
    assert!(result.is_err(), "cat /dev/urandom & should be blocked");
}

#[tokio::test]
async fn test_flood_multiple_cancellation_tokens() {
    // Test that multiple commands with different cancellation tokens work
    let cancel1 = CancellationToken::new();
    let cancel2 = CancellationToken::new();

    let handle1 = CommandExecutor::execute("sleep", &["10".to_string()], None, cancel1.clone());
    let handle2 = CommandExecutor::execute("sleep", &["10".to_string()], None, cancel2.clone());

    // Cancel only the first one
    cancel1.cancel();

    let start = std::time::Instant::now();
    let _ = handle1.wait().await;
    let elapsed1 = start.elapsed();

    // First should complete quickly
    assert!(
        elapsed1.as_secs() < 2,
        "First command should stop quickly, took {:?}",
        elapsed1
    );

    // Cancel second one
    cancel2.cancel();
    let _ = handle2.wait().await;
}

#[tokio::test]
async fn test_flood_rapid_cancellation() {
    // Test rapid cancellation doesn't cause issues
    for _ in 0..5 {
        let cancel = CancellationToken::new();
        let handle = CommandExecutor::execute("yes", &[], None, cancel.clone());

        // Cancel immediately
        cancel.cancel();

        let result = handle.wait().await;
        assert!(result.is_ok(), "Rapid cancellation should work");
    }
}

#[tokio::test]
async fn test_flood_output_streaming_cancellable() {
    // Test that streaming output can be interrupted
    let cancel = CancellationToken::new();
    let mut handle = CommandExecutor::execute("yes", &[], None, cancel.clone());

    // Read some lines
    let mut lines_read = 0;
    let start = std::time::Instant::now();

    while lines_read < 100 {
        tokio::select! {
            Some(_) = handle.lines().recv() => {
                lines_read += 1;
            }
            _ = tokio::time::sleep(tokio::time::Duration::from_secs(5)) => {
                panic!("Timeout waiting for streaming output");
            }
        }
    }

    // Cancel while streaming
    cancel.cancel();

    let result = handle.wait().await;
    let elapsed = start.elapsed();

    assert!(result.is_ok(), "Should complete after cancellation");
    assert!(
        elapsed.as_secs() < 3,
        "Should stop quickly after cancel, took {:?}",
        elapsed
    );
}

#[tokio::test]
async fn test_flood_shell_command_cancellable() {
    // Shell commands with infinite output should be cancellable
    let cancel = CancellationToken::new();
    let handle = CommandExecutor::execute(
        "sh",
        &[
            "-c".to_string(),
            "while true; do echo flood; done".to_string(),
        ],
        None,
        cancel.clone(),
    );

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    cancel.cancel();

    let start = std::time::Instant::now();
    let result = handle.wait().await;
    let elapsed = start.elapsed();

    assert!(result.is_ok(), "Shell flood should be cancellable");
    assert!(
        elapsed.as_secs() < 2,
        "Shell flood should stop quickly, took {:?}",
        elapsed
    );
}

// =============================================================================
// Package Manager Tests (apt, yum, etc.)
// =============================================================================
// These tests verify that package managers work correctly and return proper exit codes

#[tokio::test]
async fn test_apt_list_exit_code_zero() {
    // apt list should exit with code 0, not -1
    // This tests the SIGPIPE handling fix
    let cancel = CancellationToken::new();
    let handle = CommandExecutor::execute("apt", &["list".to_string()], None, cancel);
    let result = handle.wait().await.unwrap();

    // apt list should succeed with exit code 0
    // Note: apt may write a warning to stderr, but that's not an error
    assert_eq!(
        result.exit_code, 0,
        "apt list should exit with code 0, got {}. stderr: {}",
        result.exit_code, result.stderr
    );

    // Verify we got some output (package list)
    assert!(!result.stdout.is_empty(), "apt list should produce output");
}

#[tokio::test]
async fn test_command_with_stderr_warning_exits_zero() {
    // Commands that write warnings to stderr should still exit 0
    // This tests that stderr output doesn't affect exit code
    let cancel = CancellationToken::new();
    let handle = CommandExecutor::execute(
        "sh",
        &[
            "-c".to_string(),
            "echo 'output'; echo 'warning' >&2; exit 0".to_string(),
        ],
        None,
        cancel,
    );
    let result = handle.wait().await.unwrap();

    assert_eq!(
        result.exit_code, 0,
        "Command with stderr warning should exit 0"
    );
    assert!(result.stdout.contains("output"));
    assert!(result.stderr.contains("warning"));
}
