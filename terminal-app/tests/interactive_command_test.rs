use infraware_terminal::executor::CommandExecutor;
use infraware_terminal::input::discovery::CommandCache;
use infraware_terminal::input::{InputClassifier, InputType};

#[test]
fn test_command_existence_check_before_interactive() {
    // Verify that requires_interactive returns true for interactive commands
    assert!(CommandExecutor::requires_interactive("htop"));
    assert!(CommandExecutor::requires_interactive("top"));
    assert!(CommandExecutor::requires_interactive("vim"));
    assert!(CommandExecutor::requires_interactive("nano"));
    // sudo is handled via root mode wrapper, not as interactive command
    assert!(!CommandExecutor::requires_interactive("sudo"));

    // Package managers are NOT interactive (output is captured for scrolling)
    assert!(!CommandExecutor::requires_interactive("apt"));
    assert!(!CommandExecutor::requires_interactive("yum"));
    assert!(!CommandExecutor::requires_interactive("dnf"));

    // But command_exists should correctly report if they're installed
    // (this varies by system, so we just verify it doesn't panic)
    let _ = CommandExecutor::command_exists("htop");
    let _ = CommandExecutor::command_exists("top");
    let _ = CommandExecutor::command_exists("apt");
    let _ = CommandExecutor::command_exists("sudo");
}

#[tokio::test]
async fn test_classification_preserves_interactive_commands() {
    // Verify that the input classifier correctly identifies interactive commands

    let classifier = InputClassifier::new();

    // apt is a known command (in DevOps whitelist) - test only if installed (Linux only)
    if CommandCache::is_available("apt") {
        match classifier.classify("apt list").unwrap() {
            InputType::Command { command, args, .. } => {
                assert_eq!(command, "apt");
                assert_eq!(args, vec!["list"]);
            }
            other => panic!("Expected Command, got {other:?}"),
        }
    }

    // htop is a known command - test only if installed
    if CommandCache::is_available("htop") {
        match classifier.classify("htop").unwrap() {
            InputType::Command { command, args, .. } => {
                assert_eq!(command, "htop");
                assert!(args.is_empty());
            }
            other => panic!("Expected Command, got {other:?}"),
        }
    }

    // top is a known command - test only if installed
    if CommandCache::is_available("top") {
        match classifier.classify("top").unwrap() {
            InputType::Command { command, args, .. } => {
                assert_eq!(command, "top");
                assert!(args.is_empty());
            }
            other => panic!("Expected Command, got {other:?}"),
        }
    }

    // ls is universally available on Unix - always test
    match classifier.classify("ls -la").unwrap() {
        InputType::Command { command, args, .. } => {
            assert_eq!(command, "ls");
            assert_eq!(args, vec!["-la"]);
        }
        other => panic!("Expected Command, got {other:?}"),
    }
}
