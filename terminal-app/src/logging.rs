/// Logging configuration for Infraware Terminal
///
/// Uses log4rs with size-based rotation configured via environment variables.
use anyhow::{Context, Result};
use log::LevelFilter;
use log4rs::{
    append::{
        console::ConsoleAppender,
        rolling_file::{
            policy::compound::{
                roll::fixed_window::FixedWindowRoller, trigger::size::SizeTrigger, CompoundPolicy,
            },
            RollingFileAppender,
        },
    },
    config::{Appender, Config, Root},
    encode::pattern::PatternEncoder,
};
use std::path::PathBuf;

/// Initialize the logging system from environment variables
///
/// Environment variables:
/// - `LOG_LEVEL`: Log level (trace, debug, info, warn, error). Default: info
/// - `LOG_MAX_SIZE_MB`: Max log file size in MB. Default: 10
/// - `LOG_MAX_FILES`: Max number of rotated files. Default: 5
/// - `LOG_PATH`: Custom log directory. Default: platform-specific
pub fn init() -> Result<()> {
    let config = LogConfig::from_env();
    let log_dir = config.log_path()?;

    // Create log directory if it doesn't exist
    std::fs::create_dir_all(&log_dir)
        .with_context(|| format!("Failed to create log directory: {}", log_dir.display()))?;

    // Log file path
    let log_file = log_dir.join("infraware.log");

    // Console appender (for development)
    let console = ConsoleAppender::builder()
        .encoder(Box::new(PatternEncoder::new(
            "[{d(%H:%M:%S%.3f)}] {h({l})} {m}\n",
        )))
        .build();

    // File appender with size-based rotation
    let size_trigger = SizeTrigger::new(config.max_size_bytes());

    // Fixed window roller: infraware.1.log.gz, infraware.2.log.gz, ...
    let roller_pattern = log_dir.join("infraware.{}.log.gz").display().to_string();
    let roller = FixedWindowRoller::builder()
        .build(&roller_pattern, config.max_files)
        .context("Failed to create roller")?;

    let policy = CompoundPolicy::new(Box::new(size_trigger), Box::new(roller));

    let file = RollingFileAppender::builder()
        .encoder(Box::new(PatternEncoder::new(
            "[{d(%Y-%m-%d %H:%M:%S%.3f)}] {l} [{t}] {m}\n",
        )))
        .build(log_file, Box::new(policy))
        .context("Failed to create rolling file appender")?;

    // Build log4rs config
    let log_config = Config::builder()
        .appender(Appender::builder().build("console", Box::new(console)))
        .appender(Appender::builder().build("file", Box::new(file)))
        .build(
            Root::builder()
                .appender("file")
                .appender("console")
                .build(config.log_level),
        )
        .context("Failed to build log4rs config")?;

    log4rs::init_config(log_config).context("Failed to initialize log4rs")?;

    log::info!("Logging initialized at {}", log_dir.display());
    log::info!(
        "Log level: {:?}, max size: {}MB, max files: {}",
        config.log_level,
        config.max_size_mb,
        config.max_files
    );

    Ok(())
}

/// Logging configuration from environment variables
struct LogConfig {
    log_level: LevelFilter,
    max_size_mb: u64,
    max_files: u32,
    custom_path: Option<PathBuf>,
}

impl LogConfig {
    /// Load configuration from environment variables
    fn from_env() -> Self {
        let log_level = std::env::var("LOG_LEVEL")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(LevelFilter::Info);

        let max_size_mb = std::env::var("LOG_MAX_SIZE_MB")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(10);

        let max_files = std::env::var("LOG_MAX_FILES")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(5);

        let custom_path = std::env::var("LOG_PATH").ok().and_then(|s| {
            if s.trim().is_empty() {
                None
            } else {
                Some(PathBuf::from(s))
            }
        });

        Self {
            log_level,
            max_size_mb,
            max_files,
            custom_path,
        }
    }

    /// Get log directory path (cross-platform)
    fn log_path(&self) -> Result<PathBuf> {
        if let Some(path) = &self.custom_path {
            return Ok(path.clone());
        }

        // Platform-specific defaults
        let base_dir = if cfg!(target_os = "macos") {
            // macOS: ~/Library/Logs/infraware-terminal
            dirs::home_dir()
                .context("Failed to get home directory")?
                .join("Library")
                .join("Logs")
                .join("infraware-terminal")
        } else if cfg!(target_os = "windows") {
            // Windows: %APPDATA%\infraware-terminal\logs
            dirs::data_local_dir()
                .context("Failed to get local data directory")?
                .join("infraware-terminal")
                .join("logs")
        } else {
            // Linux: ~/.local/share/infraware-terminal/logs
            dirs::data_local_dir()
                .context("Failed to get local data directory")?
                .join("infraware-terminal")
                .join("logs")
        };

        Ok(base_dir)
    }

    /// Get max file size in bytes
    fn max_size_bytes(&self) -> u64 {
        self.max_size_mb * 1024 * 1024
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log_config_defaults() {
        // Clear env vars for test
        std::env::remove_var("LOG_LEVEL");
        std::env::remove_var("LOG_MAX_SIZE_MB");
        std::env::remove_var("LOG_MAX_FILES");
        std::env::remove_var("LOG_PATH");

        let config = LogConfig::from_env();
        assert_eq!(config.log_level, LevelFilter::Info);
        assert_eq!(config.max_size_mb, 10);
        assert_eq!(config.max_files, 5);
        assert!(config.custom_path.is_none());
    }

    #[test]
    fn test_log_config_from_env() {
        std::env::set_var("LOG_LEVEL", "debug");
        std::env::set_var("LOG_MAX_SIZE_MB", "20");
        std::env::set_var("LOG_MAX_FILES", "10");

        let config = LogConfig::from_env();
        assert_eq!(config.log_level, LevelFilter::Debug);
        assert_eq!(config.max_size_mb, 20);
        assert_eq!(config.max_files, 10);

        // Cleanup
        std::env::remove_var("LOG_LEVEL");
        std::env::remove_var("LOG_MAX_SIZE_MB");
        std::env::remove_var("LOG_MAX_FILES");
    }

    #[test]
    fn test_max_size_bytes() {
        std::env::set_var("LOG_MAX_SIZE_MB", "5");
        let config = LogConfig::from_env();
        assert_eq!(config.max_size_bytes(), 5 * 1024 * 1024);
        std::env::remove_var("LOG_MAX_SIZE_MB");
    }

    #[test]
    fn test_log_path_cross_platform() {
        std::env::remove_var("LOG_PATH");
        let config = LogConfig::from_env();
        let path = config.log_path().expect("Failed to get log path");

        // Path should contain "infraware-terminal"
        assert!(path.to_string_lossy().contains("infraware-terminal"));

        // Path should contain "logs" or "Logs"
        let path_str = path.to_string_lossy().to_lowercase();
        assert!(path_str.contains("logs"));
    }
}
