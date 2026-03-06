//! Command line interface arguments.

use std::str::FromStr;

use clap::Parser;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
pub struct Args {
    /// Application log level (trace, debug, info, warn, error). Can also be set via RUST_LOG or LOG_LEVEL environment variables.
    #[arg(long, env = "RUST_LOG", short = 'l', default_value = "info")]
    pub log_level: LogLevel,
    /// Use a test Debian image as the PTY backend instead of the host system. This is useful for testing and debugging in a consistent environment.
    #[cfg(feature = "pty-test_container")]
    #[arg(long, env = "USE_PTY_TEST_CONTAINER", default_value_t = false)]
    pub use_pty_test_container: bool,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

impl std::fmt::Display for LogLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            LogLevel::Trace => "trace",
            LogLevel::Debug => "debug",
            LogLevel::Info => "info",
            LogLevel::Warn => "warn",
            LogLevel::Error => "error",
        };
        write!(f, "{s}")
    }
}

impl From<LogLevel> for tracing::level_filters::LevelFilter {
    fn from(level: LogLevel) -> Self {
        match level {
            LogLevel::Trace => tracing::level_filters::LevelFilter::TRACE,
            LogLevel::Debug => tracing::level_filters::LevelFilter::DEBUG,
            LogLevel::Info => tracing::level_filters::LevelFilter::INFO,
            LogLevel::Warn => tracing::level_filters::LevelFilter::WARN,
            LogLevel::Error => tracing::level_filters::LevelFilter::ERROR,
        }
    }
}

impl FromStr for LogLevel {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "trace" => Ok(LogLevel::Trace),
            "debug" => Ok(LogLevel::Debug),
            "info" => Ok(LogLevel::Info),
            "warn" => Ok(LogLevel::Warn),
            "error" => Ok(LogLevel::Error),
            _ => Err(format!("Invalid log level: {s}")),
        }
    }
}
