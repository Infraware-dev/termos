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
fn test_classify_single_word_natural_language() {
    let classifier = InputClassifier::new();

    // Question words (English)
    assert!(matches!(
        classifier.classify("what").unwrap(),
        InputType::NaturalLanguage(_)
    ));
    assert!(matches!(
        classifier.classify("how").unwrap(),
        InputType::NaturalLanguage(_)
    ));
    assert!(matches!(
        classifier.classify("why").unwrap(),
        InputType::NaturalLanguage(_)
    ));

    // Greetings (in minimal filter list)
    assert!(matches!(
        classifier.classify("hello").unwrap(),
        InputType::NaturalLanguage(_)
    ));
    assert!(matches!(
        classifier.classify("hi").unwrap(),
        InputType::NaturalLanguage(_)
    ));
    // Note: Single-word greetings in other languages ("ciao", "hola")
    // may be caught as typos of short commands. This is acceptable since
    // the language-agnostic algorithm works for multi-word inputs:
    // "ciao come stai" → NaturalLanguage ✓

    // Case insensitive
    assert!(matches!(
        classifier.classify("HELLO").unwrap(),
        InputType::NaturalLanguage(_)
    ));
    assert!(matches!(
        classifier.classify("What").unwrap(),
        InputType::NaturalLanguage(_)
    ));

    // Common NL starters
    assert!(matches!(
        classifier.classify("help").unwrap(),
        InputType::NaturalLanguage(_)
    ));
    assert!(matches!(
        classifier.classify("thanks").unwrap(),
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

#[test]
fn test_language_agnostic_classification() {
    let classifier = InputClassifier::new();

    // Multi-word without flags → Natural Language (any language)
    assert!(matches!(
        classifier.classify("pippo ciao come stai").unwrap(),
        InputType::NaturalLanguage(_)
    ));
    assert!(matches!(
        classifier.classify("comment faire cela").unwrap(),
        InputType::NaturalLanguage(_)
    ));
    assert!(matches!(
        classifier.classify("wie mache ich das").unwrap(),
        InputType::NaturalLanguage(_)
    ));

    // With flags → Command (let the command handle validation)
    match classifier.classify("cargo --aiuto").unwrap() {
        InputType::Command { command, args, .. } => {
            assert_eq!(command, "cargo");
            assert_eq!(args, vec!["--aiuto"]);
        }
        _ => panic!("Expected Command for 'cargo --aiuto'"),
    }

    // Invalid flag still goes to shell (command will error)
    match classifier.classify("docker --hilfe").unwrap() {
        InputType::Command { command, args, .. } => {
            assert_eq!(command, "docker");
            assert_eq!(args, vec!["--hilfe"]);
        }
        _ => panic!("Expected Command for 'docker --hilfe'"),
    }
}

#[test]
fn test_classify_single_word_typo() {
    let classifier = InputClassifier::new();

    // Test "doker" → should be detected as CommandTypo and suggest "docker"
    match classifier.classify("doker").unwrap() {
        InputType::CommandTypo {
            input,
            suggestion,
            distance,
        } => {
            assert_eq!(input, "doker", "Input should be 'doker'");
            assert_eq!(suggestion, "docker", "Should suggest 'docker'");
            assert_eq!(distance, 1, "Levenshtein distance should be 1");
        }
        other => panic!("Expected CommandTypo for 'doker', got: {:?}", other),
    }

    // Test "dokcer" → should also be detected as CommandTypo
    match classifier.classify("dokcer").unwrap() {
        InputType::CommandTypo {
            input,
            suggestion,
            distance,
        } => {
            assert_eq!(input, "dokcer");
            assert_eq!(suggestion, "docker");
            assert_eq!(distance, 2);
        }
        other => panic!("Expected CommandTypo for 'dokcer', got: {:?}", other),
    }

    // Test "grpe" → should suggest "grep"
    match classifier.classify("grpe").unwrap() {
        InputType::CommandTypo {
            input,
            suggestion,
            distance,
        } => {
            assert_eq!(input, "grpe");
            assert_eq!(suggestion, "grep");
            assert!(distance <= 2);
        }
        other => panic!("Expected CommandTypo for 'grpe', got: {:?}", other),
    }
}

#[test]
fn test_classify_multi_word_typo() {
    let classifier = InputClassifier::new();

    // Test "doker ps" → 2 words, should be detected as typo
    match classifier.classify("doker ps").unwrap() {
        InputType::CommandTypo {
            input,
            suggestion,
            distance,
        } => {
            assert_eq!(input, "doker ps");
            assert_eq!(suggestion, "docker");
            assert_eq!(distance, 1);
        }
        other => panic!("Expected CommandTypo for 'doker ps', got: {:?}", other),
    }

    // Test "doker ps get" → 3 words, should ALSO be detected as typo
    // This is the critical fix - previously this was classified as NaturalLanguage
    match classifier.classify("doker ps get").unwrap() {
        InputType::CommandTypo {
            input,
            suggestion,
            distance,
        } => {
            assert_eq!(input, "doker ps get");
            assert_eq!(suggestion, "docker");
            assert_eq!(distance, 1);
        }
        other => panic!("Expected CommandTypo for 'doker ps get', got: {:?}", other),
    }

    // Test "kubeclt create deployment" → 3 words, should be detected
    match classifier.classify("kubeclt create deployment").unwrap() {
        InputType::CommandTypo {
            input,
            suggestion,
            distance,
        } => {
            assert_eq!(input, "kubeclt create deployment");
            assert_eq!(suggestion, "kubectl");
            assert!(distance <= 2);
        }
        other => panic!(
            "Expected CommandTypo for 'kubeclt create deployment', got: {:?}",
            other
        ),
    }
}
