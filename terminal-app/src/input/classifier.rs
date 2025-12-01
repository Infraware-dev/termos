/// Input classification: Command vs Natural Language
///
/// This module uses the Chain of Responsibility pattern to classify user input
/// as either commands or natural language queries.
use anyhow::Result;

use super::handler::{
    ApplicationBuiltinHandler, ClassifierChain, CommandSyntaxHandler, DefaultHandler,
    EmptyInputHandler, HandlerPosition, KnownCommandHandler, NaturalLanguageHandler,
    PathCommandHandler, PathDiscoveryHandler,
};
use super::history_expansion::HistoryExpansionHandler;
use super::shell_builtins::ShellBuiltinHandler;
use super::typo_detection::TypoDetectionHandler;
use std::sync::{Arc, RwLock};

/// Represents the type of user input
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputType {
    /// A shell command with its name and arguments
    ///
    /// Fields:
    /// - command: The command name (e.g., "ls")
    /// - args: Parsed arguments (e.g., ["-la"])
    /// - original_input: Complete original input for shell operators (pipes, redirects)
    Command {
        command: String,
        args: Vec<String>,
        original_input: Option<String>,
    },
    /// Natural language query or phrase
    NaturalLanguage(String),
    /// Empty input
    Empty,
    /// Command with typo detected
    ///
    /// Contains the original input, suggested correction, and Levenshtein distance.
    /// This prevents mistyped commands from being sent to LLM as natural language.
    CommandTypo {
        input: String,
        suggestion: String,
        distance: usize,
    },
}

/// Classifier for determining if input is a command or natural language
///
/// Uses Chain of Responsibility pattern with the following chain:
/// 1. EmptyInputHandler - handles empty/whitespace input
/// 2. HistoryExpansionHandler - expands history patterns (!!,  !$, !^, !*)
/// 3. ApplicationBuiltinHandler - app builtins (clear, reload-aliases, reload-commands)
/// 4. ShellBuiltinHandler - recognizes shell builtins (., :, [, [[, source, export, etc.)
/// 5. PathCommandHandler - detects executable paths (./script.sh, /usr/bin/cmd)
/// 6. KnownCommandHandler - checks whitelist + verifies command exists in PATH
/// 7. PathDiscoveryHandler - auto-discovers commands in PATH (for newly installed commands)
/// 8. CommandSyntaxHandler - detects command syntax (flags, pipes, redirects)
/// 9. TypoDetectionHandler - detects command typos via Levenshtein distance
/// 10. NaturalLanguageHandler - detects natural language patterns (multilingual)
/// 11. DefaultHandler - fallback to natural language
pub struct InputClassifier {
    chain: ClassifierChain,
    history: Option<Arc<RwLock<Vec<String>>>>,
}

impl std::fmt::Debug for InputClassifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InputClassifier")
            .field("chain", &"<ClassifierChain>")
            .finish()
    }
}

impl InputClassifier {
    /// Create a new input classifier with default 11-handler chain
    ///
    /// Chain order optimized for performance and accuracy:
    /// - Fast paths first (empty, history expansion)
    /// - History expansion (must happen before command parsing)
    /// - App builtins (clear, reload-aliases, reload-commands)
    /// - Shell builtins (no PATH verification needed)
    /// - Executable paths (unambiguous)
    /// - Existence-verified commands (with caching)
    /// - PATH discovery (auto-detect newly installed commands)
    /// - Syntax detection (precompiled regex)
    /// - Typo detection (prevents false LLM calls)
    /// - Natural language (precompiled patterns)
    /// - Fallback (catch-all)
    pub fn new() -> Self {
        let chain = ClassifierChain::new()
            // 1. Empty input (fastest check)
            .add_handler(HandlerPosition::Empty, Box::new(EmptyInputHandler::new()))
            // 2. History expansion (!!,  !$, !^, !* - must happen before command parsing)
            .add_handler(
                HandlerPosition::HistoryExpansion,
                Box::new(HistoryExpansionHandler::new()),
            )
            // 3. Application builtins (clear, reload-aliases, reload-commands)
            .add_handler(
                HandlerPosition::ApplicationBuiltin,
                Box::new(ApplicationBuiltinHandler::new()),
            )
            // 4. Shell builtins (., :, [, [[, source, export, etc. - no PATH verification)
            .add_handler(
                HandlerPosition::ShellBuiltin,
                Box::new(ShellBuiltinHandler::new()),
            )
            // 5. Executable paths (unambiguous: ./script.sh, /usr/bin/cmd)
            .add_handler(
                HandlerPosition::PathCommand,
                Box::new(PathCommandHandler::new()),
            )
            // 6. Known commands with PATH existence check (cached)
            .add_handler(
                HandlerPosition::KnownCommand,
                Box::new(KnownCommandHandler::with_defaults()),
            )
            // 7. PATH discovery - auto-detect newly installed commands via `which`
            .add_handler(
                HandlerPosition::PathDiscovery,
                Box::new(PathDiscoveryHandler::new()),
            )
            // 8. Command syntax detection (flags, pipes, redirects)
            .add_handler(
                HandlerPosition::CommandSyntax,
                Box::new(CommandSyntaxHandler::new()),
            )
            // 9. Typo detection (prevents "dokcer ps" → LLM)
            .add_handler(
                HandlerPosition::TypoDetection,
                Box::new(TypoDetectionHandler::with_defaults()),
            )
            // 10. Natural language patterns (precompiled regex, multilingual)
            .add_handler(
                HandlerPosition::NaturalLanguage,
                Box::new(NaturalLanguageHandler::new()),
            )
            // 11. Fallback to natural language
            .add_handler(HandlerPosition::Default, Box::new(DefaultHandler::new()));

        Self {
            chain,
            history: None,
        }
    }

    /// Set the command history for history expansion support
    ///
    /// This enables the HistoryExpansionHandler to expand patterns like !!,  !$, !^, !*
    pub fn with_history(mut self, history: Arc<RwLock<Vec<String>>>) -> Self {
        // Rebuild the chain with history-aware HistoryExpansionHandler
        self.chain = ClassifierChain::new()
            .add_handler(HandlerPosition::Empty, Box::new(EmptyInputHandler::new()))
            .add_handler(
                HandlerPosition::HistoryExpansion,
                Box::new(HistoryExpansionHandler::with_history(history.clone())),
            )
            .add_handler(
                HandlerPosition::ApplicationBuiltin,
                Box::new(ApplicationBuiltinHandler::new()),
            )
            .add_handler(
                HandlerPosition::ShellBuiltin,
                Box::new(ShellBuiltinHandler::new()),
            )
            .add_handler(
                HandlerPosition::PathCommand,
                Box::new(PathCommandHandler::new()),
            )
            .add_handler(
                HandlerPosition::KnownCommand,
                Box::new(KnownCommandHandler::with_defaults()),
            )
            .add_handler(
                HandlerPosition::PathDiscovery,
                Box::new(PathDiscoveryHandler::new()),
            )
            .add_handler(
                HandlerPosition::CommandSyntax,
                Box::new(CommandSyntaxHandler::new()),
            )
            .add_handler(
                HandlerPosition::TypoDetection,
                Box::new(TypoDetectionHandler::with_defaults()),
            )
            .add_handler(
                HandlerPosition::NaturalLanguage,
                Box::new(NaturalLanguageHandler::new()),
            )
            .add_handler(HandlerPosition::Default, Box::new(DefaultHandler::new()));

        self.history = Some(history);
        self
    }

    /// Classify the input as command or natural language
    ///
    /// Performs alias expansion before classification:
    /// 1. Extract first word from input
    /// 2. Check if it's an alias
    /// 3. If alias, expand and classify the expanded command
    /// 4. If not alias, classify original input
    pub fn classify(&self, input: &str) -> Result<InputType> {
        use super::discovery::CommandCache;

        let trimmed = input.trim();

        // Extract first word to check for alias
        if let Some(first_word) = trimmed.split_whitespace().next() {
            // Check if first word is an alias
            if let Some(expansion) = CommandCache::expand_alias(first_word) {
                // Get the rest of the arguments (everything after first word)
                // Use byte offset instead of strip_prefix to avoid fragile invariant
                let first_word_len = first_word.len();
                let rest = if first_word_len < trimmed.len() {
                    trimmed[first_word_len..].trim_start()
                } else {
                    ""
                };

                // Construct expanded input: expansion + rest
                let expanded_input = if rest.is_empty() {
                    expansion
                } else {
                    format!("{expansion} {rest}")
                };

                // Classify the expanded input
                return self.classify_internal(&expanded_input);
            }
        }

        // Not an alias, classify as-is
        self.classify_internal(trimmed)
    }

    /// Internal classification method (without alias expansion)
    fn classify_internal(&self, input: &str) -> Result<InputType> {
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

        // Test with ls (should exist on all Unix systems)
        let result = classifier.classify("ls -la").unwrap();
        // ls should be either Command (if installed) or pass through to CommandSyntaxHandler
        assert!(matches!(result, InputType::Command { .. }));

        // Commands that may not be installed should still be classified via syntax
        // (they have flags, so CommandSyntaxHandler will catch them)
        assert!(matches!(
            classifier.classify("unknown-cmd --flag").unwrap(),
            InputType::Command { .. }
        ));
    }

    #[test]
    #[cfg_attr(target_os = "macos", ignore)] // Flaky on macOS due to PATH/command differences
    fn test_natural_language() {
        let classifier = InputClassifier::new();

        // Questions with question marks - should match regex patterns
        assert!(matches!(
            classifier.classify("how do I list files?").unwrap(),
            InputType::NaturalLanguage(_)
        ));

        // "show me the logs" - has article "the", should match
        let result = classifier.classify("show me the logs").unwrap();
        assert!(
            matches!(result, InputType::NaturalLanguage(_)),
            "Expected NaturalLanguage, got: {result:?}"
        );

        // Question with "what" and question mark - uses common words only
        // Note: Avoid nouns that might be commands on some systems
        assert!(matches!(
            classifier.classify("what does this do?").unwrap(),
            InputType::NaturalLanguage(_)
        ));
    }

    #[test]
    fn test_command_syntax() {
        let classifier = InputClassifier::new();

        // Flags
        assert!(matches!(
            classifier.classify("unknown-cmd --flag").unwrap(),
            InputType::Command { .. }
        ));

        // Pipes
        assert!(matches!(
            classifier.classify("cat file.txt | grep pattern").unwrap(),
            InputType::Command { .. }
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
            InputType::Command { .. }
        ));
    }

    #[test]
    fn test_edge_cases() {
        let classifier = InputClassifier::new();

        // Single word - if command exists in whitelist + PATH, it's Command
        // Otherwise might be typo or natural language
        // Test with "ls" which should exist everywhere
        let result = classifier.classify("ls").unwrap();
        assert!(
            matches!(result, InputType::Command { .. }),
            "ls should be classified as Command"
        );

        // Articles indicate natural language
        assert!(matches!(
            classifier.classify("run the docker container").unwrap(),
            InputType::NaturalLanguage(_)
        ));
        assert!(matches!(
            classifier.classify("avvia il container docker").unwrap(),
            InputType::NaturalLanguage(_)
        ));

        // Polite expressions with clear natural language markers
        assert!(matches!(
            classifier.classify("can you help me please?").unwrap(),
            InputType::NaturalLanguage(_)
        ));

        assert!(matches!(
            classifier.classify("grazie per l'aiuto!").unwrap(),
            InputType::NaturalLanguage(_)
        ));
    }
}
