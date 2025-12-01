/// Configuration module for external configuration files
///
/// This module provides support for loading external configuration files
/// to enable runtime customization without code changes.
pub mod language;

pub use language::{LanguageConfig, LanguagePatterns};
