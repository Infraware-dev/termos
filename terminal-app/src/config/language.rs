/// Language configuration for natural language detection
///
/// This module provides structures and functions to load language-specific
/// patterns from external configuration files, enabling multilingual support
/// without code changes.
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// Complete language configuration loaded from TOML
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LanguageConfig {
    /// Default language code (e.g., "en", "it", "es")
    pub default_language: String,

    /// Map of language code to language patterns
    pub languages: HashMap<String, LanguagePatterns>,
}

/// Patterns for a specific language
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LanguagePatterns {
    /// Single-word natural language indicators (for typo detection)
    /// e.g., ["what", "how", "why"] for English
    pub single_words: Vec<String>,

    /// Question word patterns (regex)
    /// e.g., ["(?i)^(how|what|why)\\s"] for English
    pub question_patterns: Vec<String>,

    /// Article patterns (regex)
    /// e.g., ["\\s(a|an|the)\\s"] for English
    pub article_patterns: Vec<String>,

    /// Polite/request word patterns (regex)
    /// e.g., ["(?i)(^|\\s)(please|help)"] for English
    pub polite_patterns: Vec<String>,
}

impl LanguageConfig {
    /// Load language configuration from a TOML file
    ///
    /// # Arguments
    /// * `path` - Path to the language.toml configuration file
    ///
    /// # Returns
    /// * `Ok(LanguageConfig)` - Successfully loaded configuration
    /// * `Err(String)` - Error message if loading fails
    ///
    /// # Example
    /// ```no_run
    /// use infraware_terminal::config::language::LanguageConfig;
    ///
    /// let config = LanguageConfig::load_from_file("config/language.toml")
    ///     .expect("Failed to load language config");
    /// ```
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self, String> {
        let path = path.as_ref();
        let contents = fs::read_to_string(path)
            .map_err(|e| format!("Failed to read config file {:?}: {}", path, e))?;

        toml::from_str(&contents).map_err(|e| format!("Failed to parse TOML config: {}", e))
    }

    /// Load language configuration from default location
    ///
    /// Searches for configuration in the following order:
    /// 1. `./config/language.toml` (project directory)
    /// 2. `~/.config/infraware-terminal/language.toml` (user config)
    /// 3. Falls back to built-in English defaults
    ///
    /// # Returns
    /// * `LanguageConfig` - Loaded configuration or built-in defaults
    pub fn load_default() -> Self {
        // Try project directory first
        if let Ok(config) = Self::load_from_file("config/language.toml") {
            return config;
        }

        // Try user config directory
        if let Some(home) = dirs::home_dir() {
            let user_config = home.join(".config/infraware-terminal/language.toml");
            if let Ok(config) = Self::load_from_file(&user_config) {
                return config;
            }
        }

        // Fall back to built-in English defaults
        log::debug!("Using built-in English language patterns (no config file found)");
        Self::default_english()
    }

    /// Get patterns for the default language
    pub fn get_default_patterns(&self) -> Option<&LanguagePatterns> {
        self.languages.get(&self.default_language)
    }

    /// Get patterns for a specific language
    pub fn get_patterns(&self, lang: &str) -> Option<&LanguagePatterns> {
        self.languages.get(lang)
    }

    /// Built-in English defaults (fallback when no config file is found)
    pub fn default_english() -> Self {
        let mut languages = HashMap::new();
        languages.insert(
            "en".to_string(),
            LanguagePatterns {
                single_words: vec![
                    "what".to_string(),
                    "how".to_string(),
                    "why".to_string(),
                    "when".to_string(),
                    "where".to_string(),
                    "who".to_string(),
                    "which".to_string(),
                    "hello".to_string(),
                    "hi".to_string(),
                    "hey".to_string(),
                    "yes".to_string(),
                    "no".to_string(),
                    "ok".to_string(),
                    "thanks".to_string(),
                    "help".to_string(),
                ],
                question_patterns: vec![
                    r"(?i)^(how|what|why|when|where|who|which)\s".to_string(),
                    r"(?i)^(can you|could you|would you|will you)\s".to_string(),
                    r"(?i)^(please|help|show me|explain)\s".to_string(),
                ],
                article_patterns: vec![r"\s(a|an|the)\s".to_string(), r"^(a|an|the)\s".to_string()],
                polite_patterns: vec![r"(?i)(^|\s)(please|help|show me|explain)(\s|$)".to_string()],
            },
        );

        Self {
            default_language: "en".to_string(),
            languages,
        }
    }
}

impl Default for LanguageConfig {
    fn default() -> Self {
        Self::load_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_english_config() {
        let config = LanguageConfig::default_english();
        assert_eq!(config.default_language, "en");

        let en = config.get_default_patterns().unwrap();
        assert!(!en.single_words.is_empty());
        assert!(!en.question_patterns.is_empty());
        assert!(!en.article_patterns.is_empty());
    }

    #[test]
    fn test_get_patterns() {
        let config = LanguageConfig::default_english();

        // English should exist
        assert!(config.get_patterns("en").is_some());

        // Non-existent language
        assert!(config.get_patterns("xx").is_none());
    }

    #[test]
    fn test_load_from_file() {
        // Test with actual config file if it exists
        if Path::new("config/language.toml").exists() {
            let result = LanguageConfig::load_from_file("config/language.toml");
            assert!(result.is_ok(), "Should load config file successfully");

            let config = result.unwrap();
            assert!(
                !config.languages.is_empty(),
                "Should have at least one language"
            );
        }
    }
}
