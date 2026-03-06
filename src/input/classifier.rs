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

/// Returns `true` when `input` contains a `?` that is NOT part of the
/// shell exit-code variable `$?`.
fn has_real_question_mark(input: &str) -> bool {
    let stripped = input.replace("$?", "");
    stripped.contains('?')
}

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

    /// Check if input is likely natural language.
    ///
    /// Uses layered heuristics: explicit question marks are the strongest NL
    /// signal and override command detection. When the first word is a known
    /// command, weaker signals (articles, phrase length) are ignored so that
    /// inputs like `cat a file` or `sudo apt install a package` stay
    /// classified as commands.
    fn is_natural_language(&self, input: &str) -> bool {
        let words: Vec<&str> = input.split_whitespace().collect();
        let first_word = words.first().copied().unwrap_or("");
        let starts_with_command = self.looks_like_command(first_word);

        // 1. Explicit question marks → strong NL signal, overrides command detection.
        //    Ignore `$?` (shell exit-code variable) — it is not a real question mark.
        if (has_real_question_mark(input) || input.contains('¿'))
            && !PATTERNS.shell_operators.is_match(input)
        {
            return true;
        }

        // 2. If the first word is a known command, classify as command.
        //    Articles, question words and phrase length are weak signals that
        //    must not override an unambiguous command prefix.
        if starts_with_command {
            return false;
        }

        // 3. NL patterns (question words, request phrases, articles) → NL
        //    Only reached when the first word is NOT a known command.
        if PATTERNS.natural_language.is_match(input)
            && !PATTERNS.shell_operators.is_match(input)
        {
            return true;
        }

        // 4. Explicit command syntax → not NL
        if PATTERNS.command_syntax.is_match(input) {
            return false;
        }

        // 5. Contains shell operators → command
        if PATTERNS.shell_operators.is_match(input) {
            return false;
        }

        // 6. Contains non-ASCII characters → likely non-English NL
        // (e.g., "chi sono io" in Italian, "什么是" in Chinese)
        if !input.is_ascii() {
            return true;
        }

        // 7. Long phrase (>5 words) without shell operators → likely NL
        if words.len() > 5 {
            return true;
        }

        // 8. Medium phrase (3-5 words) → likely NL
        //    (known-command-first inputs already returned at step 2)
        if words.len() >= 3 {
            return true;
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

        // Known commands and tools (shell builtins, coreutils, package
        // managers, container tooling, networking, etc.).  Kept sorted
        // alphabetically for easy maintenance.  M-DOCUMENTED-MAGIC: this
        // list biases the classifier toward "command" for 3+ word inputs
        // that start with a recognized executable name.
        const COMMON_COMMANDS: &[&str] = &[
            "adduser",
            "alias",
            "apk",
            "apt",
            "apt-get",
            "ar",
            "awk",
            "base64",
            "bat",
            "bc",
            "brew",
            "cal",
            "cargo",
            "cat",
            "cd",
            "chmod",
            "chown",
            "chsh",
            "clang",
            "clear",
            "cmake",
            "code",
            "cp",
            "crontab",
            "curl",
            "cut",
            "date",
            "dd",
            "deluser",
            "df",
            "diff",
            "dmesg",
            "dnf",
            "docker",
            "dpkg",
            "du",
            "echo",
            "emacs",
            "env",
            "exa",
            "exit",
            "export",
            "expr",
            "false",
            "fd",
            "file",
            "find",
            "flatpak",
            "free",
            "fuser",
            "fzf",
            "gcc",
            "git",
            "go",
            "grep",
            "groupadd",
            "groupdel",
            "gunzip",
            "gzip",
            "head",
            "help",
            "history",
            "hostname",
            "hostnamectl",
            "htop",
            "htpasswd",
            "ifconfig",
            "info",
            "insmod",
            "ip",
            "iptables",
            "java",
            "javac",
            "journalctl",
            "jq",
            "kill",
            "killall",
            "kubectl",
            "ldd",
            "less",
            "ln",
            "locale",
            "loginctl",
            "ls",
            "lsblk",
            "lsmod",
            "lsof",
            "make",
            "man",
            "md5sum",
            "mkdir",
            "modprobe",
            "more",
            "mount",
            "mv",
            "nano",
            "netstat",
            "nft",
            "nice",
            "nm",
            "nmap",
            "node",
            "nohup",
            "npm",
            "openssl",
            "pacman",
            "passwd",
            "patch",
            "pgrep",
            "pip",
            "pkill",
            "podman",
            "printenv",
            "printf",
            "ps",
            "pwd",
            "python",
            "python3",
            "readelf",
            "renice",
            "rg",
            "rm",
            "rmmod",
            "route",
            "rpm",
            "rsync",
            "rustc",
            "scp",
            "screen",
            "sed",
            "seq",
            "service",
            "sha256sum",
            "sleep",
            "snap",
            "sort",
            "source",
            "ss",
            "ssh",
            "stat",
            "strace",
            "strings",
            "strip",
            "su",
            "subl",
            "sudo",
            "sysctl",
            "systemctl",
            "tail",
            "tar",
            "tee",
            "test",
            "timedatectl",
            "tmux",
            "top",
            "touch",
            "tr",
            "true",
            "umount",
            "uname",
            "uniq",
            "unzip",
            "update-alternatives",
            "useradd",
            "userdel",
            "vi",
            "vim",
            "watch",
            "wc",
            "wget",
            "which",
            "whereis",
            "who",
            "whoami",
            "xargs",
            "yarn",
            "yes",
            "yq",
            "zip",
            "zypper",
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

    #[test]
    fn test_which_command_not_classified_as_nl() {
        let classifier = InputClassifier::new();
        // "which" is both a question word AND a Unix command.
        // When used as first word it must be classified as Command.
        assert!(matches!(
            classifier.classify("which python"),
            InputType::Command(_)
        ));
        assert!(matches!(
            classifier.classify("which node"),
            InputType::Command(_)
        ));
    }

    #[test]
    fn test_who_command_not_classified_as_nl() {
        let classifier = InputClassifier::new();
        assert!(matches!(
            classifier.classify("who am i"),
            InputType::Command(_)
        ));
    }

    #[test]
    fn test_command_with_article_not_classified_as_nl() {
        let classifier = InputClassifier::new();
        // Commands containing articles ("a", "an", "the") must stay commands
        assert!(matches!(
            classifier.classify("cat a file.txt"),
            InputType::Command(_)
        ));
        assert!(matches!(
            classifier.classify("touch a new_file"),
            InputType::Command(_)
        ));
        assert!(matches!(
            classifier.classify("sudo apt install a package"),
            InputType::Command(_)
        ));
        assert!(matches!(
            classifier.classify("mkdir the directory"),
            InputType::Command(_)
        ));
    }

    #[test]
    fn test_container_commands() {
        let classifier = InputClassifier::new();
        // Commands commonly used inside Docker/container environments
        assert!(matches!(
            classifier.classify("service nginx restart"),
            InputType::Command(_)
        ));
        assert!(matches!(
            classifier.classify("dpkg --list"),
            InputType::Command(_)
        ));
        assert!(matches!(
            classifier.classify("ip addr show"),
            InputType::Command(_)
        ));
        assert!(matches!(
            classifier.classify("useradd -m newuser"),
            InputType::Command(_)
        ));
        assert!(matches!(
            classifier.classify("apk add curl"),
            InputType::Command(_)
        ));
    }

    #[test]
    fn test_question_mark_overrides_command_prefix() {
        let classifier = InputClassifier::new();
        // Even if the first word is a command, a question mark is a strong NL signal
        assert!(matches!(
            classifier.classify("git what branch am I on?"),
            InputType::NaturalLanguage(_)
        ));
    }

    #[test]
    fn test_dollar_question_mark_not_classified_as_nl() {
        let classifier = InputClassifier::new();
        // $? is the shell exit-code variable, not a real question mark
        assert!(matches!(
            classifier.classify("echo $?"),
            InputType::Command(_)
        ));
        assert!(matches!(
            classifier.classify("test $? -eq 0"),
            InputType::Command(_)
        ));
        // But a real question mark after a command should still trigger NL
        assert!(matches!(
            classifier.classify("echo what is this?"),
            InputType::NaturalLanguage(_)
        ));
    }
}
