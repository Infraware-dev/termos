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

/// PERFORMANCE: Static lookup table for named colors.
/// Array index matches enum discriminant for O(1) lookup instead of match.
static NAMED_COLOR_TABLE: [Color32; 18] = [
    Color32::from_rgb(0, 0, 0),       // Black
    Color32::from_rgb(204, 0, 0),     // Red
    Color32::from_rgb(78, 154, 6),    // Green
    Color32::from_rgb(196, 160, 0),   // Yellow
    Color32::from_rgb(52, 101, 164),  // Blue
    Color32::from_rgb(117, 80, 123),  // Magenta
    Color32::from_rgb(6, 152, 154),   // Cyan
    Color32::from_rgb(211, 215, 207), // White
    Color32::from_rgb(85, 87, 83),    // BrightBlack
    Color32::from_rgb(239, 41, 41),   // BrightRed
    Color32::from_rgb(138, 226, 52),  // BrightGreen
    Color32::from_rgb(252, 233, 79),  // BrightYellow
    Color32::from_rgb(114, 159, 207), // BrightBlue
    Color32::from_rgb(173, 127, 168), // BrightMagenta
    Color32::from_rgb(52, 226, 226),  // BrightCyan
    Color32::from_rgb(238, 238, 236), // BrightWhite
    Color32::from_rgb(204, 204, 204), // Foreground (#cccccc)
    Color32::from_rgb(45, 45, 45),    // Background (#2d2d2d)
];

impl NamedColor {
    /// Convert to egui Color32 with typical terminal theme.
    /// PERFORMANCE: Uses static lookup table for O(1) access.
    #[inline]
    #[must_use]
    pub fn to_egui(self) -> Color32 {
        NAMED_COLOR_TABLE[self as usize]
    }
}

use std::sync::OnceLock;

/// Convert 256-color index to egui Color32 using a cached lookup table.
/// PERFORMANCE: O(1) lookup after first initialization.
fn indexed_to_egui(idx: u8) -> Color32 {
    static CELL_COLOR_TABLE: OnceLock<[Color32; 256]> = OnceLock::new();

    let table = CELL_COLOR_TABLE.get_or_init(|| {
        let mut t = [Color32::BLACK; 256];

        // Standard colors (0-15) - map to NamedColors
        t[0] = NamedColor::Black.to_egui();
        t[1] = NamedColor::Red.to_egui();
        t[2] = NamedColor::Green.to_egui();
        t[3] = NamedColor::Yellow.to_egui();
        t[4] = NamedColor::Blue.to_egui();
        t[5] = NamedColor::Magenta.to_egui();
        t[6] = NamedColor::Cyan.to_egui();
        t[7] = NamedColor::White.to_egui();
        t[8] = NamedColor::BrightBlack.to_egui();
        t[9] = NamedColor::BrightRed.to_egui();
        t[10] = NamedColor::BrightGreen.to_egui();
        t[11] = NamedColor::BrightYellow.to_egui();
        t[12] = NamedColor::BrightBlue.to_egui();
        t[13] = NamedColor::BrightMagenta.to_egui();
        t[14] = NamedColor::BrightCyan.to_egui();
        t[15] = NamedColor::BrightWhite.to_egui();

        // 216 colors (6x6x6 cube): indices 16-231
        for i in 16..=231 {
            let idx = i - 16;
            let r = (idx / 36) % 6;
            let g = (idx / 6) % 6;
            let b = idx % 6;
            let to_rgb = |v: u8| if v == 0 { 0 } else { 55 + v * 40 };
            t[i as usize] = Color32::from_rgb(to_rgb(r), to_rgb(g), to_rgb(b));
        }

        // Grayscale: indices 232-255
        for i in 232..=255 {
            let gray = 8 + (i - 232) * 10;
            t[i as usize] = Color32::from_rgb(gray, gray, gray);
        }

        t
    });

    table[idx as usize]
}

bitflags::bitflags! {
    /// Cell attributes packed into a single byte for better cache locality.
    ///
    /// PERFORMANCE: Reduces Cell size from ~16 bytes to ~9 bytes.
    #[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
    pub struct CellAttrs: u8 {
        const BOLD          = 0b0000_0001;
        const ITALIC        = 0b0000_0010;
        const UNDERLINE     = 0b0000_0100;
        const STRIKETHROUGH = 0b0000_1000;
        const DIM           = 0b0001_0000;
        const REVERSE       = 0b0010_0000;
        const HIDDEN        = 0b0100_0000;
        const BLINK         = 0b1000_0000;
    }
}

impl CellAttrs {
    /// Reset all attributes.
    #[inline]
    pub fn reset(&mut self) {
        *self = Self::empty();
    }

    // Getter methods (replaces field access)
    #[inline]
    #[expect(dead_code, reason = "API completeness")]
    pub fn bold(&self) -> bool {
        self.contains(Self::BOLD)
    }
    #[inline]
    #[expect(dead_code, reason = "API completeness")]
    pub fn italic(&self) -> bool {
        self.contains(Self::ITALIC)
    }
    #[inline]
    pub fn underline(&self) -> bool {
        self.contains(Self::UNDERLINE)
    }
    #[inline]
    pub fn strikethrough(&self) -> bool {
        self.contains(Self::STRIKETHROUGH)
    }
    #[inline]
    pub fn dim(&self) -> bool {
        self.contains(Self::DIM)
    }
    #[inline]
    pub fn reverse(&self) -> bool {
        self.contains(Self::REVERSE)
    }
    #[inline]
    pub fn hidden(&self) -> bool {
        self.contains(Self::HIDDEN)
    }
    #[inline]
    #[expect(dead_code, reason = "API completeness")]
    pub fn blink(&self) -> bool {
        self.contains(Self::BLINK)
    }

    // Setter methods (replaces field assignment)
    #[inline]
    pub fn set_bold(&mut self, on: bool) {
        self.set(Self::BOLD, on);
    }
    #[inline]
    pub fn set_italic(&mut self, on: bool) {
        self.set(Self::ITALIC, on);
    }
    #[inline]
    pub fn set_underline(&mut self, on: bool) {
        self.set(Self::UNDERLINE, on);
    }
    #[inline]
    pub fn set_strikethrough(&mut self, on: bool) {
        self.set(Self::STRIKETHROUGH, on);
    }
    #[inline]
    pub fn set_dim(&mut self, on: bool) {
        self.set(Self::DIM, on);
    }
    #[inline]
    pub fn set_reverse(&mut self, on: bool) {
        self.set(Self::REVERSE, on);
    }
    #[inline]
    pub fn set_hidden(&mut self, on: bool) {
        self.set(Self::HIDDEN, on);
    }
    #[inline]
    #[expect(dead_code, reason = "API completeness")]
    pub fn set_blink(&mut self, on: bool) {
        self.set(Self::BLINK, on);
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

#[allow(dead_code)]
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
