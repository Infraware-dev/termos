//! Input classification: Command vs Natural Language
//!
//! This module provides a simple classifier to determine if user input
//! should be sent to the shell or to the LLM backend.
//!
//! The classification uses several heuristics:
//! 1. Question marks (?, ¿) indicate natural language
//! 2. Non-ASCII characters suggest non-English queries
//! 3. Long phrases without shell operators are likely natural language
//! 4. Question words (how, what, why, etc.) indicate queries
//! 5. Shell syntax (pipes, flags, paths) indicates commands

use once_cell::sync::Lazy;
use regex::RegexSet;

/// Represents the type of user input
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputType {
    /// A shell command to be executed
    Command(String),
    /// Natural language query for the LLM
    NaturalLanguage(String),
    /// Empty input
    Empty,
}

/// Precompiled regex patterns for efficient classification
struct Patterns {
    /// Patterns indicating natural language
    natural_language: RegexSet,
    /// Patterns indicating shell command syntax
    command_syntax: RegexSet,
    /// Shell operators
    shell_operators: RegexSet,
}

/// Global compiled patterns (initialized once)
static PATTERNS: Lazy<Patterns> = Lazy::new(|| Patterns {
    natural_language: RegexSet::new([
        r"[\?¿]",                                         // Question marks (universal)
        r"(?i)^(how|what|why|when|where|who|which)\s",    // Question words
        r"(?i)^(can you|could you|would you|will you)\s", // Request phrases
        r"(?i)(please|help me|show me|explain)\s",        // Polite phrases
        r"(?i)\s(a|an|the)\s",                            // Articles (indicate prose)
    ])
    .expect("Failed to compile natural_language patterns"),

    command_syntax: RegexSet::new([
        r"^[a-zA-Z0-9_-]+\s+--?[a-zA-Z]", // Flags: cmd --flag, cmd -f
        r"^\.{1,2}/",                     // Relative paths: ./, ../
        r"^/[a-zA-Z]",                    // Absolute paths: /usr/bin
        r"^\$[A-Z_]",                     // Env var start: $HOME
        r"^[a-z]+=$",                     // Env assignment: FOO=
    ])
    .expect("Failed to compile command_syntax patterns"),

    shell_operators: RegexSet::new([
        r"\|",      // Pipe
        r"&&|\|\|", // Logical operators
        r"[<>]",    // Redirects
        r";",       // Command separator
    ])
    .expect("Failed to compile shell_operators patterns"),
});

/// Simple input classifier
///
/// Uses heuristics to determine if input is a command or natural language.
/// Natural language queries are sent to the LLM, commands to the shell.
#[derive(Debug, Default)]
pub struct InputClassifier;

impl InputClassifier {
    /// Create a new classifier
    pub fn new() -> Self {
        Self
    }

    /// Classify user input as Command, NaturalLanguage, or Empty
    pub fn classify(&self, input: &str) -> InputType {
        let trimmed = input.trim();

        // Empty input
        if trimmed.is_empty() {
            return InputType::Empty;
        }

        // Check for explicit '?' prefix (user explicitly wants LLM)
        if let Some(query) = trimmed.strip_prefix('?') {
            let query = query.trim();
            if !query.is_empty() {
                return InputType::NaturalLanguage(query.to_string());
            }
        }

        // Check for natural language indicators
        if self.is_natural_language(trimmed) {
            return InputType::NaturalLanguage(trimmed.to_string());
        }

        // Default to command
        InputType::Command(trimmed.to_string())
    }

    /// Check if input is likely natural language
    fn is_natural_language(&self, input: &str) -> bool {
        // 1. Contains question marks → natural language
        if PATTERNS.natural_language.is_match(input) {
            // But not if it also has shell operators (e.g., "grep foo?")
            if !PATTERNS.shell_operators.is_match(input) {
                return true;
            }
        }

        // 2. Explicit command syntax → not natural language
        if PATTERNS.command_syntax.is_match(input) {
            return false;
        }

        // 3. Contains shell operators → command
        if PATTERNS.shell_operators.is_match(input) {
            return false;
        }

        // 4. Contains non-ASCII characters → likely non-English natural language
        // (e.g., "chi sono io" in Italian, "什么是" in Chinese)
        if !input.is_ascii() {
            return true;
        }

        // 5. Long phrase without shell operators → likely natural language
        let words: Vec<&str> = input.split_whitespace().collect();
        if words.len() > 5 {
            return true;
        }

        // 6. Medium phrase (3-5 words) without known command at start
        if words.len() >= 3
            && let Some(first_word) = words.first()
        {
            // If first word is not a likely command name, treat as NL
            if !self.looks_like_command(first_word) {
                return true;
            }
        }

        false
    }

    /// Check if a word looks like a Unix command name
    fn looks_like_command(&self, word: &str) -> bool {
        // Commands are typically lowercase alphanumeric with optional hyphens/underscores
        // Very short words (1-2 chars) are often commands (ls, cd, rm, cp, mv)
        if word.len() <= 2 && word.chars().all(|c| c.is_ascii_lowercase()) {
            return true;
        }

        // Common commands and tools
        const COMMON_COMMANDS: &[&str] = &[
            "ls",
            "cd",
            "rm",
            "cp",
            "mv",
            "cat",
            "grep",
            "find",
            "echo",
            "pwd",
            "mkdir",
            "touch",
            "chmod",
            "chown",
            "tar",
            "gzip",
            "gunzip",
            "zip",
            "unzip",
            "ssh",
            "scp",
            "rsync",
            "git",
            "docker",
            "kubectl",
            "npm",
            "yarn",
            "pip",
            "python",
            "python3",
            "node",
            "cargo",
            "make",
            "cmake",
            "gcc",
            "clang",
            "rustc",
            "go",
            "java",
            "javac",
            "curl",
            "wget",
            "ps",
            "top",
            "htop",
            "kill",
            "killall",
            "systemctl",
            "journalctl",
            "sudo",
            "su",
            "apt",
            "apt-get",
            "yum",
            "dnf",
            "pacman",
            "brew",
            "snap",
            "flatpak",
            "which",
            "whereis",
            "man",
            "info",
            "help",
            "clear",
            "exit",
            "history",
            "alias",
            "export",
            "source",
            "env",
            "head",
            "tail",
            "less",
            "more",
            "sort",
            "uniq",
            "wc",
            "cut",
            "awk",
            "sed",
            "xargs",
            "diff",
            "patch",
            "file",
            "stat",
            "df",
            "du",
            "free",
            "uname",
            "hostname",
            "whoami",
            "date",
            "cal",
            "bc",
            "expr",
            "test",
            "true",
            "false",
            "yes",
            "no",
            "sleep",
            "watch",
            "screen",
            "tmux",
            "vim",
            "vi",
            "nano",
            "emacs",
            "code",
            "subl",
            "bat",
            "exa",
            "fd",
            "rg",
            "fzf",
            "jq",
            "yq",
            "htpasswd",
            "openssl",
            "base64",
            "md5sum",
            "sha256sum",
        ];

        COMMON_COMMANDS.contains(&word.to_lowercase().as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_input() {
        let classifier = InputClassifier::new();
        assert_eq!(classifier.classify(""), InputType::Empty);
        assert_eq!(classifier.classify("   "), InputType::Empty);
    }

    #[test]
    fn test_explicit_query_prefix() {
        let classifier = InputClassifier::new();
        assert_eq!(
            classifier.classify("? how do I list files"),
            InputType::NaturalLanguage("how do I list files".to_string())
        );
        assert_eq!(
            classifier.classify("?chi sono io"),
            InputType::NaturalLanguage("chi sono io".to_string())
        );
    }

    #[test]
    fn test_question_marks() {
        let classifier = InputClassifier::new();
        assert!(matches!(
            classifier.classify("how do I list files?"),
            InputType::NaturalLanguage(_)
        ));
        assert!(matches!(
            classifier.classify("what is docker?"),
            InputType::NaturalLanguage(_)
        ));
    }

    #[test]
    fn test_non_ascii() {
        let classifier = InputClassifier::new();
        // Italian
        assert!(matches!(
            classifier.classify("chi sono io"),
            InputType::NaturalLanguage(_)
        ));
        // Spanish
        assert!(matches!(
            classifier.classify("cómo listar archivos"),
            InputType::NaturalLanguage(_)
        ));
    }

    #[test]
    fn test_commands() {
        let classifier = InputClassifier::new();
        assert!(matches!(
            classifier.classify("ls -la"),
            InputType::Command(_)
        ));
        assert!(matches!(
            classifier.classify("docker ps"),
            InputType::Command(_)
        ));
        assert!(matches!(
            classifier.classify("git status"),
            InputType::Command(_)
        ));
        assert!(matches!(
            classifier.classify("cat /etc/passwd"),
            InputType::Command(_)
        ));
    }

    #[test]
    fn test_shell_operators() {
        let classifier = InputClassifier::new();
        assert!(matches!(
            classifier.classify("ls | grep foo"),
            InputType::Command(_)
        ));
        assert!(matches!(
            classifier.classify("cat file > output"),
            InputType::Command(_)
        ));
        assert!(matches!(
            classifier.classify("cmd1 && cmd2"),
            InputType::Command(_)
        ));
    }

    #[test]
    fn test_long_phrases() {
        let classifier = InputClassifier::new();
        assert!(matches!(
            classifier.classify("show me all the docker containers running"),
            InputType::NaturalLanguage(_)
        ));
    }

    #[test]
    fn test_question_words() {
        let classifier = InputClassifier::new();
        assert!(matches!(
            classifier.classify("how to list files"),
            InputType::NaturalLanguage(_)
        ));
        assert!(matches!(
            classifier.classify("what is kubernetes"),
            InputType::NaturalLanguage(_)
        ));
        assert!(matches!(
            classifier.classify("why is my container failing"),
            InputType::NaturalLanguage(_)
        ));
    }

    #[test]
    fn test_polite_phrases() {
        let classifier = InputClassifier::new();
        assert!(matches!(
            classifier.classify("please help me"),
            InputType::NaturalLanguage(_)
        ));
        assert!(matches!(
            classifier.classify("can you explain docker"),
            InputType::NaturalLanguage(_)
        ));
    }

    #[test]
    fn test_whoami_classification() {
        let classifier = InputClassifier::new();
        // "whoami" is a known command
        assert!(matches!(
            classifier.classify("whoami"),
            InputType::Command(_)
        ));
        // "chi sono io" is Italian (non-ASCII 'ì' not present, but space pattern)
        // Actually "chi sono io" is all ASCII, but it's 3 words and "chi" is not a command
        assert!(matches!(
            classifier.classify("chi sono io"),
            InputType::NaturalLanguage(_)
        ));
    }
}
