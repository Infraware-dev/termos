//! Authentication configuration
//!
//! This module handles loading authentication configuration from
//! environment variables following the Single Responsibility Principle.

/// Authentication configuration loaded from environment variables
#[derive(Debug)]
pub struct AuthConfig {
    /// Backend API URL (from INFRAWARE_BACKEND_URL)
    pub backend_url: Option<String>,
    /// Anthropic API key (from ANTHROPIC_API_KEY)
    pub api_key: Option<String>,
}

impl AuthConfig {
    /// Load configuration from environment variables
    ///
    /// Reads:
    /// - `INFRAWARE_BACKEND_URL`: Backend API endpoint
    /// - `ANTHROPIC_API_KEY`: API key for authentication
    pub fn from_env() -> Self {
        Self {
            backend_url: std::env::var("INFRAWARE_BACKEND_URL").ok(),
            api_key: std::env::var("ANTHROPIC_API_KEY").ok(),
        }
    }

    /// Check if both backend URL and API key are configured
    pub fn is_configured(&self) -> bool {
        self.backend_url.is_some() && self.api_key.is_some()
    }

    /// Check if only backend URL is configured (missing API key)
    pub fn has_backend_only(&self) -> bool {
        self.backend_url.is_some() && self.api_key.is_none()
    }
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self::from_env()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_configured_both_present() {
        let config = AuthConfig {
            backend_url: Some("http://localhost:8000".to_string()),
            api_key: Some("test-key".to_string()),
        };
        assert!(config.is_configured());
    }

    #[test]
    fn test_is_configured_missing_url() {
        let config = AuthConfig {
            backend_url: None,
            api_key: Some("test-key".to_string()),
        };
        assert!(!config.is_configured());
    }

    #[test]
    fn test_is_configured_missing_key() {
        let config = AuthConfig {
            backend_url: Some("http://localhost:8000".to_string()),
            api_key: None,
        };
        assert!(!config.is_configured());
        assert!(config.has_backend_only());
    }

    #[test]
    fn test_is_configured_both_missing() {
        let config = AuthConfig {
            backend_url: None,
            api_key: None,
        };
        assert!(!config.is_configured());
        assert!(!config.has_backend_only());
    }
}
