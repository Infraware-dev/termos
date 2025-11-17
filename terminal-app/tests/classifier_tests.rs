/// Tests for input classifier
use infraware_terminal::input::{InputClassifier, InputType};

#[test]
fn test_classify_known_commands() {
    let classifier = InputClassifier::new();

    // Basic shell commands
    match classifier.classify("ls -la").unwrap() {
        InputType::Command(cmd, args) => {
            assert_eq!(cmd, "ls");
            assert_eq!(args, vec!["-la"]);
        }
        _ => panic!("Expected Command"),
    }

    match classifier.classify("docker ps").unwrap() {
        InputType::Command(cmd, args) => {
            assert_eq!(cmd, "docker");
            assert_eq!(args, vec!["ps"]);
        }
        _ => panic!("Expected Command"),
    }

    match classifier.classify("kubectl get pods").unwrap() {
        InputType::Command(cmd, args) => {
            assert_eq!(cmd, "kubectl");
            assert_eq!(args, vec!["get", "pods"]);
        }
        _ => panic!("Expected Command"),
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

    assert!(matches!(
        classifier.classify("what is kubernetes").unwrap(),
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
        InputType::Command(cmd, _) => {
            assert_eq!(cmd, "unknowncmd");
        }
        _ => panic!("Expected Command"),
    }

    // Pipes should be recognized as commands
    match classifier.classify("cat file.txt | grep pattern").unwrap() {
        InputType::Command(_, _) => {}
        _ => panic!("Expected Command"),
    }

    // Redirects
    match classifier.classify("echo hello > file.txt").unwrap() {
        InputType::Command(_, _) => {}
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
        InputType::Command(cmd, args) => {
            assert_eq!(cmd, "ls");
            assert!(args.is_empty());
        }
        _ => panic!("Expected Command"),
    }
}
