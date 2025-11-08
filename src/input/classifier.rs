/// Input classification: Command vs Natural Language
///
/// This module uses the Chain of Responsibility pattern to classify user input
/// as either commands or natural language queries.
use anyhow::Result;

use super::handler::{
    ClassifierChain, CommandSyntaxHandler, DefaultHandler, EmptyInputHandler, KnownCommandHandler,
    NaturalLanguageHandler,
};

/// Represents the type of user input
#[derive(Debug, Clone, PartialEq)]
pub enum InputType {
    /// A shell command with its name and arguments
    Command(String, Vec<String>),
    /// Natural language query or phrase
    NaturalLanguage(String),
    /// Empty input
    Empty,
}

/// Classifier for determining if input is a command or natural language
///
/// Uses Chain of Responsibility pattern with the following chain:
/// 1. EmptyInputHandler - handles empty/whitespace input
/// 2. KnownCommandHandler - checks against whitelist of known commands
/// 3. CommandSyntaxHandler - detects command syntax (flags, pipes, paths)
/// 4. NaturalLanguageHandler - detects natural language patterns
/// 5. DefaultHandler - fallback to natural language
pub struct InputClassifier {
    chain: ClassifierChain,
}

impl InputClassifier {
    /// Create a new input classifier with default chain
    pub fn new() -> Self {
        let chain = ClassifierChain::new()
            .add_handler(Box::new(EmptyInputHandler::new()))
            .add_handler(Box::new(KnownCommandHandler::with_defaults()))
            .add_handler(Box::new(CommandSyntaxHandler::new()))
            .add_handler(Box::new(NaturalLanguageHandler::new()))
            .add_handler(Box::new(DefaultHandler::new()));

        Self { chain }
    }

    /// Create a classifier with a custom chain
    #[allow(dead_code)]
    pub fn with_chain(chain: ClassifierChain) -> Self {
        Self { chain }
    }

    /// Classify the input as command or natural language
    pub fn classify(&self, input: &str) -> Result<InputType> {
        // Process through the chain of handlers
        match self.chain.process(input) {
            Some(result) => Ok(result),
            None => {
                // This should never happen with DefaultHandler at the end,
                // but we handle it gracefully
                Ok(InputType::NaturalLanguage(input.trim().to_string()))
            }
        }
    }

    /// Add a command to the known commands list (legacy method for backward compatibility)
    ///
    /// Note: This is a legacy method. In the new architecture, you should
    /// construct a custom KnownCommandHandler if you need custom commands.
    #[deprecated(
        since = "0.2.0",
        note = "Use custom KnownCommandHandler in chain instead"
    )]
    #[allow(dead_code)]
    pub fn add_known_command(&mut self, _command: String) {
        // This is a no-op in the new architecture
        // Users should create a custom KnownCommandHandler with their commands
        // and build a custom chain using with_chain()
    }
}

impl Default for InputClassifier {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_known_commands() {
        let classifier = InputClassifier::new();

        // Basic commands
        assert!(matches!(
            classifier.classify("ls -la").unwrap(),
            InputType::Command(_, _)
        ));
        assert!(matches!(
            classifier.classify("docker ps").unwrap(),
            InputType::Command(_, _)
        ));
        assert!(matches!(
            classifier.classify("kubectl get pods").unwrap(),
            InputType::Command(_, _)
        ));
    }

    #[test]
    fn test_natural_language() {
        let classifier = InputClassifier::new();

        assert!(matches!(
            classifier.classify("how do I list files?").unwrap(),
            InputType::NaturalLanguage(_)
        ));
        assert!(matches!(
            classifier.classify("what is kubernetes").unwrap(),
            InputType::NaturalLanguage(_)
        ));
        assert!(matches!(
            classifier.classify("show me the logs").unwrap(),
            InputType::NaturalLanguage(_)
        ));
    }

    #[test]
    fn test_command_syntax() {
        let classifier = InputClassifier::new();

        // Flags
        assert!(matches!(
            classifier.classify("unknown-cmd --flag").unwrap(),
            InputType::Command(_, _)
        ));

        // Pipes
        assert!(matches!(
            classifier.classify("cat file.txt | grep pattern").unwrap(),
            InputType::Command(_, _)
        ));
    }

    #[test]
    fn test_multilingual_italian() {
        let classifier = InputClassifier::new();

        // Italian questions
        assert!(matches!(
            classifier.classify("come posso listare i file?").unwrap(),
            InputType::NaturalLanguage(_)
        ));
        assert!(matches!(
            classifier.classify("cosa è docker").unwrap(),
            InputType::NaturalLanguage(_)
        ));
        assert!(matches!(
            classifier.classify("mostrami i log del container").unwrap(),
            InputType::NaturalLanguage(_)
        ));
        assert!(matches!(
            classifier
                .classify("spiegami kubernetes per favore")
                .unwrap(),
            InputType::NaturalLanguage(_)
        ));

        // Commands should still work
        assert!(matches!(
            classifier.classify("docker ps").unwrap(),
            InputType::Command(_, _)
        ));
    }

    #[test]
    fn test_multilingual_spanish() {
        let classifier = InputClassifier::new();

        // Spanish questions
        assert!(matches!(
            classifier.classify("cómo puedo listar archivos?").unwrap(),
            InputType::NaturalLanguage(_)
        ));
        assert!(matches!(
            classifier.classify("qué es kubernetes").unwrap(),
            InputType::NaturalLanguage(_)
        ));
        assert!(matches!(
            classifier.classify("muestrame los logs").unwrap(),
            InputType::NaturalLanguage(_)
        ));
        assert!(matches!(
            classifier.classify("ayuda con docker por favor").unwrap(),
            InputType::NaturalLanguage(_)
        ));
    }

    #[test]
    fn test_multilingual_french() {
        let classifier = InputClassifier::new();

        // French questions
        assert!(matches!(
            classifier.classify("comment lister les fichiers?").unwrap(),
            InputType::NaturalLanguage(_)
        ));
        assert!(matches!(
            classifier.classify("qu'est-ce que kubernetes").unwrap(),
            InputType::NaturalLanguage(_)
        ));
        assert!(matches!(
            classifier.classify("montre-moi les logs").unwrap(),
            InputType::NaturalLanguage(_)
        ));
        assert!(matches!(
            classifier
                .classify("explique docker s'il te plaît")
                .unwrap(),
            InputType::NaturalLanguage(_)
        ));
    }

    #[test]
    fn test_multilingual_german() {
        let classifier = InputClassifier::new();

        // German questions
        assert!(matches!(
            classifier
                .classify("wie kann ich Dateien auflisten?")
                .unwrap(),
            InputType::NaturalLanguage(_)
        ));
        assert!(matches!(
            classifier.classify("was ist kubernetes").unwrap(),
            InputType::NaturalLanguage(_)
        ));
        assert!(matches!(
            classifier.classify("zeig mir die logs").unwrap(),
            InputType::NaturalLanguage(_)
        ));
        assert!(matches!(
            classifier.classify("erkläre docker bitte").unwrap(),
            InputType::NaturalLanguage(_)
        ));
    }

    #[test]
    fn test_universal_patterns() {
        let classifier = InputClassifier::new();

        // Question marks (any language)
        assert!(matches!(
            classifier.classify("¿Qué es esto?").unwrap(),
            InputType::NaturalLanguage(_)
        ));
        assert!(matches!(
            classifier.classify("Was ist das?").unwrap(),
            InputType::NaturalLanguage(_)
        ));

        // Long phrases without command syntax
        assert!(matches!(
            classifier
                .classify("I really need to understand how this works")
                .unwrap(),
            InputType::NaturalLanguage(_)
        ));
        assert!(matches!(
            classifier
                .classify("voglio capire come funziona questo sistema complesso")
                .unwrap(),
            InputType::NaturalLanguage(_)
        ));

        // Commands with paths should still be commands
        assert!(matches!(
            classifier.classify("./deploy.sh --production").unwrap(),
            InputType::Command(_, _)
        ));
    }

    #[test]
    fn test_edge_cases() {
        let classifier = InputClassifier::new();

        // Single word commands
        assert!(matches!(
            classifier.classify("htop").unwrap(),
            InputType::Command(_, _)
        ));

        // Articles indicate natural language
        assert!(matches!(
            classifier.classify("run the docker container").unwrap(),
            InputType::NaturalLanguage(_)
        ));
        assert!(matches!(
            classifier.classify("avvia il container docker").unwrap(),
            InputType::NaturalLanguage(_)
        ));

        // Polite expressions
        assert!(matches!(
            classifier.classify("help me please").unwrap(),
            InputType::NaturalLanguage(_)
        ));
        assert!(matches!(
            classifier.classify("grazie per l'aiuto").unwrap(),
            InputType::NaturalLanguage(_)
        ));
    }
}
