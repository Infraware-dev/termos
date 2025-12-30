//! Custom prompt configuration.

use std::env;
use std::path::PathBuf;

/// Prompt configuration for the terminal.
#[derive(Debug, Clone)]
pub struct PromptConfig {
    /// Prefix before the prompt (e.g., "|~|")
    pub prefix: String,
    /// Username
    pub username: String,
    /// Hostname
    pub hostname: String,
    /// Whether user is root
    pub is_root: bool,
}

impl Default for PromptConfig {
    fn default() -> Self {
        Self::from_environment()
    }
}

impl PromptConfig {
    /// Create prompt config from environment.
    pub fn from_environment() -> Self {
        let username = env::var("USER")
            .or_else(|_| env::var("USERNAME"))
            .unwrap_or_else(|_| "user".to_string());

        let hostname = hostname::get()
            .map(|h| h.to_string_lossy().to_string())
            .unwrap_or_else(|_| "localhost".to_string());

        // Check if root (UID 0 on Unix)
        let is_root = unsafe { libc::getuid() } == 0;

        Self {
            prefix: "|~|".to_string(),
            username,
            hostname,
            is_root,
        }
    }

    /// Get the current working directory, shortened with ~ for home.
    pub fn get_cwd(&self) -> String {
        let cwd = env::current_dir().unwrap_or_else(|_| PathBuf::from("~"));

        // Try to replace home directory with ~
        if let Ok(home) = env::var("HOME") {
            let cwd_str = cwd.to_string_lossy();
            if cwd_str.starts_with(&home) {
                return cwd_str.replacen(&home, "~", 1);
            }
        }

        cwd.to_string_lossy().to_string()
    }

    /// Format the full prompt string.
    pub fn format(&self, cwd: &str) -> String {
        let symbol = if self.is_root { "#" } else { "$" };
        format!(
            "{} {}@{}:{}{}",
            self.prefix, self.username, self.hostname, cwd, symbol
        )
    }

}
