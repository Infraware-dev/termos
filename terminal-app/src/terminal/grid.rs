//! Terminal grid for storing and manipulating terminal state.

use super::cell::{Cell, CellAttrs, Color, NamedColor};

/// Maximum lines in scrollback buffer
const MAX_SCROLLBACK: usize = 10_000;

/// Terminal grid containing cells, cursor position, and state.
#[derive(Debug)]
pub struct TerminalGrid {
    /// Grid of cells [row][col] - visible screen only.
    cells: Vec<Vec<Cell>>,
    /// Scrollback buffer - lines that scrolled off the top.
    scrollback: Vec<Vec<Cell>>,
    /// Current scroll offset (0 = bottom/live, >0 = scrolled up).
    scroll_offset: usize,
    /// Cursor row (0-indexed).
    cursor_row: u16,
    /// Cursor column (0-indexed).
    cursor_col: u16,
    /// Cursor visibility.
    cursor_visible: bool,
    /// Scroll region top (inclusive).
    scroll_top: u16,
    /// Scroll region bottom (inclusive).
    scroll_bottom: u16,
    /// Current cell attributes for new characters.
    current_attrs: CellAttrs,
    /// Current foreground color.
    current_fg: Color,
    /// Current background color.
    current_bg: Color,
    /// Alternate screen buffer (for vim, less, etc.).
    alt_screen: Option<AltScreenState>,
    /// Grid rows.
    rows: u16,
    /// Grid columns.
    cols: u16,
    /// Tab stop positions (every 8 columns by default).
    tab_stops: Vec<u16>,
    /// Origin mode (cursor relative to scroll region).
    origin_mode: bool,
    /// Auto-wrap mode.
    auto_wrap: bool,
    /// Cursor needs wrap on next character.
    wrap_pending: bool,
    /// Saved cursor state.
    saved_cursor: Option<SavedCursor>,
}

/// Saved cursor state for DECSC/DECRC.
#[derive(Debug, Clone)]
struct SavedCursor {
    row: u16,
    col: u16,
    attrs: CellAttrs,
    fg: Color,
    bg: Color,
    origin_mode: bool,
    auto_wrap: bool,
}

/// Alternate screen state (saves main screen).
#[derive(Debug)]
struct AltScreenState {
    cells: Vec<Vec<Cell>>,
    cursor_row: u16,
    cursor_col: u16,
    saved_cursor: Option<SavedCursor>,
}

impl TerminalGrid {
    /// Create a new terminal grid with given dimensions.
    #[must_use]
    pub fn new(rows: u16, cols: u16) -> Self {
        let cells = Self::create_empty_grid(rows, cols);
        let tab_stops = (0..cols).filter(|c| c % 8 == 0).collect();

        Self {
            cells,
            scrollback: Vec::new(),
            scroll_offset: 0,
            cursor_row: 0,
            cursor_col: 0,
            cursor_visible: true,
            scroll_top: 0,
            scroll_bottom: rows.saturating_sub(1),
            current_attrs: CellAttrs::default(),
            current_fg: Color::Named(NamedColor::Foreground),
            current_bg: Color::Named(NamedColor::Background),
            alt_screen: None,
            rows,
            cols,
            tab_stops,
            origin_mode: false,
            auto_wrap: true,
            wrap_pending: false,
            saved_cursor: None,
        }
    }

    /// Create empty grid filled with default cells.
    fn create_empty_grid(rows: u16, cols: u16) -> Vec<Vec<Cell>> {
        (0..rows)
            .map(|_| (0..cols).map(|_| Cell::default()).collect())
            .collect()
    }

    /// Get grid dimensions.
    #[must_use]
    pub fn size(&self) -> (u16, u16) {
        (self.rows, self.cols)
    }

    /// Get cursor position (row, col).
    #[must_use]
    pub fn cursor_position(&self) -> (u16, u16) {
        (self.cursor_row, self.cursor_col)
    }

    /// Get cursor visibility.
    #[must_use]
    pub fn cursor_visible(&self) -> bool {
        self.cursor_visible
    }

    /// Set cursor visibility.
    pub fn set_cursor_visible(&mut self, visible: bool) {
        self.cursor_visible = visible;
    }

    /// Get cells for rendering (current screen only).
    pub fn cells(&self) -> &[Vec<Cell>] {
        &self.cells
    }

    /// Get scrollback buffer.
    pub fn scrollback(&self) -> &[Vec<Cell>] {
        &self.scrollback
    }

    /// Get current scroll offset (0 = live view, >0 = scrolled up).
    pub fn scroll_offset(&self) -> usize {
        self.scroll_offset
    }

    /// Get total scrollable lines (scrollback + visible).
    pub fn total_lines(&self) -> usize {
        self.scrollback.len() + self.cells.len()
    }

    /// Maximum scroll offset.
    pub fn max_scroll_offset(&self) -> usize {
        self.scrollback.len()
    }

    /// Scroll up by n lines (into scrollback).
    pub fn scroll_view_up(&mut self, n: usize) {
        self.scroll_offset = (self.scroll_offset + n).min(self.max_scroll_offset());
    }

    /// Scroll down by n lines (towards live view).
    pub fn scroll_view_down(&mut self, n: usize) {
        self.scroll_offset = self.scroll_offset.saturating_sub(n);
    }

    /// Scroll to bottom (live view).
    pub fn scroll_to_bottom(&mut self) {
        self.scroll_offset = 0;
    }

    /// Check if at bottom (live view).
    pub fn is_at_bottom(&self) -> bool {
        self.scroll_offset == 0
    }

    /// Get visible rows for rendering, combining scrollback and current screen.
    /// Returns rows from (scrollback + cells) based on scroll_offset.
    pub fn visible_rows(&self) -> Vec<&[Cell]> {
        let total = self.scrollback.len() + self.cells.len();
        let visible_count = self.rows as usize;

        // Calculate start position in combined buffer
        let end = total.saturating_sub(self.scroll_offset);
        let start = end.saturating_sub(visible_count);

        let mut result = Vec::with_capacity(visible_count);

        for i in start..end {
            if i < self.scrollback.len() {
                result.push(self.scrollback[i].as_slice());
            } else {
                let screen_idx = i - self.scrollback.len();
                if screen_idx < self.cells.len() {
                    result.push(self.cells[screen_idx].as_slice());
                }
            }
        }

        result
    }

    /// Check if alternate screen is active.
    #[must_use]
    pub fn is_alt_screen(&self) -> bool {
        self.alt_screen.is_some()
    }

    // ========== Character Output ==========

    /// Put a character at the current cursor position.
    pub fn put_char(&mut self, c: char) {
        // Handle wrap pending state
        if self.wrap_pending {
            self.wrap_pending = false;
            self.cursor_col = 0;
            self.linefeed();
        }

        // Put character at cursor position
        if let Some(row) = self.cells.get_mut(self.cursor_row as usize) {
            if let Some(cell) = row.get_mut(self.cursor_col as usize) {
                cell.ch = c;
                cell.fg = self.current_fg;
                cell.bg = self.current_bg;
                cell.attrs = self.current_attrs;
            }
        }

        // Advance cursor
        if self.cursor_col < self.cols.saturating_sub(1) {
            self.cursor_col += 1;
        } else if self.auto_wrap {
            self.wrap_pending = true;
        }
    }

    // ========== Cursor Movement ==========

    /// Move cursor to absolute position (1-indexed from terminal, convert to 0-indexed).
    pub fn goto(&mut self, row: u16, col: u16) {
        self.wrap_pending = false;
        let (min_row, max_row) = if self.origin_mode {
            (self.scroll_top, self.scroll_bottom)
        } else {
            (0, self.rows.saturating_sub(1))
        };

        let row = if self.origin_mode {
            self.scroll_top + row.saturating_sub(1)
        } else {
            row.saturating_sub(1)
        };

        self.cursor_row = row.clamp(min_row, max_row);
        self.cursor_col = col.saturating_sub(1).min(self.cols.saturating_sub(1));
    }

    /// Move cursor to column (1-indexed).
    pub fn goto_col(&mut self, col: u16) {
        self.wrap_pending = false;
        self.cursor_col = col.saturating_sub(1).min(self.cols.saturating_sub(1));
    }

    /// Move cursor to row (1-indexed).
    pub fn goto_row(&mut self, row: u16) {
        self.wrap_pending = false;
        self.cursor_row = row.saturating_sub(1).min(self.rows.saturating_sub(1));
    }

    /// Move cursor up by n rows.
    pub fn move_up(&mut self, n: u16) {
        self.wrap_pending = false;
        let min = if self.origin_mode { self.scroll_top } else { 0 };
        self.cursor_row = self.cursor_row.saturating_sub(n).max(min);
    }

    /// Move cursor down by n rows.
    pub fn move_down(&mut self, n: u16) {
        self.wrap_pending = false;
        let max = if self.origin_mode {
            self.scroll_bottom
        } else {
            self.rows.saturating_sub(1)
        };
        self.cursor_row = (self.cursor_row + n).min(max);
    }

    /// Move cursor right by n columns.
    pub fn move_right(&mut self, n: u16) {
        self.wrap_pending = false;
        self.cursor_col = (self.cursor_col + n).min(self.cols.saturating_sub(1));
    }

    /// Move cursor left by n columns.
    pub fn move_left(&mut self, n: u16) {
        self.wrap_pending = false;
        self.cursor_col = self.cursor_col.saturating_sub(n);
    }

    // ========== Line Operations ==========

    /// Carriage return - move cursor to column 0.
    pub fn carriage_return(&mut self) {
        self.wrap_pending = false;
        self.cursor_col = 0;
    }

    /// Line feed - move cursor down, scroll if at bottom of scroll region.
    pub fn linefeed(&mut self) {
        self.wrap_pending = false;
        if self.cursor_row == self.scroll_bottom {
            self.scroll_up(1);
        } else if self.cursor_row < self.rows.saturating_sub(1) {
            self.cursor_row += 1;
        }
    }

    /// Reverse index - move cursor up, scroll down if at top of scroll region.
    pub fn reverse_index(&mut self) {
        self.wrap_pending = false;
        if self.cursor_row == self.scroll_top {
            self.scroll_down(1);
        } else if self.cursor_row > 0 {
            self.cursor_row -= 1;
        }
    }

    /// Tab - move to next tab stop.
    pub fn tab(&mut self) {
        self.wrap_pending = false;
        let next_tab = self
            .tab_stops
            .iter()
            .find(|&&t| t > self.cursor_col)
            .copied()
            .unwrap_or(self.cols.saturating_sub(1));
        self.cursor_col = next_tab.min(self.cols.saturating_sub(1));
    }

    /// Backspace - move cursor left by 1.
    pub fn backspace(&mut self) {
        self.wrap_pending = false;
        if self.cursor_col > 0 {
            self.cursor_col -= 1;
        }
    }

    // ========== Scrolling ==========

    /// Scroll the scroll region up by n lines.
    pub fn scroll_up(&mut self, n: u16) {
        let top = self.scroll_top as usize;
        let bottom = self.scroll_bottom as usize;

        for _ in 0..n {
            if top < bottom && bottom < self.cells.len() {
                // Save the line going off the top to scrollback (only if scroll region is full screen)
                if top == 0 {
                    let removed_line = self.cells.remove(top);
                    self.scrollback.push(removed_line);
                    // Trim scrollback if too large
                    if self.scrollback.len() > MAX_SCROLLBACK {
                        self.scrollback.remove(0);
                    }
                } else {
                    self.cells.remove(top);
                }
                let new_row = (0..self.cols).map(|_| Cell::default()).collect();
                self.cells.insert(bottom, new_row);
            }
        }
    }

    /// Scroll the scroll region down by n lines.
    pub fn scroll_down(&mut self, n: u16) {
        let top = self.scroll_top as usize;
        let bottom = self.scroll_bottom as usize;

        for _ in 0..n {
            if top < bottom && bottom < self.cells.len() {
                self.cells.remove(bottom);
                let new_row = (0..self.cols).map(|_| Cell::default()).collect();
                self.cells.insert(top, new_row);
            }
        }
    }

    /// Set scroll region (1-indexed).
    pub fn set_scroll_region(&mut self, top: u16, bottom: u16) {
        let top = top.saturating_sub(1).min(self.rows.saturating_sub(1));
        let bottom = bottom.saturating_sub(1).min(self.rows.saturating_sub(1));

        if top < bottom {
            self.scroll_top = top;
            self.scroll_bottom = bottom;
        }

        // Reset cursor to origin per DEC behavior
        if self.origin_mode {
            self.cursor_row = self.scroll_top;
        } else {
            self.cursor_row = 0;
        }
        self.cursor_col = 0;
    }

    // ========== Erase Operations ==========

    /// Erase display (ED).
    /// mode: 0 = cursor to end, 1 = start to cursor, 2 = entire screen, 3 = scrollback
    pub fn erase_display(&mut self, mode: u16) {
        match mode {
            0 => {
                // Erase from cursor to end
                self.erase_line(0);
                for row in (self.cursor_row + 1) as usize..self.rows as usize {
                    if let Some(r) = self.cells.get_mut(row) {
                        for cell in r.iter_mut() {
                            cell.reset();
                        }
                    }
                }
            }
            1 => {
                // Erase from start to cursor
                for row in 0..self.cursor_row as usize {
                    if let Some(r) = self.cells.get_mut(row) {
                        for cell in r.iter_mut() {
                            cell.reset();
                        }
                    }
                }
                self.erase_line(1);
            }
            2 | 3 => {
                // Erase entire screen
                for row in &mut self.cells {
                    for cell in row.iter_mut() {
                        cell.reset();
                    }
                }
            }
            _ => {}
        }
    }

    /// Erase line (EL).
    /// mode: 0 = cursor to end, 1 = start to cursor, 2 = entire line
    pub fn erase_line(&mut self, mode: u16) {
        if let Some(row) = self.cells.get_mut(self.cursor_row as usize) {
            let (start, end) = match mode {
                0 => (self.cursor_col as usize, self.cols as usize),
                1 => (0, self.cursor_col as usize + 1),
                2 => (0, self.cols as usize),
                _ => return,
            };

            for col in start..end.min(row.len()) {
                if let Some(cell) = row.get_mut(col) {
                    cell.reset();
                }
            }
        }
    }

    /// Erase characters at cursor (ECH).
    pub fn erase_chars(&mut self, n: u16) {
        if let Some(row) = self.cells.get_mut(self.cursor_row as usize) {
            let start = self.cursor_col as usize;
            let end = (start + n as usize).min(self.cols as usize);

            for col in start..end {
                if let Some(cell) = row.get_mut(col) {
                    cell.reset();
                }
            }
        }
    }

    /// Delete characters at cursor, shift rest left (DCH).
    pub fn delete_chars(&mut self, n: u16) {
        if let Some(row) = self.cells.get_mut(self.cursor_row as usize) {
            let col = self.cursor_col as usize;
            let n = n as usize;

            for _ in 0..n.min(row.len().saturating_sub(col)) {
                if col < row.len() {
                    row.remove(col);
                    row.push(Cell::default());
                }
            }
        }
    }

    /// Insert blank characters at cursor, shift rest right (ICH).
    pub fn insert_chars(&mut self, n: u16) {
        if let Some(row) = self.cells.get_mut(self.cursor_row as usize) {
            let col = self.cursor_col as usize;

            for _ in 0..n.min(self.cols) {
                if col < row.len() {
                    row.insert(col, Cell::default());
                    row.pop();
                }
            }
        }
    }

    /// Insert lines at cursor, shift rest down (IL).
    pub fn insert_lines(&mut self, n: u16) {
        let row = self.cursor_row as usize;
        let bottom = self.scroll_bottom as usize;

        if row <= bottom {
            for _ in 0..n.min(self.rows) {
                if bottom < self.cells.len() {
                    self.cells.remove(bottom);
                }
                let new_row = (0..self.cols).map(|_| Cell::default()).collect();
                self.cells.insert(row, new_row);
            }
        }
    }

    /// Delete lines at cursor, shift rest up (DL).
    pub fn delete_lines(&mut self, n: u16) {
        let row = self.cursor_row as usize;
        let bottom = self.scroll_bottom as usize;

        if row <= bottom {
            for _ in 0..n.min(self.rows) {
                if row < self.cells.len() {
                    self.cells.remove(row);
                }
                let new_row = (0..self.cols).map(|_| Cell::default()).collect();
                if bottom <= self.cells.len() {
                    self.cells.insert(bottom, new_row);
                } else {
                    self.cells.push(new_row);
                }
            }
        }
    }

    // ========== Attributes ==========

    /// Reset all attributes to default.
    pub fn reset_attrs(&mut self) {
        self.current_attrs.reset();
        self.current_fg = Color::Named(NamedColor::Foreground);
        self.current_bg = Color::Named(NamedColor::Background);
    }

    /// Set bold attribute.
    pub fn set_bold(&mut self, on: bool) {
        self.current_attrs.bold = on;
    }

    /// Set italic attribute.
    pub fn set_italic(&mut self, on: bool) {
        self.current_attrs.italic = on;
    }

    /// Set underline attribute.
    pub fn set_underline(&mut self, on: bool) {
        self.current_attrs.underline = on;
    }

    /// Set dim attribute.
    pub fn set_dim(&mut self, on: bool) {
        self.current_attrs.dim = on;
    }

    /// Set reverse attribute.
    pub fn set_reverse(&mut self, on: bool) {
        self.current_attrs.reverse = on;
    }

    /// Set hidden attribute.
    pub fn set_hidden(&mut self, on: bool) {
        self.current_attrs.hidden = on;
    }

    /// Set strikethrough attribute.
    pub fn set_strikethrough(&mut self, on: bool) {
        self.current_attrs.strikethrough = on;
    }

    /// Set foreground color.
    pub fn set_fg(&mut self, color: Color) {
        self.current_fg = color;
    }

    /// Set background color.
    pub fn set_bg(&mut self, color: Color) {
        self.current_bg = color;
    }

    // ========== Alternate Screen ==========

    /// Enter alternate screen buffer (DECSET 1049).
    pub fn enter_alt_screen(&mut self) {
        if self.alt_screen.is_some() {
            return; // Already in alt screen
        }

        // Save current state
        let alt_state = AltScreenState {
            cells: std::mem::replace(&mut self.cells, Self::create_empty_grid(self.rows, self.cols)),
            cursor_row: self.cursor_row,
            cursor_col: self.cursor_col,
            saved_cursor: self.saved_cursor.take(),
        };
        self.alt_screen = Some(alt_state);

        // Reset cursor for alt screen
        self.cursor_row = 0;
        self.cursor_col = 0;
    }

    /// Exit alternate screen buffer (DECRST 1049).
    pub fn exit_alt_screen(&mut self) {
        if let Some(alt_state) = self.alt_screen.take() {
            self.cells = alt_state.cells;
            self.cursor_row = alt_state.cursor_row;
            self.cursor_col = alt_state.cursor_col;
            self.saved_cursor = alt_state.saved_cursor;
        }
    }

    // ========== Cursor Save/Restore ==========

    /// Save cursor position and attributes (DECSC).
    pub fn save_cursor(&mut self) {
        self.saved_cursor = Some(SavedCursor {
            row: self.cursor_row,
            col: self.cursor_col,
            attrs: self.current_attrs,
            fg: self.current_fg,
            bg: self.current_bg,
            origin_mode: self.origin_mode,
            auto_wrap: self.auto_wrap,
        });
    }

    /// Restore cursor position and attributes (DECRC).
    pub fn restore_cursor(&mut self) {
        if let Some(saved) = self.saved_cursor.take() {
            self.cursor_row = saved.row.min(self.rows.saturating_sub(1));
            self.cursor_col = saved.col.min(self.cols.saturating_sub(1));
            self.current_attrs = saved.attrs;
            self.current_fg = saved.fg;
            self.current_bg = saved.bg;
            self.origin_mode = saved.origin_mode;
            self.auto_wrap = saved.auto_wrap;
        }
    }

    // ========== Mode Settings ==========

    /// Set origin mode (DECOM).
    pub fn set_origin_mode(&mut self, on: bool) {
        self.origin_mode = on;
        // Move cursor to origin when mode changes
        if on {
            self.cursor_row = self.scroll_top;
        } else {
            self.cursor_row = 0;
        }
        self.cursor_col = 0;
    }

    /// Set auto-wrap mode (DECAWM).
    pub fn set_auto_wrap(&mut self, on: bool) {
        self.auto_wrap = on;
    }

    // ========== Resize ==========

    /// Resize the terminal grid.
    pub fn resize(&mut self, rows: u16, cols: u16) {
        if rows == self.rows && cols == self.cols {
            return;
        }

        // Create new grid
        let mut new_cells = Self::create_empty_grid(rows, cols);

        // Copy existing content
        for (r, row) in self.cells.iter().enumerate() {
            if r >= rows as usize {
                break;
            }
            for (c, cell) in row.iter().enumerate() {
                if c >= cols as usize {
                    break;
                }
                new_cells[r][c] = cell.clone();
            }
        }

        self.cells = new_cells;
        self.rows = rows;
        self.cols = cols;

        // Adjust cursor if needed
        self.cursor_row = self.cursor_row.min(rows.saturating_sub(1));
        self.cursor_col = self.cursor_col.min(cols.saturating_sub(1));

        // Adjust scroll region
        self.scroll_bottom = rows.saturating_sub(1);
        if self.scroll_top > self.scroll_bottom {
            self.scroll_top = 0;
        }

        // Update tab stops
        self.tab_stops = (0..cols).filter(|c| c % 8 == 0).collect();

        // Resize alt screen if active
        if let Some(ref mut alt) = self.alt_screen {
            let mut new_alt_cells = Self::create_empty_grid(rows, cols);
            for (r, row) in alt.cells.iter().enumerate() {
                if r >= rows as usize {
                    break;
                }
                for (c, cell) in row.iter().enumerate() {
                    if c >= cols as usize {
                        break;
                    }
                    new_alt_cells[r][c] = cell.clone();
                }
            }
            alt.cells = new_alt_cells;
            alt.cursor_row = alt.cursor_row.min(rows.saturating_sub(1));
            alt.cursor_col = alt.cursor_col.min(cols.saturating_sub(1));
        }
    }
}
