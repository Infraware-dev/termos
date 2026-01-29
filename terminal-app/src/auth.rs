//! Authentication module for backend API
//!
//! This module provides authentication functionality for the Infraware Terminal
//! backend API. It follows SOLID principles with a trait-based design that
//! allows for easy testing and extension.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────┐
//! │  AuthConfig     │ ← Loads from environment
//! └────────┬────────┘
//!          │
//!          ▼
//! ┌─────────────────┐
//! │  Authenticator  │ ← Trait (interface)
//! └────────┬────────┘
//!          │
//!     ┌────┴────┐
//!     ▼         ▼
//! ┌───────┐ ┌───────┐
//! │ Http  │ │ Mock  │ ← Implementations
//! └───────┘ └───────┘
//! ```
//!
//! # Usage
//!
//! ```rust,ignore
//! use crate::auth::{AuthConfig, HttpAuthenticator, Authenticator};
//!
//! let config = AuthConfig::from_env();
//! if let (Some(url), Some(key)) = (&config.backend_url, &config.api_key) {
//!     let auth = HttpAuthenticator::new(url.clone());
//!     auth.authenticate(key).await?;
//! }
//! ```

mod authenticator;
mod config;
mod models;

pub use authenticator::{Authenticator, HttpAuthenticator};
pub use config::AuthConfig;
