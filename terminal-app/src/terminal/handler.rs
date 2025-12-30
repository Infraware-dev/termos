//! VTE Perform trait implementation for terminal emulation.

use super::cell::Color;
use super::grid::TerminalGrid;
use log::debug;

/// Terminal handler that implements vte::Perform to process escape sequences.
#[derive(Debug)]
pub struct TerminalHandler {
    grid: TerminalGrid,
    /// Window title set by OSC sequences.
    window_title: String,
}

impl TerminalHandler {
    /// Create a new terminal handler with given dimensions.
    #[must_use]
    pub fn new(rows: u16, cols: u16) -> Self {
        Self {
            grid: TerminalGrid::new(rows, cols),
            window_title: String::from("Infraware Terminal"),
        }
    }

    /// Get a reference to the grid.
    pub fn grid(&self) -> &TerminalGrid {
        &self.grid
    }

    /// Get a mutable reference to the grid.
    pub fn grid_mut(&mut self) -> &mut TerminalGrid {
        &mut self.grid
    }

    /// Get the window title.
    #[must_use]
    pub fn window_title(&self) -> &str {
        &self.window_title
    }

    /// Resize the terminal.
    pub fn resize(&mut self, rows: u16, cols: u16) {
        self.grid.resize(rows, cols);
    }

    /// Process SGR (Select Graphic Rendition) parameters.
    fn process_sgr(&mut self, params: &[&[u16]]) {
        let mut iter = params.iter().peekable();

        while let Some(param) = iter.next() {
            let code = param.first().copied().unwrap_or(0);

            match code {
                0 => self.grid.reset_attrs(),
                1 => self.grid.set_bold(true),
                2 => self.grid.set_dim(true),
                3 => self.grid.set_italic(true),
                4 => self.grid.set_underline(true),
                5 | 6 => {} // Blink (ignored)
                7 => self.grid.set_reverse(true),
                8 => self.grid.set_hidden(true),
                9 => self.grid.set_strikethrough(true),
                21 => self.grid.set_bold(false),
                22 => {
                    self.grid.set_bold(false);
                    self.grid.set_dim(false);
                }
                23 => self.grid.set_italic(false),
                24 => self.grid.set_underline(false),
                25 => {} // Blink off
                27 => self.grid.set_reverse(false),
                28 => self.grid.set_hidden(false),
                29 => self.grid.set_strikethrough(false),

                // Standard foreground colors
                30..=37 | 90..=97 => {
                    if let Some(color) = Color::from_sgr_basic(code) {
                        self.grid.set_fg(color);
                    }
                }

                // Extended foreground color
                38 => {
                    if let Some(color) = self.parse_extended_color(&mut iter) {
                        self.grid.set_fg(color);
                    }
                }

                // Default foreground
                39 => {
                    if let Some(color) = Color::from_sgr_basic(code) {
                        self.grid.set_fg(color);
                    }
                }

                // Standard background colors
                40..=47 | 100..=107 => {
                    if let Some(color) = Color::from_sgr_basic(code) {
                        self.grid.set_bg(color);
                    }
                }

                // Extended background color
                48 => {
                    if let Some(color) = self.parse_extended_color(&mut iter) {
                        self.grid.set_bg(color);
                    }
                }

                // Default background
                49 => {
                    if let Some(color) = Color::from_sgr_basic(code) {
                        self.grid.set_bg(color);
                    }
                }

                _ => {
                    debug!("Unknown SGR code: {}", code);
                }
            }
        }
    }

    /// Parse extended color (256-color or RGB).
    fn parse_extended_color<'a>(
        &self,
        iter: &mut std::iter::Peekable<impl Iterator<Item = &'a &'a [u16]>>,
    ) -> Option<Color> {
        let next = iter.next()?;
        let color_type = next.first().copied()?;

        match color_type {
            5 => {
                // 256-color: 38;5;N or 48;5;N
                let idx = iter.next()?.first().copied()?;
                Some(Color::Indexed(idx as u8))
            }
            2 => {
                // RGB: 38;2;R;G;B or 48;2;R;G;B
                let r = iter.next()?.first().copied()? as u8;
                let g = iter.next()?.first().copied()? as u8;
                let b = iter.next()?.first().copied()? as u8;
                Some(Color::Rgb(r, g, b))
            }
            _ => None,
        }
    }
}

impl vte::Perform for TerminalHandler {
    /// Print a character to the terminal.
    fn print(&mut self, c: char) {
        self.grid.put_char(c);
    }

    /// Execute a C0 or C1 control character.
    fn execute(&mut self, byte: u8) {
        match byte {
            // Bell
            0x07 => {
                debug!("Bell");
            }
            // Backspace
            0x08 => {
                self.grid.backspace();
            }
            // Horizontal tab
            0x09 => {
                self.grid.tab();
            }
            // Line feed, vertical tab, form feed
            0x0A | 0x0B | 0x0C => {
                self.grid.linefeed();
            }
            // Carriage return
            0x0D => {
                self.grid.carriage_return();
            }
            // Shift out (to G1)
            0x0E => {
                debug!("Shift out (ignored)");
            }
            // Shift in (to G0)
            0x0F => {
                debug!("Shift in (ignored)");
            }
            _ => {
                debug!("Unknown control char: 0x{:02x}", byte);
            }
        }
    }

    /// Process a CSI (Control Sequence Introducer) sequence.
    fn csi_dispatch(
        &mut self,
        params: &vte::Params,
        intermediates: &[u8],
        _ignore: bool,
        action: char,
    ) {
        // Convert params to a more convenient format
        let params: Vec<&[u16]> = params.iter().collect();

        // Helper to get a parameter with a default value
        let param = |idx: usize, default: u16| -> u16 {
            params
                .get(idx)
                .and_then(|p| p.first().copied())
                .filter(|&v| v != 0)
                .unwrap_or(default)
        };

        // Check for DEC private mode sequences
        let is_dec_private = intermediates.first() == Some(&b'?');

        match action {
            // Cursor Up (CUU)
            'A' => {
                self.grid.move_up(param(0, 1));
            }
            // Cursor Down (CUD)
            'B' => {
                self.grid.move_down(param(0, 1));
            }
            // Cursor Forward (CUF)
            'C' => {
                self.grid.move_right(param(0, 1));
            }
            // Cursor Back (CUB)
            'D' => {
                self.grid.move_left(param(0, 1));
            }
            // Cursor Next Line (CNL)
            'E' => {
                self.grid.move_down(param(0, 1));
                self.grid.carriage_return();
            }
            // Cursor Previous Line (CPL)
            'F' => {
                self.grid.move_up(param(0, 1));
                self.grid.carriage_return();
            }
            // Cursor Horizontal Absolute (CHA)
            'G' => {
                self.grid.goto_col(param(0, 1));
            }
            // Cursor Position (CUP) / Horizontal and Vertical Position (HVP)
            'H' | 'f' => {
                self.grid.goto(param(0, 1), param(1, 1));
            }
            // Erase in Display (ED)
            'J' => {
                self.grid.erase_display(param(0, 0));
            }
            // Erase in Line (EL)
            'K' => {
                self.grid.erase_line(param(0, 0));
            }
            // Insert Lines (IL)
            'L' => {
                self.grid.insert_lines(param(0, 1));
            }
            // Delete Lines (DL)
            'M' => {
                self.grid.delete_lines(param(0, 1));
            }
            // Delete Characters (DCH)
            'P' => {
                self.grid.delete_chars(param(0, 1));
            }
            // Scroll Up (SU)
            'S' => {
                self.grid.scroll_up(param(0, 1));
            }
            // Scroll Down (SD)
            'T' => {
                self.grid.scroll_down(param(0, 1));
            }
            // Erase Characters (ECH)
            'X' => {
                self.grid.erase_chars(param(0, 1));
            }
            // Cursor Backward Tabulation (CBT)
            'Z' => {
                // Move backward n tab stops
                let n = param(0, 1);
                for _ in 0..n {
                    self.grid.move_left(8); // Approximate
                }
            }
            // Insert Characters (ICH)
            '@' => {
                self.grid.insert_chars(param(0, 1));
            }
            // Cursor Vertical Absolute (VPA)
            'd' => {
                self.grid.goto_row(param(0, 1));
            }
            // DEC Private Mode Set/Reset
            'h' => {
                if is_dec_private {
                    self.handle_dec_mode(param(0, 0), true);
                } else {
                    // Standard mode set
                    debug!("SM mode: {}", param(0, 0));
                }
            }
            'l' => {
                if is_dec_private {
                    self.handle_dec_mode(param(0, 0), false);
                } else {
                    // Standard mode reset
                    debug!("RM mode: {}", param(0, 0));
                }
            }
            // Select Graphic Rendition (SGR)
            'm' => {
                if params.is_empty() {
                    self.grid.reset_attrs();
                } else {
                    self.process_sgr(&params);
                }
            }
            // Device Status Report (DSR)
            'n' => {
                debug!("DSR request: {}", param(0, 0));
            }
            // Set Scroll Region (DECSTBM)
            'r' => {
                let (rows, _) = self.grid.size();
                let top = param(0, 1);
                let bottom = param(1, rows);
                self.grid.set_scroll_region(top, bottom);
            }
            // Save Cursor (DECSC) - via CSI
            's' => {
                self.grid.save_cursor();
            }
            // Restore Cursor (DECRC) - via CSI
            'u' => {
                self.grid.restore_cursor();
            }
            _ => {
                debug!(
                    "Unknown CSI: {:?} {:?} {} '{}'",
                    params, intermediates, _ignore, action
                );
            }
        }
    }

    /// Process an ESC sequence.
    fn esc_dispatch(&mut self, intermediates: &[u8], _ignore: bool, byte: u8) {
        match (intermediates.first(), byte) {
            // Save Cursor (DECSC)
            (None, b'7') => {
                self.grid.save_cursor();
            }
            // Restore Cursor (DECRC)
            (None, b'8') => {
                self.grid.restore_cursor();
            }
            // Reverse Index (RI)
            (None, b'M') => {
                self.grid.reverse_index();
            }
            // Next Line (NEL)
            (None, b'E') => {
                self.grid.carriage_return();
                self.grid.linefeed();
            }
            // Index (IND)
            (None, b'D') => {
                self.grid.linefeed();
            }
            // Reset to Initial State (RIS)
            (None, b'c') => {
                let (rows, cols) = self.grid.size();
                self.grid = TerminalGrid::new(rows, cols);
            }
            // Character set designation (ignored)
            (Some(b'('), _) | (Some(b')'), _) | (Some(b'*'), _) | (Some(b'+'), _) => {
                debug!("Character set designation (ignored)");
            }
            _ => {
                debug!(
                    "Unknown ESC: intermediates={:?} byte=0x{:02x}",
                    intermediates, byte
                );
            }
        }
    }

    /// Process an OSC (Operating System Command) sequence.
    fn osc_dispatch(&mut self, params: &[&[u8]], bell_terminated: bool) {
        if params.is_empty() {
            return;
        }

        // First param is the OSC command number
        let cmd = match std::str::from_utf8(params[0]) {
            Ok(s) => s.parse::<u16>().unwrap_or(0),
            Err(_) => return,
        };

        match cmd {
            // Set window title
            0 | 2 => {
                if let Some(title) = params.get(1) {
                    if let Ok(title) = std::str::from_utf8(title) {
                        self.window_title = title.to_string();
                    }
                }
            }
            // Set icon name (ignored)
            1 => {}
            // Set/query colors (ignored)
            4 | 10..=17 | 104 | 110..=117 => {
                debug!("OSC color command: {} (ignored)", cmd);
            }
            _ => {
                debug!(
                    "Unknown OSC: cmd={} params={:?} bell={}",
                    cmd, params, bell_terminated
                );
            }
        }
    }

    /// Hook for DCS sequences (ignored).
    fn hook(&mut self, _params: &vte::Params, _intermediates: &[u8], _ignore: bool, _action: char) {
        debug!("DCS hook");
    }

    /// Put data for DCS sequences (ignored).
    fn put(&mut self, _byte: u8) {}

    /// Unhook DCS sequences (ignored).
    fn unhook(&mut self) {
        debug!("DCS unhook");
    }
}

impl TerminalHandler {
    /// Handle DEC private mode set/reset.
    fn handle_dec_mode(&mut self, mode: u16, enable: bool) {
        match mode {
            // Cursor keys mode (DECCKM)
            1 => {
                debug!("DECCKM: {} (application cursor keys)", enable);
            }
            // Origin mode (DECOM)
            6 => {
                self.grid.set_origin_mode(enable);
            }
            // Auto-wrap mode (DECAWM)
            7 => {
                self.grid.set_auto_wrap(enable);
            }
            // Show/hide cursor (DECTCEM)
            25 => {
                self.grid.set_cursor_visible(enable);
            }
            // Alternate screen buffer
            47 | 1047 => {
                if enable {
                    self.grid.enter_alt_screen();
                } else {
                    self.grid.exit_alt_screen();
                }
            }
            // Alternate screen buffer with cursor save
            1049 => {
                if enable {
                    self.grid.save_cursor();
                    self.grid.enter_alt_screen();
                    self.grid.erase_display(2);
                } else {
                    self.grid.exit_alt_screen();
                    self.grid.restore_cursor();
                }
            }
            // Bracketed paste mode (ignored for now)
            2004 => {
                debug!("Bracketed paste mode: {}", enable);
            }
            _ => {
                debug!("Unknown DEC mode: {} = {}", mode, enable);
            }
        }
    }
}
