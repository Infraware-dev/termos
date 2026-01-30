//! Authenticator trait and implementations
//!
//! This module provides the authentication abstraction following SOLID principles:
//! - Single Responsibility: Only handles authentication
//! - Open/Closed: New authenticators can be added without modifying existing code
//! - Liskov Substitution: MockAuthenticator can replace HttpAuthenticator
//! - Interface Segregation: Minimal trait with only necessary methods
//! - Dependency Inversion: Consumers depend on trait, not concrete types

use std::fmt::Debug;
use std::time::Duration;

use anyhow::Result;
use async_trait::async_trait;

use super::models::{AuthRequest, AuthResponse, AuthStatus};

/// Trait for authentication services (Interface Segregation Principle)
///
/// Implementors must be thread-safe (Send + Sync) and debuggable.
#[async_trait]
pub trait Authenticator: Send + Sync + Debug {
    /// Authenticate with the backend using an API key
    ///
    /// # Arguments
    /// * `api_key` - The Anthropic API key to validate
    ///
    /// # Returns
    /// * `Ok(AuthResponse)` on successful authentication
    /// * `Err` if authentication fails or network error occurs
    async fn authenticate(&self, api_key: &str) -> Result<AuthResponse>;

    /// Check current authentication status
    ///
    /// # Returns
    /// * `Ok(true)` if authenticated
    /// * `Ok(false)` if not authenticated
    /// * `Err` on network or server error
    #[allow(dead_code)]
    async fn check_status(&self) -> Result<bool>;
}

/// HTTP-based authenticator for production use
///
/// Communicates with the FastAPI backend at the configured URL.
pub struct HttpAuthenticator {
    base_url: String,
    client: reqwest::Client,
}

impl HttpAuthenticator {
    /// Default timeout for authentication requests (10 seconds)
    const AUTH_TIMEOUT_SECS: u64 = 10;

    /// Create a new HTTP authenticator
    ///
    /// # Arguments
    /// * `base_url` - Base URL of the backend API (e.g., "http://localhost:8000")
    pub fn new(base_url: String) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(Self::AUTH_TIMEOUT_SECS))
            .build()
            .unwrap_or_default();

        Self { base_url, client }
    }
}

/// Custom Debug implementation to avoid exposing internal reqwest::Client details
impl Debug for HttpAuthenticator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HttpAuthenticator")
            .field("base_url", &self.base_url)
            .field("client", &"<reqwest::Client>")
            .finish()
    }
}

#[async_trait]
impl Authenticator for HttpAuthenticator {
    async fn authenticate(&self, api_key: &str) -> Result<AuthResponse> {
        log::debug!("Authenticating with backend at {}", self.base_url);

        let request = AuthRequest {
            api_key: api_key.to_string(),
        };

        let response = self
            .client
            .post(format!("{}/api/auth", self.base_url))
            .json(&request)
            .send()
            .await?;

        if response.status().is_success() {
            let auth_response: AuthResponse = response.json().await?;
            log::info!("Authentication successful: {}", auth_response.message);
            Ok(auth_response)
        } else {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            log::error!("Authentication failed ({}): {}", status, error_text);
            anyhow::bail!("Authentication failed ({}): {}", status, error_text)
        }
    }

    async fn check_status(&self) -> Result<bool> {
        log::debug!("Checking auth status at {}", self.base_url);

        let response = self
            .client
            .get(format!("{}/api/get-auth", self.base_url))
            .send()
            .await?;

        if response.status().is_success() {
            let status: AuthStatus = response.json().await?;
            log::debug!("Auth status: authenticated={}", status.authenticated);
            Ok(status.authenticated)
        } else {
            let error_text = response.text().await.unwrap_or_default();
            log::error!("Failed to check auth status: {}", error_text);
            anyhow::bail!("Failed to check auth status: {}", error_text)
        }
    }
}

/// Mock authenticator for testing (Liskov Substitution Principle)
///
/// Always succeeds authentication without making network calls.
#[derive(Debug, Default)]
pub struct MockAuthenticator;

impl MockAuthenticator {
    /// Create a new mock authenticator
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Authenticator for MockAuthenticator {
    async fn authenticate(&self, _api_key: &str) -> Result<AuthResponse> {
        log::debug!("Mock authentication (always succeeds)");
        Ok(AuthResponse {
            success: true,
            message: "Mock authentication successful".to_string(),
        })
    }

    async fn check_status(&self) -> Result<bool> {
        log::debug!("Mock auth status check (always authenticated)");
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_http_authenticator_debug() {
        let auth = HttpAuthenticator::new("http://localhost:8000".to_string());
        let debug_str = format!("{:?}", auth);
        assert!(debug_str.contains("HttpAuthenticator"));
        assert!(debug_str.contains("http://localhost:8000"));
        // Should NOT contain actual client internals
        assert!(!debug_str.contains("reqwest::Client {"));
    }

    #[test]
    fn test_mock_authenticator_debug() {
        let auth = MockAuthenticator::new();
        let debug_str = format!("{:?}", auth);
        assert!(debug_str.contains("MockAuthenticator"));
    }

    #[tokio::test]
    async fn test_mock_authenticator_authenticate() {
        let auth = MockAuthenticator::new();
        let result = auth.authenticate("test-key").await;
        assert!(result.is_ok());
        let response = result.unwrap();
        assert!(response.success);
    }

    #[tokio::test]
    async fn test_mock_authenticator_check_status() {
        let auth = MockAuthenticator::new();
        let result = auth.check_status().await;
        assert!(result.is_ok());
        assert!(result.unwrap());
    }
}
