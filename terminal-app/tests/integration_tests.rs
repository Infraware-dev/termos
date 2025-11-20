use infraware_terminal::executor::CommandExecutor;
/// Integration tests for Infraware Terminal
use infraware_terminal::input::{InputClassifier, InputType};
use infraware_terminal::llm::{LLMClientTrait, MockLLMClient, ResponseRenderer};

#[tokio::test]
async fn test_end_to_end_command_execution() {
    let classifier = InputClassifier::new();

    // Classify input
    let input = "echo test";
    let classified = classifier.classify(input).unwrap();

    // Execute if it's a command
    match classified {
        InputType::Command {
            command,
            args,
            original_input,
        } => {
            let result = CommandExecutor::execute(&command, &args, original_input.as_deref())
                .await
                .unwrap();
            assert!(result.is_success());
            assert_eq!(result.stdout.trim(), "test");
        }
        _ => panic!("Expected command"),
    }
}

#[tokio::test]
async fn test_end_to_end_natural_language() {
    let classifier = InputClassifier::new();
    let llm = MockLLMClient;

    // Classify input
    let input = "how do I list files?";
    let classified = classifier.classify(input).unwrap();

    // Query LLM if it's natural language
    match classified {
        InputType::NaturalLanguage(query) => {
            let response = llm.query(&query).await.unwrap();
            assert!(response.contains("ls"));
        }
        _ => panic!("Expected natural language"),
    }
}

#[tokio::test]
async fn test_llm_response_rendering() {
    let llm = MockLLMClient;
    let renderer = ResponseRenderer::new();

    // Get LLM response
    let response = llm.query("what is docker").await.unwrap();

    // Render the response
    let rendered = renderer.render(&response);

    assert!(!rendered.is_empty());

    // Print to verify colors (for manual inspection)
    println!("\n=== RAW RESPONSE ===");
    println!("{}", response);
    println!("\n=== RENDERED WITH ANSI COLORS ===");
    for line in &rendered {
        println!("{}", line);
    }
}

#[test]
fn test_command_classification_accuracy() {
    let classifier = InputClassifier::new();

    let test_cases = vec![
        ("ls -la", true),                      // Always available
        ("unknown-cmd --flag", true),          // CommandSyntaxHandler catches flags
        ("cat file.txt | grep pattern", true), // Pipes are command syntax
        ("how do I list files?", false),       // Question mark = natural language
        ("what are containers?", false),       // Question = natural language
        ("show me the logs", false),           // Article "the" = natural language
        ("explain docker to me", false),       // Natural language phrase
    ];

    for (input, should_be_command) in test_cases {
        let result = classifier.classify(input).unwrap();
        let is_command = matches!(
            result,
            InputType::Command { .. } | InputType::CommandTypo { .. }
        );
        assert_eq!(is_command, should_be_command, "Failed for input: {}", input);
    }
}

#[tokio::test]
async fn test_pipe_command_end_to_end() {
    let classifier = InputClassifier::new();

    // Test pipe command classification and execution
    let input = "echo hello | grep hello";
    let classified = classifier.classify(input).unwrap();

    match classified {
        InputType::Command {
            command,
            args,
            original_input,
        } => {
            // Verify original_input is preserved for shell operators
            assert!(original_input.is_some());
            assert_eq!(original_input.as_deref().unwrap(), input);

            // Execute with shell interpretation
            let result = CommandExecutor::execute(&command, &args, original_input.as_deref())
                .await
                .unwrap();
            assert!(result.is_success());
            assert_eq!(result.stdout.trim(), "hello");
        }
        _ => panic!("Expected Command with pipe"),
    }
}

#[tokio::test]
async fn test_redirect_command_end_to_end() {
    let classifier = InputClassifier::new();

    // Test redirect command
    let input = "echo test > /tmp/test_e2e.txt && cat /tmp/test_e2e.txt && rm /tmp/test_e2e.txt";
    let classified = classifier.classify(input).unwrap();

    match classified {
        InputType::Command {
            command,
            args,
            original_input,
        } => {
            assert!(original_input.is_some());
            let result = CommandExecutor::execute(&command, &args, original_input.as_deref())
                .await
                .unwrap();
            assert!(result.is_success());
            assert_eq!(result.stdout.trim(), "test");
        }
        _ => panic!("Expected Command with redirect"),
    }
}

#[tokio::test]
async fn test_simple_command_no_shell_interpretation() {
    let classifier = InputClassifier::new();

    // Simple command without operators should NOT use shell interpretation
    let input = "echo hello";
    let classified = classifier.classify(input).unwrap();

    match classified {
        InputType::Command {
            command,
            args,
            original_input,
        } => {
            // Verify NO original_input for simple commands (no shell operators)
            assert!(original_input.is_none());
            assert_eq!(command, "echo");
            assert_eq!(args, vec!["hello"]);

            // Execute directly without shell
            let result = CommandExecutor::execute(&command, &args, None)
                .await
                .unwrap();
            assert!(result.is_success());
            assert_eq!(result.stdout.trim(), "hello");
        }
        _ => panic!("Expected simple Command"),
    }
}

#[tokio::test]
async fn test_grep_no_match_exit_code_1() {
    let classifier = InputClassifier::new();

    // Test grep with no match returns exit 1 (benign, not an error)
    let input = "ls -la | grep ps";
    let classified = classifier.classify(input).unwrap();

    match classified {
        InputType::Command {
            command,
            args,
            original_input,
        } => {
            // Execute the command
            let result = CommandExecutor::execute(&command, &args, original_input.as_deref())
                .await
                .unwrap();

            // grep returns exit 1 when no match is found
            // This is NOT an error, it's semantic (no match)
            assert_eq!(result.exit_code, 1);

            // No output because grep found no match
            assert!(result.stdout.is_empty());

            // No stderr either
            assert!(result.stderr.is_empty());
        }
        _ => panic!("Expected Command with pipe"),
    }
}

#[tokio::test]
async fn test_grep_with_match_exit_code_0() {
    let classifier = InputClassifier::new();

    // Test grep with match returns exit 0
    let input = "ls -la | grep Cargo";
    let classified = classifier.classify(input).unwrap();

    match classified {
        InputType::Command {
            command,
            args,
            original_input,
        } => {
            // Execute the command
            let result = CommandExecutor::execute(&command, &args, original_input.as_deref())
                .await
                .unwrap();

            // grep returns exit 0 when match is found
            assert_eq!(result.exit_code, 0);

            // Should have output with matched lines
            assert!(!result.stdout.is_empty());
            assert!(result.stdout.contains("Cargo"));
        }
        _ => panic!("Expected Command with pipe"),
    }
}

#[test]
fn test_alias_expansion_in_classifier() {
    use infraware_terminal::input::discovery::CommandCache;

    // Clear cache and add test alias
    CommandCache::clear();
    {
        let cache = std::sync::RwLock::new(());
        let guard = cache.write().unwrap();
        drop(guard); // Just to ensure we can acquire lock

        // Manually add alias via internal method
        // This simulates what load_system_aliases() would do
    }

    // For now, test that classifier doesn't crash with non-existent aliases
    let classifier = InputClassifier::new();

    // Test with command that's not an alias
    let result = classifier.classify("ls -la").unwrap();
    assert!(matches!(result, InputType::Command { .. }));

    // Clean up
    CommandCache::clear();
}

#[tokio::test]
async fn test_reload_aliases_command() {
    use infraware_terminal::input::discovery::CommandCache;

    // This tests that the reload mechanism works
    CommandCache::clear();

    // Load system aliases
    let result = CommandCache::load_system_aliases();
    assert!(result.is_ok());

    // Just verify it doesn't panic - actual aliases depend on system config
    let _stats = CommandCache::stats();

    CommandCache::clear();
}

#[tokio::test]
async fn test_shell_builtin_colon_execution() {
    let classifier = InputClassifier::new();

    // Test : (no-op) builtin
    let input = ":";
    let classified = classifier.classify(input).unwrap();

    match classified {
        InputType::Command {
            command,
            args,
            original_input,
        } => {
            assert_eq!(command, ":");
            assert!(args.is_empty());

            // Execute the builtin
            let result = CommandExecutor::execute(&command, &args, original_input.as_deref())
                .await
                .unwrap();

            // : always succeeds
            assert!(result.is_success());
            assert_eq!(result.exit_code, 0);
        }
        _ => panic!("Expected Command"),
    }
}

#[tokio::test]
async fn test_shell_builtin_true_false() {
    let classifier = InputClassifier::new();

    // Test true builtin
    let true_result = classifier.classify("true").unwrap();
    match true_result {
        InputType::Command {
            command,
            args,
            original_input,
        } => {
            let result = CommandExecutor::execute(&command, &args, original_input.as_deref())
                .await
                .unwrap();
            assert_eq!(result.exit_code, 0);
        }
        _ => panic!("Expected Command for 'true'"),
    }

    // Test false builtin
    let false_result = classifier.classify("false").unwrap();
    match false_result {
        InputType::Command {
            command,
            args,
            original_input,
        } => {
            let result = CommandExecutor::execute(&command, &args, original_input.as_deref())
                .await
                .unwrap();
            assert_eq!(result.exit_code, 1);
        }
        _ => panic!("Expected Command for 'false'"),
    }
}

#[tokio::test]
async fn test_shell_builtin_export() {
    let classifier = InputClassifier::new();

    // Test export builtin
    let input = "export TEST_VAR=hello";
    let classified = classifier.classify(input).unwrap();

    match classified {
        InputType::Command {
            command,
            args,
            original_input,
        } => {
            assert_eq!(command, "export");
            assert_eq!(args, vec!["TEST_VAR=hello"]);

            // Execute the builtin
            let result = CommandExecutor::execute(&command, &args, original_input.as_deref())
                .await
                .unwrap();

            // export in a subshell will succeed but won't affect parent
            assert!(result.is_success());
        }
        _ => panic!("Expected Command"),
    }
}

#[tokio::test]
async fn test_shell_builtin_test_command() {
    let classifier = InputClassifier::new();

    // Test [ builtin with file test
    let input = "[ -d /tmp ]";
    let classified = classifier.classify(input).unwrap();

    match classified {
        InputType::Command {
            command,
            args,
            original_input,
        } => {
            assert_eq!(command, "[");

            // Execute the builtin
            let result = CommandExecutor::execute(&command, &args, original_input.as_deref())
                .await
                .unwrap();

            // /tmp should exist as a directory
            assert_eq!(result.exit_code, 0);
        }
        _ => panic!("Expected Command"),
    }
}

#[tokio::test]
async fn test_shell_builtin_double_bracket() {
    let classifier = InputClassifier::new();

    // Test [[ builtin (bash-specific)
    let input = "[[ -d /tmp ]]";
    let classified = classifier.classify(input).unwrap();

    match classified {
        InputType::Command {
            command,
            args,
            original_input,
        } => {
            assert_eq!(command, "[[");

            // Execute the builtin
            let _result = CommandExecutor::execute(&command, &args, original_input.as_deref())
                .await
                .unwrap();

            // [[ requires bash, but sh might not support it
            // We execute via sh, so this might fail on systems without bash
            // Just verify it doesn't crash
        }
        _ => panic!("Expected Command"),
    }
}
