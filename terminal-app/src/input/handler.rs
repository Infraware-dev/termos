/// Chain of Responsibility pattern for input classification
///
/// This module implements a flexible chain of handlers that can classify
/// user input as either commands or natural language queries.
use anyhow::Result;

use super::InputType;

/// Handler trait for the Chain of Responsibility pattern
///
/// Each handler in the chain can either:
/// 1. Handle the input and return a classification
/// 2. Pass it to the next handler in the chain
pub trait InputHandler: Send + Sync {
    /// Attempt to handle the input
    ///
    /// Returns:
    /// - `Some(InputType)` if this handler can classify the input
    /// - `None` if the input should be passed to the next handler
    fn handle(&self, input: &str) -> Option<InputType>;
}

/// Chain of handlers for input classification
pub struct ClassifierChain {
    handlers: Vec<Box<dyn InputHandler>>,
}

impl std::fmt::Debug for ClassifierChain {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClassifierChain")
            .field("handlers_count", &self.handlers.len())
            .finish()
    }
}

impl ClassifierChain {
    /// Create a new empty chain
    pub fn new() -> Self {
        Self {
            handlers: Vec::new(),
        }
    }

    /// Add a handler to the end of the chain
    pub fn add_handler(mut self, handler: Box<dyn InputHandler>) -> Self {
        self.handlers.push(handler);
        self
    }

    /// Process input through the chain of handlers
    ///
    /// Returns the first successful classification, or None if no handler matches
    pub fn process(&self, input: &str) -> Option<InputType> {
        for handler in &self.handlers {
            if let Some(result) = handler.handle(input) {
                return Some(result);
            }
        }
        None
    }
}

impl Default for ClassifierChain {
    fn default() -> Self {
        Self::new()
    }
}

/// Handler for empty input
#[derive(Debug)]
pub struct EmptyInputHandler;

impl EmptyInputHandler {
    pub const fn new() -> Self {
        Self
    }
}

impl InputHandler for EmptyInputHandler {
    fn handle(&self, input: &str) -> Option<InputType> {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            Some(InputType::Empty)
        } else {
            None
        }
    }
}

impl Default for EmptyInputHandler {
    fn default() -> Self {
        Self::new()
    }
}

/// Handler for application-specific builtin commands
///
/// Recognizes commands that are built into the terminal application:
/// - `clear`: Clear terminal output buffer
/// - `reload-aliases`: Reload alias definitions
/// - `reload-commands`: Clear command cache
///
/// This handler has high priority (position 3 in chain) to prevent these
/// commands from being misclassified as natural language by later handlers.
#[derive(Debug)]
pub struct ApplicationBuiltinHandler;

impl ApplicationBuiltinHandler {
    pub const fn new() -> Self {
        Self
    }
}

impl InputHandler for ApplicationBuiltinHandler {
    fn handle(&self, input: &str) -> Option<InputType> {
        let trimmed = input.trim();

        // Extract the command (first word)
        let command = trimmed.split_whitespace().next()?;

        // Check if it's an application builtin
        if crate::input::application_builtins::is_application_builtin(command) {
            // Parse args directly without intermediate Vec allocation
            let args: Vec<String> = trimmed
                .split_whitespace()
                .skip(1)
                .map(|s| s.to_string())
                .collect();

            // Only preserve original_input if shell operators present (consistent with other handlers)
            let patterns = crate::input::patterns::CompiledPatterns::get();
            let original_input = if patterns.has_shell_operators(trimmed) {
                Some(trimmed.to_string())
            } else {
                None
            };

            return Some(InputType::Command {
                command: command.to_string(),
                args,
                original_input,
            });
        }

        None
    }
}

impl Default for ApplicationBuiltinHandler {
    fn default() -> Self {
        Self::new()
    }
}

/// Handler for known commands (whitelist-based)
pub struct KnownCommandHandler {
    known_commands: Vec<String>,
}

impl std::fmt::Debug for KnownCommandHandler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KnownCommandHandler")
            .field("known_commands_count", &self.known_commands.len())
            .finish()
    }
}

impl KnownCommandHandler {
    pub const fn new(known_commands: Vec<String>) -> Self {
        Self { known_commands }
    }

    /// Create with default DevOps commands
    pub fn with_defaults() -> Self {
        Self::new(crate::input::known_commands::default_devops_commands())
    }

    /// Check if the input starts with a known command
    fn is_known_command(&self, input: &str) -> bool {
        let first_word = input.split_whitespace().next().unwrap_or("");
        self.known_commands.iter().any(|cmd| cmd == first_word)
    }

    /// Parse input as a command
    fn parse_as_command(&self, input: &str) -> Result<InputType> {
        let parts = shell_words::split(input)?;

        if parts.is_empty() {
            return Ok(InputType::Empty);
        }

        // Preserve original input if it contains shell operators
        let patterns = crate::input::patterns::CompiledPatterns::get();
        let original_input = if patterns.has_shell_operators(input) {
            Some(input.to_string())
        } else {
            None
        };

        Ok(InputType::Command {
            command: parts.first().cloned().unwrap_or_default(),
            args: parts.get(1..).unwrap_or(&[]).to_vec(),
            original_input,
        })
    }
}

impl InputHandler for KnownCommandHandler {
    fn handle(&self, input: &str) -> Option<InputType> {
        let trimmed = input.trim();

        if !self.is_known_command(trimmed) {
            return None;
        }

        // Command is in whitelist - verify it actually exists in PATH
        let first_word = trimmed.split_whitespace().next()?;

        // Use CommandCache for fast existence check
        if crate::input::discovery::CommandCache::is_available(first_word) {
            self.parse_as_command(trimmed).ok()
        } else {
            // Command in whitelist but not installed - pass to next handler
            None
        }
    }
}

/// Handler for command syntax detection (flags, pipes, paths, etc.)
#[derive(Debug)]
pub struct CommandSyntaxHandler;

impl CommandSyntaxHandler {
    pub const fn new() -> Self {
        Self
    }

    /// Check if input looks like a command based on syntax
    ///
    /// Language-agnostic algorithm:
    /// - Has flags (-/--) → Command (let the command validate)
    /// - Has shell operators (|, >, <, &&) → Command
    /// - Has paths (./, ../, /) or env vars ($VAR) → Command
    /// - Multi-word without above → Natural Language (defer to next handler)
    fn looks_like_command(&self, input: &str) -> bool {
        let patterns = crate::input::patterns::CompiledPatterns::get();

        // Shell operators (|, >, <, &&, ||, ;) → definitely a command
        if patterns.has_shell_operators(input) {
            return true;
        }

        // Command syntax (flags, paths, env vars) → definitely a command
        // Even invalid flags like --aiuto will be handled by the command itself
        if patterns.has_command_syntax(input) {
            return true;
        }

        // Multi-word without flags/operators → likely natural language
        // Single word → defer to other handlers (KnownCommand, Typo, Path, etc.)
        false
    }

    /// Parse input as a command
    fn parse_as_command(&self, input: &str) -> Result<InputType> {
        let parts = shell_words::split(input)?;

        if parts.is_empty() {
            return Ok(InputType::Empty);
        }

        // Preserve original input if it contains shell operators
        let patterns = crate::input::patterns::CompiledPatterns::get();
        let original_input = if patterns.has_shell_operators(input) {
            Some(input.to_string())
        } else {
            None
        };

        Ok(InputType::Command {
            command: parts.first().cloned().unwrap_or_default(),
            args: parts.get(1..).unwrap_or(&[]).to_vec(),
            original_input,
        })
    }
}

impl InputHandler for CommandSyntaxHandler {
    fn handle(&self, input: &str) -> Option<InputType> {
        let trimmed = input.trim();
        if self.looks_like_command(trimmed) {
            self.parse_as_command(trimmed).ok()
        } else {
            None
        }
    }
}

impl Default for CommandSyntaxHandler {
    fn default() -> Self {
        Self::new()
    }
}

/// Handler for natural language detection (multilingual support)
#[derive(Debug)]
pub struct NaturalLanguageHandler;

impl NaturalLanguageHandler {
    pub const fn new() -> Self {
        Self
    }

    /// Check if input is likely natural language using language-agnostic heuristics
    ///
    /// Uses universal patterns instead of hardcoded words to support any language:
    /// - Punctuation (?, !, commas, periods)
    /// - Non-ASCII characters (accents, unicode, non-Latin scripts)
    /// - Structural patterns (word count, spacing, capitalization)
    /// - Statistical analysis (word/symbol ratios)
    fn is_likely_natural_language(&self, input: &str) -> bool {
        // Get precompiled patterns
        let patterns = crate::input::patterns::CompiledPatterns::get();

        // 1. Universal punctuation patterns (question/exclamation marks, sentence boundaries)
        if patterns.has_natural_language_indicators(input) {
            return true;
        }

        // 2. Language-agnostic regex patterns (English-only fallback, maintained for compatibility)
        if patterns.starts_with_question_word(input) || patterns.has_articles(input) {
            return true;
        }

        // 3. Word count heuristic (universal across languages)
        let word_count = input.split_whitespace().count();
        if word_count > 5 && !patterns.has_shell_operators(input) {
            return true;
        }

        // ===== LANGUAGE-AGNOSTIC HEURISTICS =====

        // 4. Non-ASCII character detection (accents, unicode, non-Latin scripts)
        // Most natural language contains non-ASCII, shell commands are ASCII
        if !input.is_ascii() {
            // Exclude common command patterns with unicode (e.g., docker --help with smart quotes)
            let looks_like_command = patterns.has_command_syntax(input)
                || patterns.has_shell_operators(input)
                || input.starts_with('/')
                || input.starts_with("./");

            if !looks_like_command {
                return true;
            }
        }

        // 5. Repeated punctuation (e.g., "??", "!!", "...") indicates natural language
        if input.contains("??") || input.contains("!!") || input.contains("...") {
            return true;
        }

        // 6. Multiple short words (2-3 chars) with spaces - likely articles/prepositions
        // Example: "a la", "de la", "in the", "per il"
        let words: Vec<&str> = input.split_whitespace().collect();
        if words.len() >= 3 {
            let short_word_count = words
                .iter()
                .filter(|w| w.len() >= 2 && w.len() <= 3)
                .count();
            let short_word_ratio = short_word_count as f64 / words.len() as f64;

            // If >30% of words are 2-3 chars and no command syntax, likely NL
            if short_word_ratio > 0.3 && !patterns.has_command_syntax(input) {
                return true;
            }
        }

        // 7. Medium-length phrases (3-5 words) without command indicators
        // Commands are typically 1-2 words or have flags/operators
        if (3..=5).contains(&word_count)
            && !patterns.has_command_syntax(input)
            && !patterns.has_shell_operators(input)
            && !input.starts_with('/')
            && !input.starts_with("./")
        {
            // Additional check: no known command at start
            // Use CommandCache for consistency with KnownCommandHandler (DRY principle)
            let first_word = words.first().map(|w| w.to_lowercase()).unwrap_or_default();
            if !crate::input::discovery::CommandCache::is_available(&first_word) {
                return true;
            }
        }

        // 8. Contractions (e.g., "don't", "can't", "I'm") - universal NL indicator
        // Check without trailing space to catch end-of-sentence and before-punctuation cases
        let contractions = ["'t", "'re", "'ve", "'ll", "'s", "'m", "'d"];
        if contractions.iter().any(|c| input.contains(c)) {
            return true;
        }

        false
    }
}

impl InputHandler for NaturalLanguageHandler {
    fn handle(&self, input: &str) -> Option<InputType> {
        let trimmed = input.trim();
        if self.is_likely_natural_language(trimmed) {
            Some(InputType::NaturalLanguage(trimmed.to_string()))
        } else {
            None
        }
    }
}

impl Default for NaturalLanguageHandler {
    fn default() -> Self {
        Self::new()
    }
}

/// Handler for commands that exist in PATH but aren't in the known commands list
///
/// This handler catches newly installed commands that aren't in the hardcoded
/// whitelist by verifying their existence via `which`. It runs AFTER KnownCommandHandler
/// but BEFORE TypoDetectionHandler to prevent false typo suggestions for valid commands.
///
/// # Performance
/// - Uses `CommandCache::is_available()` which caches PATH lookups
/// - Cache miss: O(PATH_length) via `which` crate (~1-5ms)
/// - Cache hit: O(1) hash lookup
#[derive(Debug)]
pub struct PathDiscoveryHandler;

impl PathDiscoveryHandler {
    pub const fn new() -> Self {
        Self
    }

    /// Parse input as a command
    fn parse_as_command(&self, input: &str) -> Option<InputType> {
        let parts = shell_words::split(input).ok()?;

        if parts.is_empty() {
            return Some(InputType::Empty);
        }

        // Preserve original input if it contains shell operators
        let patterns = crate::input::patterns::CompiledPatterns::get();
        let original_input = if patterns.has_shell_operators(input) {
            Some(input.to_string())
        } else {
            None
        };

        Some(InputType::Command {
            command: parts.first().cloned().unwrap_or_default(),
            args: parts.get(1..).unwrap_or(&[]).to_vec(),
            original_input,
        })
    }
}

impl InputHandler for PathDiscoveryHandler {
    fn handle(&self, input: &str) -> Option<InputType> {
        let trimmed = input.trim();

        // Get first word (the command)
        let first_word = trimmed.split_whitespace().next()?;

        // Skip if it contains path separators (handled by PathCommandHandler)
        if first_word.contains('/') || first_word.contains('\\') {
            return None;
        }

        // Check if command exists in PATH using CommandCache
        // This uses `which` internally and caches the result
        // We check PATH first because commands like "gh auth login" are valid
        // even without flags - the subcommands (auth, login) are arguments
        if crate::input::discovery::CommandCache::is_available(first_word) {
            return self.parse_as_command(trimmed);
        }

        None
    }
}

impl Default for PathDiscoveryHandler {
    fn default() -> Self {
        Self::new()
    }
}

/// Handler for executable paths (./script.sh, /usr/bin/cmd, etc.)
///
/// Detects inputs that start with path-like prefixes and verifies
/// that the file exists and is executable.
#[derive(Debug)]
pub struct PathCommandHandler;

impl PathCommandHandler {
    pub const fn new() -> Self {
        Self
    }

    /// Check if input starts with a path-like prefix
    fn is_path(&self, input: &str) -> bool {
        let first_token = input.split_whitespace().next().unwrap_or("");

        first_token.starts_with('/')
            || first_token.starts_with("./")
            || first_token.starts_with("../")
    }

    /// Check if the path is executable
    #[cfg(unix)]
    fn is_executable(&self, path: &str) -> bool {
        use std::os::unix::fs::PermissionsExt;
        use std::path::Path;

        let path_obj = Path::new(path);
        if let Ok(metadata) = std::fs::metadata(path_obj) {
            metadata.is_file() && metadata.permissions().mode() & 0o111 != 0
        } else {
            // Path doesn't exist or can't be accessed
            // Still classify as command (will fail at execution)
            path_obj.extension().is_some()
        }
    }

    /// Check if the path looks like an executable on Windows
    #[cfg(windows)]
    fn is_executable(&self, path: &str) -> bool {
        use std::path::Path;

        let path_obj = Path::new(path);
        if let Some(ext) = path_obj.extension() {
            let ext_lower = ext.to_string_lossy().to_lowercase();
            ["exe", "bat", "cmd", "ps1", "sh"].contains(&ext_lower.as_str())
        } else {
            false
        }
    }

    /// Parse input as a command
    fn parse_as_command(&self, input: &str) -> anyhow::Result<InputType> {
        let parts = shell_words::split(input)?;

        if parts.is_empty() {
            return Ok(InputType::Empty);
        }

        // Preserve original input if it contains shell operators
        let patterns = crate::input::patterns::CompiledPatterns::get();
        let original_input = if patterns.has_shell_operators(input) {
            Some(input.to_string())
        } else {
            None
        };

        Ok(InputType::Command {
            command: parts.first().cloned().unwrap_or_default(),
            args: parts.get(1..).unwrap_or(&[]).to_vec(),
            original_input,
        })
    }
}

impl InputHandler for PathCommandHandler {
    fn handle(&self, input: &str) -> Option<InputType> {
        let trimmed = input.trim();

        if !self.is_path(trimmed) {
            return None;
        }

        // Extract the path (first token)
        let first_token = trimmed.split_whitespace().next()?;

        // Check if it's executable
        if self.is_executable(first_token) {
            self.parse_as_command(trimmed).ok()
        } else {
            // Path exists but not executable, or doesn't exist
            // Pass to next handler
            None
        }
    }
}

impl Default for PathCommandHandler {
    fn default() -> Self {
        Self::new()
    }
}

/// Default handler - treats everything as natural language
///
/// This is the final handler in the chain that catches any input
/// that wasn't classified by previous handlers.
#[derive(Debug)]
pub struct DefaultHandler;

impl DefaultHandler {
    pub const fn new() -> Self {
        Self
    }
}

impl InputHandler for DefaultHandler {
    fn handle(&self, input: &str) -> Option<InputType> {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            Some(InputType::Empty)
        } else {
            Some(InputType::NaturalLanguage(trimmed.to_string()))
        }
    }
}

impl Default for DefaultHandler {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_input_handler() {
        let handler = EmptyInputHandler::new();

        assert_eq!(handler.handle(""), Some(InputType::Empty));
        assert_eq!(handler.handle("   "), Some(InputType::Empty));
        assert_eq!(handler.handle("test"), None);
    }

    #[test]
    fn test_application_builtin_handler() {
        let handler = ApplicationBuiltinHandler::new();

        // Test clear command
        assert!(matches!(
            handler.handle("clear"),
            Some(InputType::Command {
                command,
                args,
                ..
            }) if command == "clear" && args.is_empty()
        ));

        // Test reload-aliases command
        assert!(matches!(
            handler.handle("reload-aliases"),
            Some(InputType::Command {
                command,
                args,
                ..
            }) if command == "reload-aliases" && args.is_empty()
        ));

        // Test reload-commands command
        assert!(matches!(
            handler.handle("reload-commands"),
            Some(InputType::Command {
                command,
                args,
                ..
            }) if command == "reload-commands" && args.is_empty()
        ));

        // Test application builtin with arguments (should still work)
        assert!(matches!(
            handler.handle("clear --extra-arg"),
            Some(InputType::Command {
                command,
                args,
                ..
            }) if command == "clear" && args == vec!["--extra-arg"]
        ));

        // Test non-builtin commands should pass through
        assert_eq!(handler.handle("docker ps"), None);
        assert_eq!(handler.handle("ls -la"), None);
        assert_eq!(handler.handle("how do I clear the screen"), None);
    }

    #[test]
    fn test_application_builtin_original_input() {
        let handler = ApplicationBuiltinHandler::new();

        // Test without shell operators → original_input should be None
        match handler.handle("clear") {
            Some(InputType::Command {
                command,
                original_input,
                ..
            }) => {
                assert_eq!(command, "clear");
                assert_eq!(
                    original_input, None,
                    "original_input should be None without shell operators"
                );
            }
            _ => panic!("Expected Command type"),
        }

        // Test with pipe operator → original_input should be Some
        match handler.handle("clear | grep foo") {
            Some(InputType::Command {
                command,
                original_input,
                ..
            }) => {
                assert_eq!(command, "clear");
                assert!(
                    original_input.is_some(),
                    "original_input should be Some with shell operators"
                );
                assert_eq!(original_input.unwrap(), "clear | grep foo");
            }
            _ => panic!("Expected Command type"),
        }

        // Test with redirect → original_input should be Some
        match handler.handle("reload-aliases > output.txt") {
            Some(InputType::Command { original_input, .. }) => {
                assert!(original_input.is_some());
            }
            _ => panic!("Expected Command type"),
        }
    }

    #[test]
    fn test_known_command_handler() {
        let handler = KnownCommandHandler::with_defaults();

        // Test behavior based on what's actually installed
        // The handler correctly returns None if command is in whitelist but not in PATH

        // Test 1: Unknown commands should always pass through
        assert_eq!(handler.handle("unknown-command-xyz-123"), None);
        assert_eq!(handler.handle("how do I list files"), None);

        // Test 2: Test with a command that should be universally available
        // If 'ls' exists in PATH (common on Unix), it should be recognized
        if crate::input::discovery::CommandCache::is_available("ls") {
            assert!(matches!(
                handler.handle("ls -la"),
                Some(InputType::Command { .. })
            ));
        }

        // Test 3: Command in whitelist but not installed should return None
        // This is the CORRECT behavior - handler passes to next handler
        if crate::input::discovery::CommandCache::is_available("docker") {
            // If docker IS installed, handler should recognize it
            assert!(matches!(
                handler.handle("docker ps"),
                Some(InputType::Command { .. })
            ));
        } else {
            // If docker not installed, handler should return None (correct)
            assert_eq!(handler.handle("docker ps"), None);
        }
    }

    #[test]
    fn test_command_syntax_handler() {
        let handler = CommandSyntaxHandler::new();

        // Commands with flags
        assert!(matches!(
            handler.handle("unknown-cmd --flag"),
            Some(InputType::Command { .. })
        ));

        // Commands with pipes
        assert!(matches!(
            handler.handle("cat file.txt | grep pattern"),
            Some(InputType::Command { .. })
        ));

        // Paths
        assert!(matches!(
            handler.handle("./deploy.sh"),
            Some(InputType::Command { .. })
        ));

        // Natural language should pass through
        assert_eq!(handler.handle("how do I list files"), None);
    }

    #[test]
    fn test_natural_language_handler() {
        let handler = NaturalLanguageHandler::new();

        // Questions
        assert!(matches!(
            handler.handle("how do I list files?"),
            Some(InputType::NaturalLanguage(_))
        ));

        // Long phrases
        assert!(matches!(
            handler.handle("show me the docker containers"),
            Some(InputType::NaturalLanguage(_))
        ));

        // Polite expressions
        assert!(matches!(
            handler.handle("please help me"),
            Some(InputType::NaturalLanguage(_))
        ));

        // Commands should pass through
        assert_eq!(handler.handle("ls"), None);
    }

    #[test]
    fn test_natural_language_contractions_edge_cases() {
        let handler = NaturalLanguageHandler::new();

        // Test contractions at end of sentence (no trailing space)
        assert!(
            matches!(handler.handle("don't"), Some(InputType::NaturalLanguage(_))),
            "Should detect contraction 'don't' at end of input"
        );

        assert!(
            matches!(
                handler.handle("I can't"),
                Some(InputType::NaturalLanguage(_))
            ),
            "Should detect contraction 'can't' at end of input"
        );

        assert!(
            matches!(handler.handle("I'm"), Some(InputType::NaturalLanguage(_))),
            "Should detect contraction 'I'm' at end of input"
        );

        // Test contractions before punctuation
        assert!(
            matches!(
                handler.handle("can't."),
                Some(InputType::NaturalLanguage(_))
            ),
            "Should detect contraction before period"
        );

        assert!(
            matches!(
                handler.handle("don't!"),
                Some(InputType::NaturalLanguage(_))
            ),
            "Should detect contraction before exclamation"
        );

        assert!(
            matches!(
                handler.handle("won't?"),
                Some(InputType::NaturalLanguage(_))
            ),
            "Should detect contraction before question mark"
        );

        // Test contractions in middle of sentence (original behavior still works)
        assert!(
            matches!(
                handler.handle("don't know"),
                Some(InputType::NaturalLanguage(_))
            ),
            "Should detect contraction in middle of sentence"
        );

        assert!(
            matches!(
                handler.handle("you're right"),
                Some(InputType::NaturalLanguage(_))
            ),
            "Should detect contraction 'you're' in middle"
        );

        // Test multiple contractions
        assert!(
            matches!(
                handler.handle("I don't think you're right"),
                Some(InputType::NaturalLanguage(_))
            ),
            "Should detect multiple contractions"
        );
    }

    #[test]
    fn test_natural_language_medium_phrases() {
        let handler = NaturalLanguageHandler::new();

        // Test 3-5 word phrase with no known command at start → should be NL
        // "show" is not a known system command
        assert!(
            matches!(
                handler.handle("show container status now"),
                Some(InputType::NaturalLanguage(_))
            ),
            "Should detect 4-word phrase with unknown command as NL"
        );

        // Test 3-word phrase with unknown verb
        assert!(
            matches!(
                handler.handle("explain this thing"),
                Some(InputType::NaturalLanguage(_))
            ),
            "Should detect 3-word phrase with unknown command as NL"
        );

        // Note: NaturalLanguageHandler runs late in chain (after KnownCommandHandler)
        // So commands like "docker ps" are already classified before reaching this handler
        // This handler focuses on phrases that look like NL, not command filtering

        // Test edge case: less than 3 words should not trigger medium-phrase heuristic
        // (though it might be caught by other NL indicators)
        // Use a phrase that's clearly 2 words and not in regex patterns
        let result = handler.handle("check now");
        // "check now" is 2 words, no clear NL indicators, should pass through
        assert_eq!(
            result, None,
            "2-word phrase without NL indicators should not trigger medium-phrase heuristic"
        );

        // Test edge case: more than 5 words falls back to word count > 5 heuristic
        assert!(
            matches!(
                handler.handle("please show me all the logs now"),
                Some(InputType::NaturalLanguage(_))
            ),
            "6+ word phrase should be caught by word count heuristic"
        );
    }

    #[test]
    fn test_classifier_chain() {
        let chain = ClassifierChain::new()
            .add_handler(Box::new(EmptyInputHandler::new()))
            .add_handler(Box::new(KnownCommandHandler::with_defaults()))
            .add_handler(Box::new(CommandSyntaxHandler::new()))
            .add_handler(Box::new(NaturalLanguageHandler::new()))
            .add_handler(Box::new(DefaultHandler::new()));

        // Empty input
        assert_eq!(chain.process(""), Some(InputType::Empty));

        // Known command
        assert!(matches!(
            chain.process("ls -la"),
            Some(InputType::Command { .. })
        ));

        // Command syntax
        assert!(matches!(
            chain.process("unknown --flag"),
            Some(InputType::Command { .. })
        ));

        // Natural language
        assert!(matches!(
            chain.process("how do I list files?"),
            Some(InputType::NaturalLanguage(_))
        ));

        // Default (ambiguous input)
        assert!(matches!(
            chain.process("something ambiguous"),
            Some(InputType::NaturalLanguage(_))
        ));
    }

    #[test]
    fn test_chain_order_matters() {
        // Different order - empty handler should be first
        let chain = ClassifierChain::new()
            .add_handler(Box::new(EmptyInputHandler::new()))
            .add_handler(Box::new(KnownCommandHandler::with_defaults()));

        assert_eq!(chain.process(""), Some(InputType::Empty));

        // Without empty handler first, default would catch it
        let chain2 = ClassifierChain::new().add_handler(Box::new(DefaultHandler::new()));

        assert!(matches!(chain2.process(""), Some(InputType::Empty)));
    }

    #[test]
    fn test_path_command_handler() {
        let handler = PathCommandHandler::new();

        // Relative paths should be detected
        assert!(handler.is_path("./script.sh"));
        assert!(handler.is_path("../deploy.sh --flag"));
        assert!(handler.is_path("./script.sh arg1 arg2"));

        // Absolute paths should be detected
        assert!(handler.is_path("/usr/bin/cmd"));
        assert!(handler.is_path("/bin/sh -c 'echo test'"));

        // Non-paths should not be detected
        assert!(!handler.is_path("docker ps"));
        assert!(!handler.is_path("ls -la"));
        assert!(!handler.is_path("how do I run a script"));
    }

    #[test]
    #[cfg(unix)]
    fn test_path_executable_check_unix() {
        let handler = PathCommandHandler::new();

        // Common executables that should exist on Unix systems
        assert!(handler.is_executable("/bin/sh") || handler.is_executable("/bin/bash"));

        // Non-existent file with extension (should still return true)
        assert!(handler.is_executable("./nonexistent.sh"));

        // Non-executable path
        assert!(!handler.is_executable("/etc/passwd"));
    }

    #[test]
    #[cfg(windows)]
    fn test_path_executable_check_windows() {
        let handler = PathCommandHandler::new();

        // Windows executables by extension
        assert!(handler.is_executable("./script.bat"));
        assert!(handler.is_executable("./program.exe"));
        assert!(handler.is_executable("./script.ps1"));
        assert!(handler.is_executable("./deploy.cmd"));

        // Non-executable extension
        assert!(!handler.is_executable("./readme.txt"));
        assert!(!handler.is_executable("./data.json"));
    }
}
