//! Data models for the memory system
#![expect(
    dead_code,
    reason = "Phase 2 memory infrastructure - not yet connected to agent"
)]

use serde::{Deserialize, Serialize};

/// Type of interaction stored in memory
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DataType {
    #[serde(rename = "command")]
    Command,
    #[serde(rename = "natural_language")]
    NaturalLanguage,
}

/// Context about where an interaction occurred
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InteractionContext {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub working_dir: Option<String>,
}

/// A single interaction record stored in memory
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InteractionRecord {
    pub id: String,
    pub timestamp: String,
    pub data_type: DataType,
    pub intent: String,
    pub input: String,
    pub stderr: bool,
    /// Truncated output/result of the interaction (for commands: stdout/stderr)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
    pub context: InteractionContext,
}

impl InteractionRecord {
    /// Create a new interaction record with auto-generated id and timestamp
    pub fn new(
        data_type: DataType,
        intent: String,
        input: String,
        stderr: bool,
        working_dir: Option<String>,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            timestamp: chrono::Utc::now().to_rfc3339(),
            data_type,
            intent,
            input,
            stderr,
            output: None,
            context: InteractionContext { working_dir },
        }
    }

    /// Set the output field (stripped of ANSI codes, truncated to max_bytes)
    pub fn with_output(mut self, output: &str, max_bytes: usize) -> Self {
        // Strip ANSI escape codes and control characters
        let clean = strip_ansi_codes(output);
        let clean = clean.trim();

        let truncated = if clean.len() <= max_bytes {
            clean.to_string()
        } else {
            let mut end = max_bytes;
            while end > 0 && !clean.is_char_boundary(end) {
                end -= 1;
            }
            format!("{}...", clean[..end].trim())
        };
        if !truncated.is_empty() {
            self.output = Some(truncated);
        }
        self
    }
}

/// Strip ANSI escape sequences and control characters from a string
fn strip_ansi_codes(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '\x1b' {
            // Skip ESC sequences: ESC [ ... (letter)  or  ESC ] ... (BEL/ST)
            if chars.peek() == Some(&'[') || chars.peek() == Some(&']') {
                for ch in chars.by_ref() {
                    if ch.is_ascii_alphabetic() || ch == '\x07' {
                        break;
                    }
                }
            }
        } else if c == '\r' {
            // Skip carriage return
        } else if c.is_control() && c != '\n' && c != '\t' {
            // Skip other control characters (keep newlines and tabs)
        } else {
            result.push(c);
        }
    }

    result
}

/// A search result with similarity score
#[derive(Debug, Clone)]
pub struct SearchResult {
    pub record: InteractionRecord,
    pub similarity: f32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serde_roundtrip() {
        let record = InteractionRecord::new(
            DataType::Command,
            "executed docker-compose up".to_string(),
            "docker-compose up -d".to_string(),
            false,
            Some("/home/user/project".to_string()),
        );

        let json = serde_json::to_string(&record).unwrap();
        let deserialized: InteractionRecord = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.id, record.id);
        assert_eq!(deserialized.data_type, DataType::Command);
        assert_eq!(deserialized.intent, "executed docker-compose up");
        assert_eq!(deserialized.input, "docker-compose up -d");
        assert!(!deserialized.stderr);
        assert_eq!(
            deserialized.context.working_dir.as_deref(),
            Some("/home/user/project")
        );
    }

    #[test]
    fn test_data_type_serde_rename() {
        let json = serde_json::to_string(&DataType::Command).unwrap();
        assert_eq!(json, r#""command""#);

        let json = serde_json::to_string(&DataType::NaturalLanguage).unwrap();
        assert_eq!(json, r#""natural_language""#);

        let dt: DataType = serde_json::from_str(r#""command""#).unwrap();
        assert_eq!(dt, DataType::Command);

        let dt: DataType = serde_json::from_str(r#""natural_language""#).unwrap();
        assert_eq!(dt, DataType::NaturalLanguage);
    }

    #[test]
    fn test_new_generates_valid_id_and_timestamp() {
        let record = InteractionRecord::new(
            DataType::NaturalLanguage,
            "install redis".to_string(),
            "how do I install redis".to_string(),
            false,
            None,
        );

        // ID should be a valid UUID
        assert!(uuid::Uuid::parse_str(&record.id).is_ok());

        // Timestamp should be parseable
        assert!(chrono::DateTime::parse_from_rfc3339(&record.timestamp).is_ok());

        // No working dir
        assert!(record.context.working_dir.is_none());
    }

    #[test]
    fn test_context_working_dir_skip_serializing_none() {
        let ctx = InteractionContext { working_dir: None };
        let json = serde_json::to_string(&ctx).unwrap();
        assert!(!json.contains("working_dir"));

        let ctx = InteractionContext {
            working_dir: Some("/tmp".to_string()),
        };
        let json = serde_json::to_string(&ctx).unwrap();
        assert!(json.contains("working_dir"));
    }
}
