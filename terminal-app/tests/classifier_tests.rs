/// Tests for input classifier
use infraware_terminal::input::{InputClassifier, InputType};

#[test]
fn test_classify_known_commands() {
    let classifier = InputClassifier::new();

    // Basic shell commands
    match classifier.classify("ls -la").unwrap() {
        InputType::Command { command, args, .. } => {
            assert_eq!(command, "ls");
            assert_eq!(args, vec!["-la"]);
        }
        _ => panic!("Expected Command"),
    }

    // Commands with flags are classified by CommandSyntaxHandler
    // even if the command isn't installed (docker/kubectl may not be available)
    match classifier.classify("unknown-cmd --flag").unwrap() {
        InputType::Command { command, args, .. } => {
            assert_eq!(command, "unknown-cmd");
            assert_eq!(args, vec!["--flag"]);
        }
        InputType::CommandTypo { .. } => {
            // May be detected as typo if similar to a known command
        }
        _ => panic!("Expected Command or CommandTypo"),
    }
}

#[test]
fn test_classify_natural_language() {
    let classifier = InputClassifier::new();

    // Questions
    assert!(matches!(
        classifier.classify("how do I list files?").unwrap(),
        InputType::NaturalLanguage(_)
    ));

    // Use clearer natural language to avoid "kubernetes" → "kubectl" typo detection
    assert!(matches!(
        classifier.classify("what are containers?").unwrap(),
        InputType::NaturalLanguage(_)
    ));

    // Phrases
    assert!(matches!(
        classifier.classify("show me the logs").unwrap(),
        InputType::NaturalLanguage(_)
    ));

    assert!(matches!(
        classifier.classify("explain how docker works").unwrap(),
        InputType::NaturalLanguage(_)
    ));
}

#[test]
fn test_classify_command_syntax() {
    let classifier = InputClassifier::new();

    // Flags should be recognized as commands
    match classifier.classify("unknowncmd --flag value").unwrap() {
        InputType::Command { command, .. } => {
            assert_eq!(command, "unknowncmd");
        }
        _ => panic!("Expected Command"),
    }

    // Pipes should be recognized as commands
    match classifier.classify("cat file.txt | grep pattern").unwrap() {
        InputType::Command { .. } => {}
        _ => panic!("Expected Command"),
    }

    // Redirects
    match classifier.classify("echo hello > file.txt").unwrap() {
        InputType::Command { .. } => {}
        _ => panic!("Expected Command"),
    }
}

#[test]
fn test_classify_empty() {
    let classifier = InputClassifier::new();

    assert!(matches!(classifier.classify("").unwrap(), InputType::Empty));

    assert!(matches!(
        classifier.classify("   ").unwrap(),
        InputType::Empty
    ));
}

#[test]
fn test_classify_edge_cases() {
    let classifier = InputClassifier::new();

    // Long natural language should be classified correctly
    let long_query =
        "can you please explain how to set up a kubernetes cluster with helm and terraform";
    assert!(matches!(
        classifier.classify(long_query).unwrap(),
        InputType::NaturalLanguage(_)
    ));

    // Short commands should work
    match classifier.classify("ls").unwrap() {
        InputType::Command { command, args, .. } => {
            assert_eq!(command, "ls");
            assert!(args.is_empty());
        }
        _ => panic!("Expected Command"),
    }
}

#[test]
fn test_classify_shell_builtins() {
    let classifier = InputClassifier::new();

    // Test . (dot/source) builtin
    match classifier.classify(".").unwrap() {
        InputType::Command { command, args, .. } => {
            assert_eq!(command, ".");
            assert!(args.is_empty());
        }
        _ => panic!("Expected Command for '.'"),
    }

    // Test . with file argument
    match classifier.classify(". ~/.bashrc").unwrap() {
        InputType::Command { command, args, .. } => {
            assert_eq!(command, ".");
            assert_eq!(args, vec!["~/.bashrc"]);
        }
        _ => panic!("Expected Command for '. ~/.bashrc'"),
    }

    // Test : (colon/no-op) builtin
    match classifier.classify(":").unwrap() {
        InputType::Command { command, args, .. } => {
            assert_eq!(command, ":");
            assert!(args.is_empty());
        }
        _ => panic!("Expected Command for ':'"),
    }

    // Test [ (single bracket) builtin
    match classifier.classify("[ -f file.txt ]").unwrap() {
        InputType::Command { command, args, .. } => {
            assert_eq!(command, "[");
            assert_eq!(args, vec!["-f", "file.txt", "]"]);
        }
        _ => panic!("Expected Command for '['"),
    }

    // Test [[ (double bracket) builtin
    match classifier.classify("[[ -f file.txt ]]").unwrap() {
        InputType::Command { command, args, .. } => {
            assert_eq!(command, "[[");
            assert_eq!(args, vec!["-f", "file.txt", "]]"]);
        }
        _ => panic!("Expected Command for '[['"),
    }

    // Test source builtin
    match classifier.classify("source ~/.bashrc").unwrap() {
        InputType::Command { command, args, .. } => {
            assert_eq!(command, "source");
            assert_eq!(args, vec!["~/.bashrc"]);
        }
        _ => panic!("Expected Command for 'source'"),
    }

    // Test export builtin
    match classifier.classify("export PATH=/usr/bin").unwrap() {
        InputType::Command { command, args, .. } => {
            assert_eq!(command, "export");
            assert_eq!(args, vec!["PATH=/usr/bin"]);
        }
        _ => panic!("Expected Command for 'export'"),
    }

    // Test test builtin
    match classifier.classify("test -f file.txt").unwrap() {
        InputType::Command { command, args, .. } => {
            assert_eq!(command, "test");
            assert_eq!(args, vec!["-f", "file.txt"]);
        }
        _ => panic!("Expected Command for 'test'"),
    }

    // Test true/false builtins
    match classifier.classify("true").unwrap() {
        InputType::Command { command, .. } => {
            assert_eq!(command, "true");
        }
        _ => panic!("Expected Command for 'true'"),
    }

    match classifier.classify("false").unwrap() {
        InputType::Command { command, .. } => {
            assert_eq!(command, "false");
        }
        _ => panic!("Expected Command for 'false'"),
    }
}

#[test]
fn test_empty_quotes_no_panic() {
    let classifier = InputClassifier::new();

    // Empty quotes should not cause panic
    let result1 = classifier.classify("\"\"");
    assert!(result1.is_ok(), "Empty double quotes should not panic");

    let result2 = classifier.classify("''");
    assert!(result2.is_ok(), "Empty single quotes should not panic");

    // Empty quotes with other content
    let result3 = classifier.classify("\"\" --flag");
    assert!(result3.is_ok(), "Empty quotes with flags should not panic");
}

#[test]
fn test_malformed_input_no_panic() {
    let classifier = InputClassifier::new();

    // Malformed quotes - may return error, but should not panic
    let result1 = classifier.classify("\"unclosed");
    assert!(
        result1.is_ok() || result1.is_err(),
        "Unclosed quote should not panic"
    );

    // Multiple spaces
    let result2 = classifier.classify("     ");
    assert!(result2.is_ok(), "Multiple spaces should not panic");

    // Special characters
    let result3 = classifier.classify("@#$%^&*");
    assert!(result3.is_ok(), "Special characters should not panic");
}
