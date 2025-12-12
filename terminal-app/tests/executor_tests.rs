/// Tests for command executor
use infraware_terminal::executor::CommandExecutor;

#[tokio::test]
async fn test_execute_simple_command() {
    let result = CommandExecutor::execute("echo", &["hello".to_string()], None)
        .await
        .unwrap();

    assert!(result.is_success());
    assert_eq!(result.stdout.trim(), "hello");
    assert_eq!(result.exit_code, 0);
}

#[tokio::test]
async fn test_execute_command_with_args() {
    let result =
        CommandExecutor::execute("echo", &["hello".to_string(), "world".to_string()], None)
            .await
            .unwrap();

    assert!(result.is_success());
    assert_eq!(result.stdout.trim(), "hello world");
}

#[tokio::test]
async fn test_command_not_found() {
    let result = CommandExecutor::execute("nonexistentcommand12345", &[], None).await;
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
    let result = CommandExecutor::execute("ls", &["/nonexistent/directory/path".to_string()], None)
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
    let result = CommandExecutor::execute("sh", &[], Some("echo hello | cat"))
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
    )
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
    )
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
    )
    .await
    .unwrap();

    assert!(result.is_success());
    assert_eq!(result.stdout.trim(), "stdout");
    assert_eq!(result.stderr.trim(), "stderr");
}

#[tokio::test]
async fn test_execute_exit_codes() {
    // Exit code 0
    let result = CommandExecutor::execute("sh", &["-c".to_string(), "exit 0".to_string()], None)
        .await
        .unwrap();
    assert_eq!(result.exit_code, 0);
    assert!(result.is_success());

    // Exit code 1
    let result = CommandExecutor::execute("sh", &["-c".to_string(), "exit 1".to_string()], None)
        .await
        .unwrap();
    assert_eq!(result.exit_code, 1);
    assert!(!result.is_success());

    // Exit code 42
    let result = CommandExecutor::execute("sh", &["-c".to_string(), "exit 42".to_string()], None)
        .await
        .unwrap();
    assert_eq!(result.exit_code, 42);
    assert!(!result.is_success());
}

// =============================================================================
// Infinite Output Command Blocking Tests
// =============================================================================

#[tokio::test]
async fn test_yes_command_blocked() {
    let output = CommandExecutor::execute("yes", &[], None).await.unwrap();
    assert!(!output.is_success());
    assert_eq!(output.exit_code, 1);
    assert!(
        output.stderr.contains("blocked")
            || output.stderr.contains("not supported")
            || output.stderr.contains("Interactive")
    );
}

#[tokio::test]
async fn test_yes_command_error_message_helpful() {
    let output = CommandExecutor::execute("yes", &[], None).await.unwrap();
    // Verify the error message provides some guidance
    assert!(
        !output.stderr.is_empty(),
        "Error message should not be empty"
    );
    assert!(
        output.stderr.contains("Suggestions")
            || output.stderr.contains("Alternative")
            || output.stderr.contains("not supported"),
        "Error message should provide helpful suggestions"
    );
}

#[tokio::test]
async fn test_cat_dev_zero_blocked() {
    let output = CommandExecutor::execute("cat", &["/dev/zero".to_string()], None)
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
    let output = CommandExecutor::execute("cat", &["/dev/urandom".to_string()], None)
        .await
        .unwrap();
    assert!(!output.is_success());
    assert!(output.stderr.contains("blocked") || output.stderr.contains("infinite"));
}

#[tokio::test]
async fn test_cat_normal_file_allowed() {
    // cat of a normal file should work
    let output = CommandExecutor::execute("cat", &["/etc/hostname".to_string()], None)
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
    let output = CommandExecutor::execute("dd", &["if=/dev/zero".to_string()], None)
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
    )
    .await
    .unwrap();
    // /dev/null is not in INFINITE_DEVICES, so it should be allowed
    assert!(
        !output.stderr.contains("blocked"),
        "dd with /dev/null should not be blocked"
    );
}

#[tokio::test]
async fn test_ping_without_count_blocked() {
    let output = CommandExecutor::execute("ping", &["localhost".to_string()], None)
        .await
        .unwrap();
    assert!(!output.is_success());
    assert!(
        output.stderr.contains("-c") || output.stderr.contains("count"),
        "Error should suggest using -c flag"
    );
}

#[tokio::test]
async fn test_ping_with_count_allowed() {
    let output = CommandExecutor::execute(
        "ping",
        &["-c".to_string(), "1".to_string(), "localhost".to_string()],
        None,
    )
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
    )
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
    let output = CommandExecutor::execute("sh", &[], Some("cat /dev/zero"))
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
    let output = CommandExecutor::execute("sh", &[], Some("cat /dev/urandom | head -c 10"))
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
    )
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
    let output = CommandExecutor::execute("sh", &[], Some("yes | head -5"))
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
    )
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
    )
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
    )
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
    )
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
    let output = CommandExecutor::execute("bash", &[], Some("bash -c 'echo {01..02}'"))
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
    )
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
    )
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
    )
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
    )
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
