//! Cell, Color, and attribute definitions for terminal grid.

use egui::Color32;

/// Terminal color representation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Color {
    /// Named ANSI colors (0-7 normal, 8-15 bright).
    Named(NamedColor),
    /// 256-color palette index.
    Indexed(u8),
    /// True color RGB.
    Rgb(u8, u8, u8),
}

impl Default for Color {
    fn default() -> Self {
        Self::Named(NamedColor::Foreground)
    }
}

/// Named ANSI colors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NamedColor {
    // Standard colors (0-7)
    Black,
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
    White,
    // Bright colors (8-15)
    BrightBlack,
    BrightRed,
    BrightGreen,
    BrightYellow,
    BrightBlue,
    BrightMagenta,
    BrightCyan,
    BrightWhite,
    // Default colors
    Foreground,
    Background,
}

impl Color {
    /// Convert to egui Color32.
    #[must_use]
    pub fn to_egui(self, _is_foreground: bool) -> Color32 {
        match self {
            Self::Named(named) => named.to_egui(),
            Self::Indexed(idx) => indexed_to_egui(idx),
            Self::Rgb(r, g, b) => Color32::from_rgb(r, g, b),
        }
    }

    /// Create color from SGR parameter (30-37 fg, 40-47 bg).
    #[must_use]
    pub fn from_sgr_basic(code: u16) -> Option<Self> {
        let named = match code {
            30 | 40 => NamedColor::Black,
            31 | 41 => NamedColor::Red,
            32 | 42 => NamedColor::Green,
            33 | 43 => NamedColor::Yellow,
            34 | 44 => NamedColor::Blue,
            35 | 45 => NamedColor::Magenta,
            36 | 46 => NamedColor::Cyan,
            37 | 47 => NamedColor::White,
            90 | 100 => NamedColor::BrightBlack,
            91 | 101 => NamedColor::BrightRed,
            92 | 102 => NamedColor::BrightGreen,
            93 | 103 => NamedColor::BrightYellow,
            94 | 104 => NamedColor::BrightBlue,
            95 | 105 => NamedColor::BrightMagenta,
            96 | 106 => NamedColor::BrightCyan,
            97 | 107 => NamedColor::BrightWhite,
            39 => NamedColor::Foreground,
            49 => NamedColor::Background,
            _ => return None,
        };
        Some(Self::Named(named))
    }
}

impl NamedColor {
    /// Convert to egui Color32 with typical terminal theme.
    #[must_use]
    pub fn to_egui(self) -> Color32 {
        match self {
            Self::Black => Color32::from_rgb(0, 0, 0),
            Self::Red => Color32::from_rgb(204, 0, 0),
            Self::Green => Color32::from_rgb(78, 154, 6),
            Self::Yellow => Color32::from_rgb(196, 160, 0),
            Self::Blue => Color32::from_rgb(52, 101, 164),
            Self::Magenta => Color32::from_rgb(117, 80, 123),
            Self::Cyan => Color32::from_rgb(6, 152, 154),
            Self::White => Color32::from_rgb(211, 215, 207),
            Self::BrightBlack => Color32::from_rgb(85, 87, 83),
            Self::BrightRed => Color32::from_rgb(239, 41, 41),
            Self::BrightGreen => Color32::from_rgb(138, 226, 52),
            Self::BrightYellow => Color32::from_rgb(252, 233, 79),
            Self::BrightBlue => Color32::from_rgb(114, 159, 207),
            Self::BrightMagenta => Color32::from_rgb(173, 127, 168),
            Self::BrightCyan => Color32::from_rgb(52, 226, 226),
            Self::BrightWhite => Color32::from_rgb(238, 238, 236),
            Self::Foreground => Color32::from_rgb(211, 215, 207),
            Self::Background => Color32::from_rgb(0, 0, 0),
        }
    }
}

/// Convert 256-color index to egui Color32.
fn indexed_to_egui(idx: u8) -> Color32 {
    match idx {
        // Standard colors (0-15)
        0 => NamedColor::Black.to_egui(),
        1 => NamedColor::Red.to_egui(),
        2 => NamedColor::Green.to_egui(),
        3 => NamedColor::Yellow.to_egui(),
        4 => NamedColor::Blue.to_egui(),
        5 => NamedColor::Magenta.to_egui(),
        6 => NamedColor::Cyan.to_egui(),
        7 => NamedColor::White.to_egui(),
        8 => NamedColor::BrightBlack.to_egui(),
        9 => NamedColor::BrightRed.to_egui(),
        10 => NamedColor::BrightGreen.to_egui(),
        11 => NamedColor::BrightYellow.to_egui(),
        12 => NamedColor::BrightBlue.to_egui(),
        13 => NamedColor::BrightMagenta.to_egui(),
        14 => NamedColor::BrightCyan.to_egui(),
        15 => NamedColor::BrightWhite.to_egui(),
        // 216 colors (6x6x6 cube): indices 16-231
        16..=231 => {
            let idx = idx - 16;
            let r = (idx / 36) % 6;
            let g = (idx / 6) % 6;
            let b = idx % 6;
            let to_rgb = |v: u8| if v == 0 { 0 } else { 55 + v * 40 };
            Color32::from_rgb(to_rgb(r), to_rgb(g), to_rgb(b))
        }
        // Grayscale: indices 232-255
        232..=255 => {
            let gray = 8 + (idx - 232) * 10;
            Color32::from_rgb(gray, gray, gray)
        }
    }
}

/// Cell attributes (bold, italic, underline, etc.).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CellAttrs {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strikethrough: bool,
    pub dim: bool,
    pub reverse: bool,
    pub hidden: bool,
    pub blink: bool,
}

impl CellAttrs {
    /// Reset all attributes.
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

/// A single cell in the terminal grid.
#[derive(Debug, Clone)]
pub struct Cell {
    pub ch: char,
    pub fg: Color,
    pub bg: Color,
    pub attrs: CellAttrs,
}

impl Default for Cell {
    fn default() -> Self {
        Self {
            ch: ' ',
            fg: Color::Named(NamedColor::Foreground),
            bg: Color::Named(NamedColor::Background),
            attrs: CellAttrs::default(),
        }
    }
}

impl Cell {
    /// Create a new cell with a character.
    #[must_use]
    pub fn new(ch: char, fg: Color, bg: Color, attrs: CellAttrs) -> Self {
        Self { ch, fg, bg, attrs }
    }

    /// Reset cell to default (space with default colors).
    pub fn reset(&mut self) {
        self.ch = ' ';
        self.fg = Color::Named(NamedColor::Foreground);
        self.bg = Color::Named(NamedColor::Background);
        self.attrs.reset();
    }
}
