/// Precompiled regex patterns for SCAN algorithm performance optimization
///
/// This module uses `once_cell::Lazy` to compile regex patterns once at startup,
/// providing 10-100x faster pattern matching compared to repeated compilation.
///
/// # Design Pattern: Lazy Singleton
/// - Patterns are compiled once and cached globally
/// - Thread-safe access via `once_cell::Lazy`
/// - Zero-cost abstraction (no runtime overhead after initialization)
use once_cell::sync::Lazy;
use regex::RegexSet;

/// Precompiled regex patterns for command and natural language detection
pub struct CompiledPatterns {
    /// Patterns that indicate command syntax (flags, pipes, redirects, env vars)
    #[allow(dead_code)]
    pub command_syntax: RegexSet,

    /// Patterns that indicate natural language (question words, articles, punctuation)
    pub natural_language: RegexSet,

    /// Shell operators (pipes, redirects, subshells, etc.)
    pub shell_operators: RegexSet,

    /// English question word patterns (multilingual queries handled by LLM)
    pub question_words: RegexSet,

    /// English article patterns for natural language detection
    pub articles: RegexSet,
}

/// Global instance of precompiled patterns
///
/// Initialized lazily on first access, then reused for all subsequent pattern matches.
/// This provides significant performance gains over compiling regex on every classification.
static PATTERNS: Lazy<CompiledPatterns> = Lazy::new(|| {
    CompiledPatterns {
        // Command syntax patterns
        command_syntax: RegexSet::new([
            r"^[a-zA-Z0-9_-]+(\s+--?[a-zA-Z0-9])", // Flags: --flag, -f
            r"(^|\s)(\./|\.\./)[\w.-]+",           // Relative paths: ./, ../
            r"^/[\w/.-]+",                         // Absolute paths: /usr/bin
            r"\$\{?[A-Z_][A-Z0-9_]*\}?",           // Env vars: $VAR, ${VAR}
        ])
        .expect("Failed to compile command_syntax patterns"),

        // Natural language indicators
        natural_language: RegexSet::new([
            r"[\?¿]",                                         // Question marks (universal)
            r"!",                                             // Exclamation marks
            r"\.\s+[A-Z]",                                    // Sentence boundaries
            r",\s+\w+",                                       // Commas with continuation
            r"(?i)(^|\s)(please|help|show me|explain)(\s|$)", // English polite/request words
        ])
        .expect("Failed to compile natural_language patterns"),

        // Shell operators (pipes, redirects, subshells)
        shell_operators: RegexSet::new([
            r"\|",      // Pipe
            r"&&|\|\|", // Logical operators
            r"[<>]",    // Redirects
            r"[;&]",    // Command separators
            r"\$\(",    // Subshell start
            r"`[^`]+`", // Backtick command substitution
            r"\{|\}",   // Braces
            r"\(|\)",   // Parentheses
        ])
        .expect("Failed to compile shell_operators patterns"),

        // English question words only (multilingual handled by LLM)
        question_words: RegexSet::new([
            r"(?i)^(how|what|why|when|where|who|which)\s",
            r"(?i)^(can you|could you|would you|will you)\s",
            r"(?i)^(please|help|show me|explain)\s",
        ])
        .expect("Failed to compile question_words patterns"),

        // English articles only (indicates natural language structure)
        articles: RegexSet::new([r"\s(a|an|the)\s", r"^(a|an|the)\s"])
            .expect("Failed to compile articles patterns"),
    }
});

impl CompiledPatterns {
    /// Get the global instance of precompiled patterns
    ///
    /// # Example
    /// ```
    /// use infraware_terminal::input::patterns::CompiledPatterns;
    ///
    /// let patterns = CompiledPatterns::get();
    /// assert!(patterns.question_words.is_match("how do I list files?"));
    /// ```
    pub fn get() -> &'static CompiledPatterns {
        &PATTERNS
    }

    /// Check if input matches any command syntax pattern
    #[inline]
    pub fn has_command_syntax(&self, input: &str) -> bool {
        self.command_syntax.is_match(input)
    }

    /// Check if input matches any natural language pattern
    #[inline]
    pub fn has_natural_language_indicators(&self, input: &str) -> bool {
        self.natural_language.is_match(input)
    }

    /// Check if input contains shell operators
    #[inline]
    pub fn has_shell_operators(&self, input: &str) -> bool {
        self.shell_operators.is_match(input)
    }

    /// Check if input starts with a question word (any supported language)
    #[inline]
    pub fn starts_with_question_word(&self, input: &str) -> bool {
        self.question_words.is_match(input)
    }

    /// Check if input contains articles (indicates natural language)
    #[inline]
    pub fn has_articles(&self, input: &str) -> bool {
        self.articles.is_match(input)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_command_syntax_patterns() {
        let patterns = CompiledPatterns::get();

        // Flags
        assert!(patterns.has_command_syntax("ls --all"));
        assert!(patterns.has_command_syntax("docker -v"));

        // Paths
        assert!(patterns.has_command_syntax("./script.sh"));
        assert!(patterns.has_command_syntax("../deploy.sh"));
        assert!(patterns.has_command_syntax("/usr/bin/cmd"));

        // Environment variables
        assert!(patterns.has_command_syntax("echo $HOME"));
        assert!(patterns.has_command_syntax("echo ${USER}"));

        // Not command syntax
        assert!(!patterns.has_command_syntax("how do I list files"));
        assert!(!patterns.has_command_syntax("show me the logs"));
    }

    #[test]
    fn test_natural_language_patterns() {
        let patterns = CompiledPatterns::get();

        // Question marks
        assert!(patterns.has_natural_language_indicators("what is this?"));
        assert!(patterns.has_natural_language_indicators("how do I run this?"));

        // Exclamation marks
        assert!(patterns.has_natural_language_indicators("help me please!"));
        assert!(patterns.has_natural_language_indicators("show me the logs!"));

        // Not natural language
        assert!(!patterns.has_natural_language_indicators("docker ps"));
        assert!(!patterns.has_natural_language_indicators("ls -la"));
    }

    #[test]
    fn test_shell_operators() {
        let patterns = CompiledPatterns::get();

        // Pipes
        assert!(patterns.has_shell_operators("cat file | grep pattern"));

        // Redirects
        assert!(patterns.has_shell_operators("echo hello > file.txt"));
        assert!(patterns.has_shell_operators("cat < input.txt"));

        // Logical operators
        assert!(patterns.has_shell_operators("cmd1 && cmd2"));
        assert!(patterns.has_shell_operators("cmd1 || cmd2"));

        // Subshells
        assert!(patterns.has_shell_operators("echo $(date)"));
        assert!(patterns.has_shell_operators("echo `date`"));

        // Not shell operators
        assert!(!patterns.has_shell_operators("how do I use pipes?"));
    }

    #[test]
    fn test_question_words_english() {
        let patterns = CompiledPatterns::get();

        // English question words
        assert!(patterns.starts_with_question_word("how do I list files?"));
        assert!(patterns.starts_with_question_word("what is docker"));
        assert!(patterns.starts_with_question_word("why does this fail"));
        assert!(patterns.starts_with_question_word("when should I use this"));
        assert!(patterns.starts_with_question_word("where are the logs"));
        assert!(patterns.starts_with_question_word("who can access this"));
        assert!(patterns.starts_with_question_word("which command should I use"));

        // Polite phrases
        assert!(patterns.starts_with_question_word("can you help me"));
        assert!(patterns.starts_with_question_word("could you explain"));
        assert!(patterns.starts_with_question_word("please show me"));
        assert!(patterns.starts_with_question_word("help me understand"));
        assert!(patterns.starts_with_question_word("explain docker to me"));

        // Not question words
        assert!(!patterns.starts_with_question_word("docker ps"));
        assert!(!patterns.starts_with_question_word("list all files"));
    }

    #[test]
    fn test_articles() {
        let patterns = CompiledPatterns::get();

        // English articles
        assert!(patterns.has_articles("run the tests"));
        assert!(patterns.has_articles("start a container"));
        assert!(patterns.has_articles("deploy an application"));
        assert!(patterns.has_articles("show me the logs"));
        assert!(patterns.has_articles("the container is running"));
        assert!(patterns.has_articles("a quick example"));

        // No articles (likely command)
        assert!(!patterns.has_articles("docker ps"));
        assert!(!patterns.has_articles("list files"));
        assert!(!patterns.has_articles("kubectl get pods"));
    }

    #[test]
    fn test_edge_cases() {
        let patterns = CompiledPatterns::get();

        // "run the tests" - has article, should be natural language
        assert!(patterns.has_articles("run the tests"));

        // "run tests" - no article, ambiguous
        assert!(!patterns.has_articles("run tests"));

        // Case insensitive question words
        assert!(patterns.starts_with_question_word("HOW do I"));
        assert!(patterns.starts_with_question_word("How Do I"));

        // Mixed content
        let input = "how do I run ./script.sh with flags?";
        assert!(patterns.starts_with_question_word(input));
        assert!(patterns.has_command_syntax(input)); // Contains ./
    }
}
