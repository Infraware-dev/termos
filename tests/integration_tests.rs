/// Integration tests for Infraware Terminal
use infraware_terminal::input::{InputClassifier, InputType};
use infraware_terminal::executor::CommandExecutor;
use infraware_terminal::llm::{MockLLMClient, ResponseRenderer};

#[tokio::test]
async fn test_end_to_end_command_execution() {
    let classifier = InputClassifier::new();

    // Classify input
    let input = "echo test";
    let classified = classifier.classify(input).unwrap();

    // Execute if it's a command
    match classified {
        InputType::Command(cmd, args) => {
            let result = CommandExecutor::execute(&cmd, &args).await.unwrap();
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
}

#[test]
fn test_command_classification_accuracy() {
    let classifier = InputClassifier::new();

    let test_cases = vec![
        ("ls -la", true),
        ("docker ps", true),
        ("kubectl get pods", true),
        ("how do I list files", false),
        ("what is kubernetes", false),
        ("show me the logs", false),
        ("cat file.txt | grep pattern", true),
        ("explain docker to me", false),
    ];

    for (input, should_be_command) in test_cases {
        let result = classifier.classify(input).unwrap();
        let is_command = matches!(result, InputType::Command(_, _));
        assert_eq!(
            is_command, should_be_command,
            "Failed for input: {}",
            input
        );
    }
}
