/// ANSI color utilities for terminal output
use std::fmt;

/// ANSI color codes for terminal output
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)] // Complete ANSI color set, not all variants used
pub enum AnsiColor {
    Black,
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
    White,
    BrightBlack,
    BrightRed,
    BrightGreen,
    BrightYellow,
    BrightBlue,
    BrightMagenta,
    BrightCyan,
    BrightWhite,
    Reset,
}

impl AnsiColor {
    /// Get the ANSI code for this color
    pub fn code(&self) -> &'static str {
        match self {
            AnsiColor::Black => "\x1b[30m",
            AnsiColor::Red => "\x1b[31m",
            AnsiColor::Green => "\x1b[32m",
            AnsiColor::Yellow => "\x1b[33m",
            AnsiColor::Blue => "\x1b[34m",
            AnsiColor::Magenta => "\x1b[35m",
            AnsiColor::Cyan => "\x1b[36m",
            AnsiColor::White => "\x1b[37m",
            AnsiColor::BrightBlack => "\x1b[90m",
            AnsiColor::BrightRed => "\x1b[91m",
            AnsiColor::BrightGreen => "\x1b[92m",
            AnsiColor::BrightYellow => "\x1b[93m",
            AnsiColor::BrightBlue => "\x1b[94m",
            AnsiColor::BrightMagenta => "\x1b[95m",
            AnsiColor::BrightCyan => "\x1b[96m",
            AnsiColor::BrightWhite => "\x1b[97m",
            AnsiColor::Reset => "\x1b[0m",
        }
    }

    /// Colorize a string with this color
    pub fn colorize(&self, text: &str) -> String {
        format!("{}{}{}", self.code(), text, AnsiColor::Reset.code())
    }
}

impl fmt::Display for AnsiColor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.code())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_colorize() {
        let text = AnsiColor::Red.colorize("Error");
        assert!(text.starts_with("\x1b[31m"));
        assert!(text.ends_with("\x1b[0m"));
        assert!(text.contains("Error"));
    }

    // Test all color codes
    #[test]
    fn test_color_codes() {
        assert_eq!(AnsiColor::Black.code(), "\x1b[30m");
        assert_eq!(AnsiColor::Red.code(), "\x1b[31m");
        assert_eq!(AnsiColor::Green.code(), "\x1b[32m");
        assert_eq!(AnsiColor::Yellow.code(), "\x1b[33m");
        assert_eq!(AnsiColor::Blue.code(), "\x1b[34m");
        assert_eq!(AnsiColor::Magenta.code(), "\x1b[35m");
        assert_eq!(AnsiColor::Cyan.code(), "\x1b[36m");
        assert_eq!(AnsiColor::White.code(), "\x1b[37m");
        assert_eq!(AnsiColor::Reset.code(), "\x1b[0m");
    }

    #[test]
    fn test_bright_color_codes() {
        assert_eq!(AnsiColor::BrightBlack.code(), "\x1b[90m");
        assert_eq!(AnsiColor::BrightRed.code(), "\x1b[91m");
        assert_eq!(AnsiColor::BrightGreen.code(), "\x1b[92m");
        assert_eq!(AnsiColor::BrightYellow.code(), "\x1b[93m");
        assert_eq!(AnsiColor::BrightBlue.code(), "\x1b[94m");
        assert_eq!(AnsiColor::BrightMagenta.code(), "\x1b[95m");
        assert_eq!(AnsiColor::BrightCyan.code(), "\x1b[96m");
        assert_eq!(AnsiColor::BrightWhite.code(), "\x1b[97m");
    }

    #[test]
    fn test_colorize_all_colors() {
        let colors = vec![
            AnsiColor::Black,
            AnsiColor::Red,
            AnsiColor::Green,
            AnsiColor::Yellow,
            AnsiColor::Blue,
            AnsiColor::Magenta,
            AnsiColor::Cyan,
            AnsiColor::White,
        ];

        for color in colors {
            let text = color.colorize("test");
            assert!(text.contains("test"));
            assert!(text.ends_with("\x1b[0m"));
        }
    }

    #[test]
    fn test_colorize_bright_colors() {
        let colors = vec![
            AnsiColor::BrightBlack,
            AnsiColor::BrightRed,
            AnsiColor::BrightGreen,
            AnsiColor::BrightYellow,
            AnsiColor::BrightBlue,
            AnsiColor::BrightMagenta,
            AnsiColor::BrightCyan,
            AnsiColor::BrightWhite,
        ];

        for color in colors {
            let text = color.colorize("test");
            assert!(text.contains("test"));
            assert!(text.ends_with("\x1b[0m"));
        }
    }

    #[test]
    fn test_display_trait() {
        assert_eq!(format!("{}", AnsiColor::Red), "\x1b[31m");
        assert_eq!(format!("{}", AnsiColor::Green), "\x1b[32m");
        assert_eq!(format!("{}", AnsiColor::BrightCyan), "\x1b[96m");
    }

    #[test]
    fn test_color_equality() {
        assert_eq!(AnsiColor::Red, AnsiColor::Red);
        assert_ne!(AnsiColor::Red, AnsiColor::Blue);
    }

    #[test]
    fn test_color_debug() {
        let debug_str = format!("{:?}", AnsiColor::Cyan);
        assert_eq!(debug_str, "Cyan");
    }
}
