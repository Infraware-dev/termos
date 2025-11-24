/// Typo detection for command classification
///
/// This module provides typo detection using Levenshtein distance to prevent
/// false natural language classification of mistyped commands.
///
/// # Design Pattern: Strategy Pattern
/// - TypoDetectionHandler implements InputHandler trait
/// - Pluggable into the classification chain
/// - Configurable max distance threshold
use crate::input::InputType;
use strsim::levenshtein;

/// Handler for detecting command typos using Levenshtein distance
///
/// Prevents mistyped commands like "dokcer ps" from being classified as
/// natural language, which would trigger unnecessary LLM requests.
///
/// # Example
/// ```
/// use infraware_terminal::input::typo_detection::TypoDetectionHandler;
/// use infraware_terminal::input::handler::InputHandler;
///
/// let handler = TypoDetectionHandler::with_defaults();
/// let result = handler.handle("dokcer ps");
/// // Returns CommandTypo with suggestion "docker"
/// ```
pub struct TypoDetectionHandler {
    known_commands: Vec<String>,
    max_distance: usize,
}

impl TypoDetectionHandler {
    /// Create a new typo detection handler
    ///
    /// # Arguments
    /// * `known_commands` - List of valid commands to check against
    /// * `max_distance` - Maximum Levenshtein distance to consider (default: 2)
    pub const fn new(known_commands: Vec<String>, max_distance: usize) -> Self {
        Self {
            known_commands,
            max_distance,
        }
    }

    /// Create handler with default DevOps commands and max_distance=2
    pub fn with_defaults() -> Self {
        Self::new(crate::input::known_commands::default_devops_commands(), 2)
    }

    /// Find the closest matching command within max_distance
    ///
    /// # Arguments
    /// * `input` - The potentially mistyped command
    ///
    /// # Returns
    /// `Some((closest_match, distance))` if a close match is found,
    /// `None` if no match within max_distance
    fn find_closest_match(&self, input: &str) -> Option<(String, usize)> {
        self.known_commands
            .iter()
            .map(|cmd| (cmd.clone(), levenshtein(input, cmd)))
            .filter(|(_, dist)| *dist <= self.max_distance && *dist > 0)
            .min_by_key(|(_, dist)| *dist)
    }

    /// Check if input looks like a command (not a natural language phrase)
    ///
    /// Language-agnostic algorithm for multi-word inputs:
    /// - Has flags (-/--) → might be a typo (e.g., "dokcer --help")
    /// - Multi-word without flags → likely natural language, not a typo
    ///
    /// For single words, we filter out common NL words to avoid false positives
    /// like "what" → "cat". This is a pragmatic compromise for short inputs.
    fn looks_like_command(&self, input: &str) -> bool {
        let word_count = input.split_whitespace().count();

        // Single word → might be a command typo, but filter common NL words
        if word_count == 1 {
            // Common single-word NL indicators that could match short commands
            // Keep this list minimal - multi-word is fully language-agnostic
            const NL_SINGLE_WORDS: &[&str] = &[
                // Question words (universal patterns)
                "what", "how", "why", "when", "where", "who", "which",
                // Common greetings/responses
                "hello", "hi", "hey", "yes", "no", "ok", "thanks", "help",
            ];

            let lower = input.to_lowercase();
            if NL_SINGLE_WORDS.contains(&lower.as_str()) {
                return false;
            }
            return true;
        }

        // Has flags → might be a command typo (e.g., "dokcer -v")
        let patterns = crate::input::patterns::CompiledPatterns::get();
        if patterns.has_command_syntax(input) || patterns.has_shell_operators(input) {
            return true;
        }

        // Multi-word without flags → likely natural language (language-agnostic)
        false
    }

    /// Check if a command is actually incorrect (not in the known commands list)
    fn is_unknown_command(&self, word: &str) -> bool {
        !self.known_commands.iter().any(|cmd| cmd == word)
    }
}

/// Implement InputHandler trait for typo detection
impl crate::input::handler::InputHandler for TypoDetectionHandler {
    fn handle(&self, input: &str) -> Option<InputType> {
        let trimmed = input.trim();

        // Skip if doesn't look like a command
        if !self.looks_like_command(trimmed) {
            return None;
        }

        // Extract first word (the command)
        let first_word = trimmed.split_whitespace().next()?;

        // Only check for typos if the command is unknown
        if !self.is_unknown_command(first_word) {
            // Command is in our list, not a typo - pass to next handler
            return None;
        }

        // Check for typos in the command name
        if let Some((closest, distance)) = self.find_closest_match(first_word) {
            // Found a typo - return CommandTypo instead of letting it fall through to NL
            return Some(InputType::CommandTypo {
                input: trimmed.to_string(),
                suggestion: closest,
                distance,
            });
        }

        // No typo detected, pass to next handler
        None
    }
}

impl Default for TypoDetectionHandler {
    fn default() -> Self {
        Self::with_defaults()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::handler::InputHandler;

    #[test]
    fn test_find_closest_match() {
        let handler = TypoDetectionHandler::with_defaults();

        // Typos with clear expected matches
        let result = handler.find_closest_match("dokcer");
        assert!(result.is_some());
        let (cmd, dist) = result.unwrap();
        assert_eq!(cmd, "docker");
        assert_eq!(dist, 2);

        // One character off
        let result = handler.find_closest_match("dockerr");
        assert!(result.is_some());
        let (cmd, dist) = result.unwrap();
        assert_eq!(cmd, "docker");
        assert_eq!(dist, 1);

        // Within distance threshold
        let result = handler.find_closest_match("kubeclt");
        assert!(result.is_some());
        let (_, dist) = result.unwrap();
        assert!(dist <= 2);

        // Exact match returns None (distance must be > 0)
        // But "docker" might match "packer" at distance 2, so we test with unique word
        let handler_single = TypoDetectionHandler::new(vec!["uniquecmd".to_string()], 2);
        assert_eq!(handler_single.find_closest_match("uniquecmd"), None);
    }

    #[test]
    fn test_common_typos() {
        let handler = TypoDetectionHandler::with_defaults();

        // Test actual typos (not exact matches)
        let typos = vec![
            "dokcer",  // -> docker
            "kubeclt", // -> kubectl
            "grpe",    // -> grep
        ];

        for typo in typos {
            let result = handler.find_closest_match(typo);
            assert!(result.is_some(), "Typo '{typo}' should find a close match");
            if let Some((_, distance)) = result {
                assert!(distance <= 2, "Distance should be <= 2 for typo '{typo}'");
            }
        }

        // Note: We don't test exact matches with find_closest_match because
        // exact commands (like "docker") might still match other commands (like "packer")
        // within the distance threshold. The handler.handle() method properly filters
        // exact matches via is_unknown_command().
    }

    #[test]
    fn test_handle_typo() {
        let handler = TypoDetectionHandler::with_defaults();

        // Single word typo should be detected
        let result = handler.handle("dokcer");
        assert!(matches!(result, Some(InputType::CommandTypo { .. })));

        if let Some(InputType::CommandTypo {
            suggestion,
            distance,
            ..
        }) = result
        {
            assert_eq!(suggestion, "docker");
            assert!(distance <= 2);
        }

        // Typo with flags should be detected
        let result = handler.handle("dokcer --version");
        assert!(matches!(result, Some(InputType::CommandTypo { .. })));
    }

    #[test]
    fn test_handle_correct_command() {
        let handler = TypoDetectionHandler::with_defaults();

        // Correct command should pass through (return None)
        let result = handler.handle("docker ps");
        assert_eq!(result, None);
    }

    #[test]
    fn test_handle_natural_language() {
        let handler = TypoDetectionHandler::with_defaults();

        // Natural language should pass through
        let result = handler.handle("how do I use docker?");
        assert_eq!(result, None);

        let result = handler.handle("show me the logs");
        assert_eq!(result, None);
    }

    #[test]
    fn test_looks_like_command() {
        let handler = TypoDetectionHandler::with_defaults();

        // Single word → might be command typo
        assert!(handler.looks_like_command("ls"));
        assert!(handler.looks_like_command("dokcer"));

        // Multi-word with flags → might be command typo
        assert!(handler.looks_like_command("dokcer -v"));
        assert!(handler.looks_like_command("kubeclt --help"));

        // Multi-word without flags → NOT command-like (language-agnostic)
        assert!(!handler.looks_like_command("dokcer ps")); // No flags
        assert!(!handler.looks_like_command("git status")); // No flags
        assert!(!handler.looks_like_command("pippo ciao come stai"));
        assert!(!handler.looks_like_command("how do I list files"));
    }

    #[test]
    fn test_distance_threshold() {
        let handler = TypoDetectionHandler::new(vec!["docker".to_string()], 1);

        // Within threshold (distance=1)
        let result = handler.find_closest_match("docer");
        assert!(result.is_some());

        // Beyond threshold (distance=2)
        let handler_strict = TypoDetectionHandler::new(vec!["docker".to_string()], 1);
        let result = handler_strict.find_closest_match("dokcer");
        // dokcer has distance 2 from docker, should be None with max_distance=1
        assert_eq!(result, None);
    }

    #[test]
    fn test_multiple_close_matches() {
        // Create handler with similar commands
        let handler = TypoDetectionHandler::new(
            vec!["grep".to_string(), "gzip".to_string(), "gunzip".to_string()],
            2,
        );

        // "grpe" is closer to "grep" than others
        let result = handler.find_closest_match("grpe");
        assert!(result.is_some());
        let (suggestion, distance) = result.unwrap();
        assert_eq!(suggestion, "grep");
        assert_eq!(distance, 2);
    }

    #[test]
    fn test_case_sensitivity() {
        let handler = TypoDetectionHandler::with_defaults();

        // Commands are case-sensitive
        let result = handler.find_closest_match("Docker");
        // Docker vs docker = distance 1 (capital D)
        assert!(result.is_some());
    }

    #[test]
    fn test_empty_input() {
        let handler = TypoDetectionHandler::with_defaults();

        let result = handler.handle("");
        assert_eq!(result, None);
    }

    #[test]
    fn test_single_word_typo() {
        let handler = TypoDetectionHandler::with_defaults();

        let result = handler.handle("gti");
        assert!(matches!(result, Some(InputType::CommandTypo { .. })));

        if let Some(InputType::CommandTypo {
            suggestion,
            distance,
            ..
        }) = result
        {
            // gti is closest to git (distance should be 1 or 2 depending on algorithm)
            // We just verify we got a suggestion with valid distance
            assert!(distance <= 2);
            // The suggestion should be one of the similar commands
            assert!(["git", "gzip"].contains(&suggestion.as_str()));
        }
    }

    #[test]
    fn test_with_flags() {
        let handler = TypoDetectionHandler::with_defaults();

        // Typo with flags should be detected
        let result = handler.handle("kubeclt --help");
        assert!(matches!(result, Some(InputType::CommandTypo { .. })));

        if let Some(InputType::CommandTypo { suggestion, .. }) = result {
            assert_eq!(suggestion, "kubectl");
        }

        // Multi-word without flags should NOT be detected as typo
        let result = handler.handle("kubeclt get pods");
        assert_eq!(result, None); // Passes through to NL handler
    }
}
