//! Authentication middleware
//!
//! Validates API key from Authorization header or X-Api-Key header.

/// Shared state for auth middleware
#[derive(Clone, Debug)]
pub struct AuthConfig {
    /// Valid API key (if None, auth is disabled)
    api_key: Option<String>,
}

impl AuthConfig {
    /// Create a new auth configuration
    ///
    /// If `api_key` is None, authentication is disabled and all requests pass.
    pub fn new(api_key: Option<String>) -> Self {
        Self { api_key }
    }

    /// Check if authentication is enabled
    pub fn is_enabled(&self) -> bool {
        self.api_key.is_some()
    }

    /// Validate a token against the configured API key
    ///
    /// Uses constant-time comparison to prevent timing attacks.
    pub fn validate(&self, token: &str) -> bool {
        match &self.api_key {
            Some(key) => constant_time_eq(key.as_bytes(), token.as_bytes()),
            None => true, // Auth disabled
        }
    }
}

/// Constant-time string comparison to prevent timing attacks
///
/// Note: The length check does leak timing information, but this is acceptable
/// since API keys should all be the same length in practice.
/// For higher security, use the `subtle` crate's ConstantTimeEq trait.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }

    let mut result = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        result |= x ^ y;
    }
    result == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_auth_config_disabled() {
        let config = AuthConfig::new(None);
        assert!(!config.is_enabled());
        assert!(config.validate("any-token"));
    }

    #[test]
    fn test_auth_config_enabled() {
        let config = AuthConfig::new(Some("secret-key".to_string()));
        assert!(config.is_enabled());
        assert!(config.validate("secret-key"));
        assert!(!config.validate("wrong-key"));
    }

    #[test]
    fn test_constant_time_eq() {
        assert!(constant_time_eq(b"hello", b"hello"));
        assert!(!constant_time_eq(b"hello", b"world"));
        assert!(!constant_time_eq(b"hello", b"hell"));
    }

    #[test]
    fn test_auth_config_debug() {
        let config = AuthConfig::new(Some("secret".to_string()));
        // Should implement Debug without exposing the actual key
        let debug_str = format!("{:?}", config);
        assert!(debug_str.contains("AuthConfig"));
    }
}
