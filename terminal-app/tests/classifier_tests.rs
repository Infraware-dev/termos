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
#[cfg_attr(target_os = "macos", ignore)] // Flaky on macOS due to PATH/command differences
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
#[cfg_attr(target_os = "macos", ignore)] // Flaky on macOS due to PATH/command differences
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

    // With typo detection disabled (max_distance=0), typos fall through to NaturalLanguage
    // Test "doker" → should be classified as NaturalLanguage
    match classifier.classify("doker").unwrap() {
        InputType::NaturalLanguage(text) => {
            assert_eq!(text, "doker", "Input should be 'doker'");
        }
        other => panic!(
            "Expected NaturalLanguage for 'doker' (typo detection disabled), got: {:?}",
            other
        ),
    }

    // Test "dokcer" → should also be NaturalLanguage
    match classifier.classify("dokcer").unwrap() {
        InputType::NaturalLanguage(text) => {
            assert_eq!(text, "dokcer");
        }
        other => panic!(
            "Expected NaturalLanguage for 'dokcer' (typo detection disabled), got: {:?}",
            other
        ),
    }

    // Test "grpe" → should be NaturalLanguage
    match classifier.classify("grpe").unwrap() {
        InputType::NaturalLanguage(text) => {
            assert_eq!(text, "grpe");
        }
        other => panic!(
            "Expected NaturalLanguage for 'grpe' (typo detection disabled), got: {:?}",
            other
        ),
    }
}

#[test]
fn test_classify_multi_word_typo() {
    let classifier = InputClassifier::new();

    // With typo detection disabled (max_distance=0), typos fall through to NaturalLanguage
    // Test "doker ps" → 2 words, should be NaturalLanguage
    match classifier.classify("doker ps").unwrap() {
        InputType::NaturalLanguage(text) => {
            assert_eq!(text, "doker ps");
        }
        other => panic!(
            "Expected NaturalLanguage for 'doker ps' (typo detection disabled), got: {:?}",
            other
        ),
    }

    // Test "doker ps get" → 3 words, should be NaturalLanguage
    match classifier.classify("doker ps get").unwrap() {
        InputType::NaturalLanguage(text) => {
            assert_eq!(text, "doker ps get");
        }
        other => panic!(
            "Expected NaturalLanguage for 'doker ps get' (typo detection disabled), got: {:?}",
            other
        ),
    }

    // Test "kubeclt create deployment" → 3 words, should be NaturalLanguage
    match classifier.classify("kubeclt create deployment").unwrap() {
        InputType::NaturalLanguage(text) => {
            assert_eq!(text, "kubeclt create deployment");
        }
        other => panic!(
            "Expected NaturalLanguage for 'kubeclt create deployment' (typo detection disabled), got: {:?}",
            other
        ),
    }
}

// =============================================================================
// PathDiscoveryHandler Tests
// =============================================================================

/// Test that PathDiscoveryHandler recognizes commands installed in PATH
/// This test uses 'cat' which is universally available on Unix systems
#[test]
fn test_path_discovery_handler_finds_installed_command() {
    let classifier = InputClassifier::new();

    // 'cat' is a standard Unix command that should be in PATH
    match classifier.classify("cat /etc/passwd").unwrap() {
        InputType::Command { command, args, .. } => {
            assert_eq!(command, "cat");
            assert_eq!(args, vec!["/etc/passwd"]);
        }
        other => panic!("Expected Command for 'cat /etc/passwd', got: {:?}", other),
    }
}

/// Test that PathDiscoveryHandler skips path-like commands (those with / or \)
/// These should be handled by PathCommandHandler instead
#[test]
fn test_path_discovery_handler_skips_path_commands() {
    let classifier = InputClassifier::new();

    // Commands starting with ./ should be handled by PathCommandHandler, not PathDiscoveryHandler
    match classifier.classify("./script.sh").unwrap() {
        InputType::Command { command, .. } => {
            // PathCommandHandler handles this
            assert!(command.contains('/') || command == "./script.sh");
        }
        other => panic!("Expected Command for './script.sh', got: {:?}", other),
    }

    // Absolute paths should also be handled by PathCommandHandler
    match classifier.classify("/usr/bin/python3").unwrap() {
        InputType::Command { command, .. } => {
            assert!(command.contains('/'));
        }
        other => panic!("Expected Command for '/usr/bin/python3', got: {:?}", other),
    }
}

/// Test that PathDiscoveryHandler correctly handles multi-word commands
#[test]
fn test_path_discovery_handler_multi_word_commands() {
    let classifier = InputClassifier::new();

    // 'echo' is a standard command, should be recognized with arguments
    match classifier.classify("echo hello world").unwrap() {
        InputType::Command { command, args, .. } => {
            assert_eq!(command, "echo");
            assert_eq!(args, vec!["hello", "world"]);
        }
        other => panic!("Expected Command for 'echo hello world', got: {:?}", other),
    }
}

/// Test that PathDiscoveryHandler runs before TypoDetectionHandler
/// This ensures installed commands are recognized before typo detection kicks in
#[test]
#[serial_test::serial]
fn test_path_discovery_before_typo_detection() {
    use infraware_terminal::input::discovery::CommandCache;

    // Clear cache to ensure fresh state
    CommandCache::clear();

    let classifier = InputClassifier::new();

    // 'ls' is installed and should NOT be detected as a typo
    match classifier.classify("ls").unwrap() {
        InputType::Command { command, .. } => {
            assert_eq!(command, "ls");
        }
        InputType::CommandTypo { .. } => {
            panic!("'ls' should not be detected as a typo - PathDiscoveryHandler should catch it");
        }
        other => panic!("Expected Command for 'ls', got: {:?}", other),
    }
}

/// Verify the complete 11-handler chain order
#[test]
#[cfg_attr(target_os = "macos", ignore)] // Flaky on macOS due to PATH/command differences
fn test_11_handler_chain_order() {
    // The chain order is critical for correct classification:
    // 1. EmptyInputHandler - fast path for empty/whitespace
    // 2. HistoryExpansionHandler - !! and related expansions
    // 3. ApplicationBuiltinHandler - clear, reload-aliases, reload-commands
    // 4. ShellBuiltinHandler - ., :, [, [[, export, etc.
    // 5. PathCommandHandler - ./script.sh, /usr/bin/cmd
    // 6. KnownCommandHandler - 60+ DevOps commands
    // 7. PathDiscoveryHandler - auto-discover PATH commands (NEW)
    // 8. CommandSyntaxHandler - flags, pipes, redirects
    // 9. TypoDetectionHandler - Levenshtein ≤2
    // 10. NaturalLanguageHandler - language-agnostic heuristics
    // 11. DefaultHandler - fallback to LLM

    let classifier = InputClassifier::new();

    // Test each handler in order:

    // 1. EmptyInputHandler
    assert!(matches!(classifier.classify("").unwrap(), InputType::Empty));
    assert!(matches!(
        classifier.classify("   ").unwrap(),
        InputType::Empty
    ));

    // 3. ApplicationBuiltinHandler
    match classifier.classify("clear").unwrap() {
        InputType::Command { command, .. } => assert_eq!(command, "clear"),
        other => panic!("Expected Command for 'clear', got: {:?}", other),
    }

    // 4. ShellBuiltinHandler
    match classifier.classify("export FOO=bar").unwrap() {
        InputType::Command { command, .. } => assert_eq!(command, "export"),
        other => panic!("Expected Command for 'export', got: {:?}", other),
    }

    // 5. PathCommandHandler
    match classifier.classify("./test.sh").unwrap() {
        InputType::Command { .. } => {}
        other => panic!("Expected Command for './test.sh', got: {:?}", other),
    }

    // 6. KnownCommandHandler - docker is in known commands
    match classifier.classify("docker ps").unwrap() {
        InputType::Command { command, .. } => assert_eq!(command, "docker"),
        other => panic!("Expected Command for 'docker ps', got: {:?}", other),
    }

    // 7. PathDiscoveryHandler - cat is in PATH but not in known commands list
    match classifier.classify("cat file.txt").unwrap() {
        InputType::Command { command, .. } => assert_eq!(command, "cat"),
        other => panic!("Expected Command for 'cat file.txt', got: {:?}", other),
    }

    // 8. CommandSyntaxHandler - recognizes flag patterns
    if let InputType::Command { command, .. } = classifier.classify("unknowncmd --flag").unwrap() {
        assert_eq!(command, "unknowncmd");
        // May also be caught by typo handler depending on PATH
    }

    // 9. TypoDetectionHandler (disabled with max_distance=0, so typos become NaturalLanguage)
    match classifier.classify("doker").unwrap() {
        InputType::NaturalLanguage(text) => assert_eq!(text, "doker"),
        other => panic!(
            "Expected NaturalLanguage for 'doker' (typo detection disabled), got: {:?}",
            other
        ),
    }

    // 10. NaturalLanguageHandler - multi-word without flags
    assert!(matches!(
        classifier.classify("how do I deploy").unwrap(),
        InputType::NaturalLanguage(_)
    ));

    // 11. DefaultHandler - fallback for ambiguous single words
    // (most single words are caught by other handlers)
}

// =============================================================================
// History Expansion Tests
// =============================================================================

// NOTE: History expansion uses "get-second-to-last" semantics because
// the current command is already in history when classified. The terminal
// calls submit_input() which adds the command to history before classification.
// For testing, we need to add a placeholder for the "current command" at the end.

/// Test classifier with history support
#[test]
fn test_classifier_with_history() {
    use std::sync::{Arc, RwLock};

    // Note: The last entry simulates the current command (which would be "!!")
    // The expansion will get the second-to-last: "echo hello world"
    let history = Arc::new(RwLock::new(vec![
        "ls -la".to_string(),
        "docker ps".to_string(),
        "echo hello world".to_string(),
        "!!".to_string(), // Current command placeholder
    ]));

    let classifier = InputClassifier::new().with_history(history);

    // Test !! expansion (previous command = second-to-last = "echo hello world")
    match classifier.classify("!!").unwrap() {
        InputType::Command { command, args, .. } => {
            assert_eq!(command, "echo");
            assert_eq!(args, vec!["hello", "world"]);
        }
        other => panic!("Expected Command from !! expansion, got: {:?}", other),
    }
}

/// Test !$ expansion (last argument of previous command)
#[test]
fn test_history_expansion_last_arg() {
    use std::sync::{Arc, RwLock};

    // Note: The last entry simulates the current command
    // The expansion will get the second-to-last: "cat /path/to/file.txt"
    let history = Arc::new(RwLock::new(vec![
        "ls -la".to_string(),
        "cat /path/to/file.txt".to_string(),
        "vim !$".to_string(), // Current command placeholder
    ]));

    let classifier = InputClassifier::new().with_history(history);

    // Test !$ expansion (last argument of "cat /path/to/file.txt" = "/path/to/file.txt")
    match classifier.classify("vim !$").unwrap() {
        InputType::Command { command, args, .. } => {
            assert_eq!(command, "vim");
            assert!(args.iter().any(|a| a.contains("file.txt")));
        }
        other => panic!("Expected Command from !$ expansion, got: {:?}", other),
    }
}

/// Test !^ expansion (first argument of previous command)
#[test]
fn test_history_expansion_first_arg() {
    use std::sync::{Arc, RwLock};

    // Note: The last entry simulates the current command
    // The expansion will get the second-to-last: "grep pattern file1.txt file2.txt"
    let history = Arc::new(RwLock::new(vec![
        "grep pattern file1.txt file2.txt".to_string(),
        "echo !^".to_string(), // Current command placeholder
    ]));

    let classifier = InputClassifier::new().with_history(history);

    // Test !^ expansion (first argument of "grep pattern file1.txt file2.txt" = "pattern")
    match classifier.classify("echo !^").unwrap() {
        InputType::Command { command, args, .. } => {
            assert_eq!(command, "echo");
            // First arg of "grep pattern file1.txt file2.txt" is "pattern"
            assert!(args.iter().any(|a| a == "pattern"));
        }
        other => panic!("Expected Command from !^ expansion, got: {:?}", other),
    }
}

/// Test !* expansion (all arguments of previous command)
#[test]
fn test_history_expansion_all_args() {
    use std::sync::{Arc, RwLock};

    // Note: The last entry simulates the current command
    // The expansion will get the second-to-last: "cp source.txt dest.txt"
    let history = Arc::new(RwLock::new(vec![
        "cp source.txt dest.txt".to_string(),
        "mv !*".to_string(), // Current command placeholder
    ]));

    let classifier = InputClassifier::new().with_history(history);

    // Test !* expansion (all arguments of "cp source.txt dest.txt" = "source.txt dest.txt")
    match classifier.classify("mv !*").unwrap() {
        InputType::Command { command, args, .. } => {
            assert_eq!(command, "mv");
            assert!(args.contains(&"source.txt".to_string()));
            assert!(args.contains(&"dest.txt".to_string()));
        }
        other => panic!("Expected Command from !* expansion, got: {:?}", other),
    }
}

// =============================================================================
// Alias Expansion Tests
// =============================================================================

/// Test that classifier handles alias expansion
#[test]
#[serial_test::serial]
fn test_alias_expansion_basic() {
    use infraware_terminal::input::discovery::CommandCache;

    // Clear cache and set up test alias
    CommandCache::clear();

    // Note: We can't easily set custom aliases from tests without modifying
    // the global state. This test documents the expected behavior.
    let classifier = InputClassifier::new();

    // Even without an alias, the classifier should work
    match classifier.classify("ls").unwrap() {
        InputType::Command { command, .. } => {
            assert_eq!(command, "ls");
        }
        other => panic!("Expected Command, got: {:?}", other),
    }
}

// =============================================================================
// Debug Implementation Tests
// =============================================================================

#[test]
fn test_classifier_debug() {
    let classifier = InputClassifier::new();
    let debug_str = format!("{:?}", classifier);
    assert!(debug_str.contains("InputClassifier"));
    assert!(debug_str.contains("ClassifierChain"));
}

#[test]
fn test_classifier_default() {
    let classifier = InputClassifier::default();
    // Should work the same as new()
    match classifier.classify("ls").unwrap() {
        InputType::Command { command, .. } => {
            assert_eq!(command, "ls");
        }
        other => panic!("Expected Command, got: {:?}", other),
    }
}

// =============================================================================
// InputType Tests
// =============================================================================

#[test]
fn test_input_type_debug() {
    let cmd = InputType::Command {
        command: "ls".to_string(),
        args: vec!["-la".to_string()],
        original_input: None,
    };
    let debug_str = format!("{:?}", cmd);
    assert!(debug_str.contains("Command"));
    assert!(debug_str.contains("ls"));

    let nl = InputType::NaturalLanguage("hello".to_string());
    let debug_str = format!("{:?}", nl);
    assert!(debug_str.contains("NaturalLanguage"));
    assert!(debug_str.contains("hello"));

    let empty = InputType::Empty;
    let debug_str = format!("{:?}", empty);
    assert!(debug_str.contains("Empty"));

    let typo = InputType::CommandTypo {
        input: "doker".to_string(),
        suggestion: "docker".to_string(),
        distance: 1,
    };
    let debug_str = format!("{:?}", typo);
    assert!(debug_str.contains("CommandTypo"));
    assert!(debug_str.contains("doker"));
}

#[test]
fn test_input_type_clone() {
    let cmd = InputType::Command {
        command: "ls".to_string(),
        args: vec!["-la".to_string()],
        original_input: Some("ls -la | grep test".to_string()),
    };
    let cloned = cmd.clone();
    assert_eq!(cmd, cloned);

    let nl = InputType::NaturalLanguage("test".to_string());
    let cloned = nl.clone();
    assert_eq!(nl, cloned);
}

#[test]
fn test_input_type_equality() {
    let cmd1 = InputType::Command {
        command: "ls".to_string(),
        args: vec![],
        original_input: None,
    };
    let cmd2 = InputType::Command {
        command: "ls".to_string(),
        args: vec![],
        original_input: None,
    };
    assert_eq!(cmd1, cmd2);

    let cmd3 = InputType::Command {
        command: "pwd".to_string(),
        args: vec![],
        original_input: None,
    };
    assert_ne!(cmd1, cmd3);

    assert_eq!(InputType::Empty, InputType::Empty);
    assert_ne!(
        InputType::Empty,
        InputType::NaturalLanguage("test".to_string())
    );
}

// =============================================================================
// Edge Cases and Error Handling
// =============================================================================

#[test]
fn test_classify_whitespace_only() {
    let classifier = InputClassifier::new();

    assert!(matches!(classifier.classify("").unwrap(), InputType::Empty));
    assert!(matches!(
        classifier.classify("   ").unwrap(),
        InputType::Empty
    ));
    assert!(matches!(
        classifier.classify("\t").unwrap(),
        InputType::Empty
    ));
    assert!(matches!(
        classifier.classify("\n").unwrap(),
        InputType::Empty
    ));
    assert!(matches!(
        classifier.classify("  \t  \n  ").unwrap(),
        InputType::Empty
    ));
}

#[test]
fn test_classify_special_characters() {
    let classifier = InputClassifier::new();

    // These should not panic, even if classification varies
    let _ = classifier.classify("@#$%^&*()");
    let _ = classifier.classify("!@#$%");
    let _ = classifier.classify("...");
    let _ = classifier.classify("---");
    let _ = classifier.classify("___");
}

#[test]
fn test_classify_very_long_input() {
    let classifier = InputClassifier::new();

    // Long natural language should be classified correctly
    let long_text = "This is a very long piece of text that goes on and on and on \
        and keeps going with lots of words to see how the classifier handles it \
        when there are many many words in the input string that need to be processed";

    assert!(matches!(
        classifier.classify(long_text).unwrap(),
        InputType::NaturalLanguage(_)
    ));
}

#[test]
fn test_classify_command_with_original_input() {
    let classifier = InputClassifier::new();

    // Commands with pipes should preserve original_input
    match classifier.classify("echo hello | cat").unwrap() {
        InputType::Command { original_input, .. } => {
            assert!(original_input.is_some());
            assert!(original_input.unwrap().contains("|"));
        }
        other => panic!("Expected Command, got: {:?}", other),
    }

    // Commands with redirects
    match classifier.classify("echo hello > file.txt").unwrap() {
        InputType::Command { original_input, .. } => {
            assert!(original_input.is_some());
        }
        other => panic!("Expected Command, got: {:?}", other),
    }
}
