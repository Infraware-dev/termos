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
    assert!(CommandExecutor::requires_interactive("sudo"));
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
