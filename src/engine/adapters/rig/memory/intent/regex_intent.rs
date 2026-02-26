//! Regex-based intent generator for Phase 1
//!
//! Generates semantic intent descriptions using simple text processing:
//! - Commands: extract base verb + first few tokens → "executed docker-compose up"
//! - Natural language: strip question words and punctuation → "install redis"

use std::collections::HashSet;

use anyhow::Result;

use crate::engine::adapters::rig::memory::models::DataType;
use crate::engine::adapters::rig::memory::traits::IntentGenerator;

/// Intent generator using regex/text heuristics (no API calls)
#[derive(Debug, Clone)]
pub struct RegexIntentGenerator {
    question_words: HashSet<&'static str>,
    prefix_commands: HashSet<&'static str>,
}

impl Default for RegexIntentGenerator {
    fn default() -> Self {
        Self::new()
    }
}

impl RegexIntentGenerator {
    pub fn new() -> Self {
        Self {
            question_words: [
                "how", "what", "where", "when", "why", "which", "who", "whom", "whose", "can",
                "could", "would", "should", "do", "does", "did", "is", "are", "was", "were",
                "will", "shall", "may", "might", "must", "has", "have", "had", "to", "i", "me",
                "my", "the", "a", "an",
            ]
            .into_iter()
            .collect(),
            prefix_commands: ["sudo", "nohup", "nice", "time", "strace", "env", "xargs"]
                .into_iter()
                .collect(),
        }
    }
}

impl IntentGenerator for RegexIntentGenerator {
    async fn generate(&self, input: &str, data_type: DataType) -> Result<String> {
        match data_type {
            DataType::Command => Ok(fallback_command_intent(input, &self.prefix_commands)),
            DataType::NaturalLanguage => {
                Ok(normalize_natural_language(input, &self.question_words))
            }
        }
    }
}

/// Generate a simple intent for a command by extracting the base command + first few args
fn fallback_command_intent(input: &str, prefix_commands: &HashSet<&str>) -> String {
    let tokens: Vec<&str> = input.split_whitespace().collect();
    if tokens.is_empty() {
        return "executed unknown command".to_string();
    }

    // Skip prefix commands (sudo, nohup, etc.)
    let start = tokens
        .iter()
        .position(|t| !prefix_commands.contains(t))
        .unwrap_or(0);

    // Take up to 3 meaningful tokens
    let meaningful: Vec<&str> = tokens[start..].iter().take(3).copied().collect();

    if meaningful.is_empty() {
        return "executed unknown command".to_string();
    }

    format!("executed {}", meaningful.join(" "))
}

/// Normalize a natural language query by removing question words and punctuation
fn normalize_natural_language(input: &str, question_words: &HashSet<&str>) -> String {
    let cleaned: String = input
        .chars()
        .filter(|c| c.is_alphanumeric() || c.is_whitespace() || *c == '-' || *c == '_')
        .collect();

    let tokens: Vec<&str> = cleaned
        .split_whitespace()
        .filter(|t| !question_words.contains(&t.to_lowercase().as_str()))
        .collect();

    if tokens.is_empty() {
        return input.trim().to_lowercase();
    }

    tokens.join(" ").to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_command_intent_simple() {
        let generator = RegexIntentGenerator::new();
        let intent = generator
            .generate("docker-compose up -d", DataType::Command)
            .await
            .unwrap();
        assert_eq!(intent, "executed docker-compose up -d");
    }

    #[tokio::test]
    async fn test_command_intent_strips_sudo() {
        let generator = RegexIntentGenerator::new();
        let intent = generator
            .generate("sudo apt-get install nginx", DataType::Command)
            .await
            .unwrap();
        assert_eq!(intent, "executed apt-get install nginx");
    }

    #[tokio::test]
    async fn test_command_intent_strips_multiple_prefixes() {
        let generator = RegexIntentGenerator::new();
        let intent = generator
            .generate("sudo nohup python server.py", DataType::Command)
            .await
            .unwrap();
        assert_eq!(intent, "executed python server.py");
    }

    #[tokio::test]
    async fn test_command_intent_long_command() {
        let generator = RegexIntentGenerator::new();
        let intent = generator
            .generate("pip install flask gunicorn redis celery", DataType::Command)
            .await
            .unwrap();
        assert_eq!(intent, "executed pip install flask");
    }

    #[tokio::test]
    async fn test_nl_normalization() {
        let generator = RegexIntentGenerator::new();
        let intent = generator
            .generate("how can I install redis", DataType::NaturalLanguage)
            .await
            .unwrap();
        assert_eq!(intent, "install redis");
    }

    #[tokio::test]
    async fn test_nl_normalization_question_mark() {
        let generator = RegexIntentGenerator::new();
        let intent = generator
            .generate("What is the best way to deploy?", DataType::NaturalLanguage)
            .await
            .unwrap();
        assert_eq!(intent, "best way deploy");
    }

    #[tokio::test]
    async fn test_nl_normalization_simple() {
        let generator = RegexIntentGenerator::new();
        let intent = generator
            .generate("install docker", DataType::NaturalLanguage)
            .await
            .unwrap();
        assert_eq!(intent, "install docker");
    }

    #[tokio::test]
    async fn test_empty_input() {
        let generator = RegexIntentGenerator::new();
        let intent = generator.generate("", DataType::Command).await.unwrap();
        assert_eq!(intent, "executed unknown command");
    }
}
