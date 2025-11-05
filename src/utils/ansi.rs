/// ANSI color utilities for terminal output
use std::fmt;

/// ANSI color codes for terminal output
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

/// ANSI formatting styles
pub struct AnsiStyle;

impl AnsiStyle {
    pub const BOLD: &'static str = "\x1b[1m";
    pub const DIM: &'static str = "\x1b[2m";
    pub const ITALIC: &'static str = "\x1b[3m";
    pub const UNDERLINE: &'static str = "\x1b[4m";
    pub const BLINK: &'static str = "\x1b[5m";
    pub const REVERSE: &'static str = "\x1b[7m";
    pub const HIDDEN: &'static str = "\x1b[8m";
    pub const STRIKETHROUGH: &'static str = "\x1b[9m";
    pub const RESET: &'static str = "\x1b[0m";

    /// Apply bold formatting
    pub fn bold(text: &str) -> String {
        format!("{}{}{}", Self::BOLD, text, Self::RESET)
    }

    /// Apply dim formatting
    pub fn dim(text: &str) -> String {
        format!("{}{}{}", Self::DIM, text, Self::RESET)
    }

    /// Apply italic formatting
    pub fn italic(text: &str) -> String {
        format!("{}{}{}", Self::ITALIC, text, Self::RESET)
    }

    /// Apply underline formatting
    pub fn underline(text: &str) -> String {
        format!("{}{}{}", Self::UNDERLINE, text, Self::RESET)
    }
}

/// Strip ANSI codes from a string
pub fn strip_ansi_codes(text: &str) -> String {
    let re = regex::Regex::new(r"\x1b\[[0-9;]*m").unwrap();
    re.replace_all(text, "").to_string()
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

    #[test]
    fn test_bold() {
        let text = AnsiStyle::bold("Important");
        assert!(text.starts_with("\x1b[1m"));
        assert!(text.ends_with("\x1b[0m"));
        assert!(text.contains("Important"));
    }

    #[test]
    fn test_strip_ansi_codes() {
        let colored = AnsiColor::Green.colorize("Success");
        let plain = strip_ansi_codes(&colored);
        assert_eq!(plain, "Success");
    }
}
