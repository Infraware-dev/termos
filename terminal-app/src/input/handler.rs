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

    /// Get the name of this handler (for debugging/logging)
    #[allow(dead_code)]
    fn name(&self) -> &str;
}

/// Chain of handlers for input classification
pub struct ClassifierChain {
    handlers: Vec<Box<dyn InputHandler>>,
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

    /// Get the number of handlers in the chain
    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.handlers.len()
    }

    /// Check if the chain is empty
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.handlers.is_empty()
    }
}

impl Default for ClassifierChain {
    fn default() -> Self {
        Self::new()
    }
}

/// Handler for empty input
pub struct EmptyInputHandler;

impl EmptyInputHandler {
    pub fn new() -> Self {
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

    fn name(&self) -> &str {
        "EmptyInputHandler"
    }
}

impl Default for EmptyInputHandler {
    fn default() -> Self {
        Self::new()
    }
}

/// Handler for known commands (whitelist-based)
pub struct KnownCommandHandler {
    known_commands: Vec<String>,
}

impl KnownCommandHandler {
    pub fn new(known_commands: Vec<String>) -> Self {
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
            command: parts[0].clone(),
            args: parts[1..].to_vec(),
            original_input,
        })
    }

    /// Add a command to the known commands list
    #[allow(dead_code)]
    pub fn add_command(&mut self, command: String) {
        if !self.known_commands.contains(&command) {
            self.known_commands.push(command);
        }
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

    fn name(&self) -> &str {
        "KnownCommandHandler"
    }
}

/// Handler for command syntax detection (flags, pipes, paths, etc.)
pub struct CommandSyntaxHandler;

impl CommandSyntaxHandler {
    pub fn new() -> Self {
        Self
    }

    /// Check if input looks like a command based on syntax
    fn looks_like_command(&self, input: &str) -> bool {
        // Contains flags
        if input.contains(" -") || input.contains(" --") {
            return true;
        }

        // Contains pipes or redirects
        if input.contains('|') || input.contains('>') || input.contains('<') {
            return true;
        }

        // Environment variable syntax
        if input.contains("$") || input.contains("${") {
            return true;
        }

        // Looks like a path
        if input.starts_with('/') || input.starts_with("./") || input.starts_with("../") {
            return true;
        }

        // Single word without spaces (might be a command)
        if !input.contains(' ') && input.len() < 20 {
            return true;
        }

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
            command: parts[0].clone(),
            args: parts[1..].to_vec(),
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

    fn name(&self) -> &str {
        "CommandSyntaxHandler"
    }
}

impl Default for CommandSyntaxHandler {
    fn default() -> Self {
        Self::new()
    }
}

/// Handler for natural language detection (multilingual support)
pub struct NaturalLanguageHandler;

impl NaturalLanguageHandler {
    pub fn new() -> Self {
        Self
    }

    /// Check if input is likely natural language (multilingual support)
    /// Supports: English, Italian, Spanish, French, German
    ///
    /// Uses precompiled regex patterns for 10-100x faster matching
    fn is_likely_natural_language(&self, input: &str) -> bool {
        // Get precompiled patterns
        let patterns = crate::input::patterns::CompiledPatterns::get();

        // Fast regex-based detection
        if patterns.has_natural_language_indicators(input) {
            return true;
        }

        // Check for question words (any language)
        if patterns.starts_with_question_word(input) {
            return true;
        }

        // Check for articles (indicates natural language structure)
        if patterns.has_articles(input) {
            return true;
        }

        // Long input without command syntax (universal heuristic)
        let word_count = input.split_whitespace().count();
        if word_count > 5 && !patterns.has_shell_operators(input) {
            return true;
        }

        // Legacy fallback checks (kept for edge cases not covered by regex)
        let lowercase = input.to_lowercase();

        // ===== MULTILINGUAL PATTERNS =====

        // 4. Question words at the start (EN, IT, ES, FR, DE)
        let question_words = [
            // English
            "how",
            "what",
            "why",
            "when",
            "where",
            "who",
            "which",
            "can you",
            "could you",
            "would you",
            "will you",
            // Italian
            "come",
            "cosa",
            "perché",
            "perche",
            "quando",
            "dove",
            "chi",
            "quale",
            "puoi",
            "potresti",
            "vorresti",
            // Spanish
            "cómo",
            "como",
            "qué",
            "que",
            "por qué",
            "porque",
            "cuando",
            "dónde",
            "donde",
            "quién",
            "quien",
            "cuál",
            "cual",
            "puedes",
            "podrías",
            "podrías",
            // French
            "comment",
            "quoi",
            "pourquoi",
            "quand",
            "où",
            "ou",
            "qui",
            "quel",
            "quelle",
            "peux-tu",
            "pourrais-tu",
            "peux tu",
            "pourrais tu",
            // German
            "wie",
            "was",
            "warum",
            "wann",
            "wo",
            "wer",
            "welche",
            "welcher",
            "kannst du",
            "könntest du",
            "kannst",
            "könntest",
        ];

        for word in &question_words {
            if lowercase.starts_with(word) {
                return true;
            }
        }

        // 5. Common articles (indicates natural language structure)
        let articles = [
            // English
            " a ", " an ", " the ", // Italian
            " un ", " uno ", " una ", " il ", " lo ", " la ", " i ", " gli ", " le ", " dell",
            " dell'", " della ", " dello ", // Spanish
            " un ", " una ", " el ", " la ", " los ", " las ", " del ", " de la ", " de los ",
            // French
            " un ", " une ", " le ", " la ", " les ", " des ", " du ", " de la ", " de l'",
            // German
            " der ", " die ", " das ", " den ", " dem ", " des ", " ein ", " eine ", " einen ",
            " einem ",
        ];

        for article in &articles {
            if lowercase.contains(article) {
                return true;
            }
        }

        // 6. Common request verbs/phrases (EN, IT, ES, FR, DE)
        let nl_verbs = [
            // English
            "show me",
            "explain",
            "help",
            "tell me",
            "describe",
            "find",
            "list",
            "get me",
            "i need",
            "i want",
            "please",
            // Italian
            "mostrami",
            "spiega",
            "spiegami",
            "aiuto",
            "aiutami",
            "dimmi",
            "descrivi",
            "trova",
            "elenca",
            "ho bisogno",
            "voglio",
            "per favore",
            // Spanish
            "muéstrame",
            "muestrame",
            "explica",
            "ayuda",
            "ayúdame",
            "ayudame",
            "dime",
            "describe",
            "encuentra",
            "lista",
            "necesito",
            "quiero",
            "por favor",
            // French
            "montre-moi",
            "montre moi",
            "explique",
            "aide",
            "aide-moi",
            "dis-moi",
            "dis moi",
            "décris",
            "decris",
            "trouve",
            "liste",
            "j'ai besoin",
            "je veux",
            "s'il te plaît",
            "s'il vous plaît",
            // German
            "zeig mir",
            "zeige mir",
            "erkläre",
            "erklare",
            "hilfe",
            "hilf mir",
            "sag mir",
            "sage mir",
            "beschreibe",
            "finde",
            "liste",
            "ich brauche",
            "ich will",
            "bitte",
        ];

        for verb in &nl_verbs {
            if lowercase.contains(verb) {
                return true;
            }
        }

        // 7. Polite expressions (strong indicator of natural language)
        let polite_expressions = [
            "please",
            "per favore",
            "per piacere",
            "por favor",
            "s'il te plaît",
            "s'il vous plaît",
            "bitte",
            "grazie",
            "thank",
            "merci",
            "danke",
            "gracias",
        ];

        for expr in &polite_expressions {
            if lowercase.contains(expr) {
                return true;
            }
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

    fn name(&self) -> &str {
        "NaturalLanguageHandler"
    }
}

impl Default for NaturalLanguageHandler {
    fn default() -> Self {
        Self::new()
    }
}

/// Handler for executable paths (./script.sh, /usr/bin/cmd, etc.)
///
/// Detects inputs that start with path-like prefixes and verifies
/// that the file exists and is executable.
pub struct PathCommandHandler;

impl PathCommandHandler {
    pub fn new() -> Self {
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
            command: parts[0].clone(),
            args: parts[1..].to_vec(),
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

    fn name(&self) -> &str {
        "PathCommandHandler"
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
pub struct DefaultHandler;

impl DefaultHandler {
    pub fn new() -> Self {
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

    fn name(&self) -> &str {
        "DefaultHandler"
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
    fn test_known_command_handler() {
        let handler = KnownCommandHandler::with_defaults();

        // Known commands should be handled
        assert!(matches!(
            handler.handle("ls -la"),
            Some(InputType::Command { .. })
        ));
        assert!(matches!(
            handler.handle("docker ps"),
            Some(InputType::Command { .. })
        ));

        // Unknown commands should pass through
        assert_eq!(handler.handle("unknown-command"), None);
        assert_eq!(handler.handle("how do I list files"), None);
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
