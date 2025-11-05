/// Tests for command executor
use infraware_terminal::executor::CommandExecutor;

#[tokio::test]
async fn test_execute_simple_command() {
    let result = CommandExecutor::execute("echo", &vec!["hello".to_string()])
        .await
        .unwrap();

    assert!(result.is_success());
    assert_eq!(result.stdout.trim(), "hello");
    assert_eq!(result.exit_code, 0);
}

#[tokio::test]
async fn test_execute_command_with_args() {
    let result = CommandExecutor::execute("echo", &vec!["hello".to_string(), "world".to_string()])
        .await
        .unwrap();

    assert!(result.is_success());
    assert_eq!(result.stdout.trim(), "hello world");
}

#[tokio::test]
async fn test_command_not_found() {
    let result = CommandExecutor::execute("nonexistentcommand12345", &vec![]).await;
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
    let result = CommandExecutor::execute("ls", &vec!["/nonexistent/directory/path".to_string()])
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
