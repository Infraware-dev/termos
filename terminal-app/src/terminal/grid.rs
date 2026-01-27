//! Terminal grid for storing and manipulating terminal state.

use super::cell::{Cell, CellAttrs, Color, NamedColor};
use std::collections::VecDeque;

/// Maximum lines in scrollback buffer
const MAX_SCROLLBACK: usize = 10_000;

/// Terminal grid containing cells, cursor position, and state.
/// Uses a ring buffer for O(1) scrolling operations.
#[derive(Debug)]
pub struct TerminalGrid {
    /// Grid of cells [row][col] - visible screen only.
    /// Accessed via ring buffer with cells_offset for O(1) scroll.
    cells: Vec<Vec<Cell>>,
    /// Ring buffer offset - physical index of logical row 0.
    /// Used to avoid O(n) remove/insert during scroll.
    cells_offset: usize,
    /// Scrollback buffer - lines that scrolled off the top.
    /// Uses VecDeque for O(1) pop_front when trimming.
    scrollback: VecDeque<Vec<Cell>>,
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
    cells_offset: usize,
    cursor_row: u16,
    cursor_col: u16,
    saved_cursor: Option<SavedCursor>,
}

#[allow(dead_code)]
impl TerminalGrid {
    /// Create a new terminal grid with given dimensions.
    #[must_use]
    pub fn new(rows: u16, cols: u16) -> Self {
        let cells = Self::create_empty_grid(rows, cols);
        let tab_stops = (0..cols).filter(|c| c % 8 == 0).collect();

        Self {
            cells,
            cells_offset: 0,
            scrollback: VecDeque::new(),
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
        let cols_usize = cols as usize;
        (0..rows)
            .map(|_| vec![Cell::default(); cols_usize])
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
    /// Note: Returns physical array order, use visible_rows() for logical order.
    pub fn cells(&self) -> &[Vec<Cell>] {
        &self.cells
    }

    // ========== Ring Buffer Helpers ==========

    /// Convert logical row index to physical index in the ring buffer.
    #[inline]
    fn physical_row(&self, logical_row: usize) -> usize {
        (self.cells_offset + logical_row) % self.cells.len()
    }

    /// Get mutable reference to a row by logical index.
    #[inline]
    fn row_mut(&mut self, logical_row: usize) -> Option<&mut Vec<Cell>> {
        let physical = self.physical_row(logical_row);
        self.cells.get_mut(physical)
    }

    /// Get reference to a row by logical index.
    #[inline]
    fn row(&self, logical_row: usize) -> Option<&Vec<Cell>> {
        let physical = self.physical_row(logical_row);
        self.cells.get(physical)
    }

    /// Get scrollback buffer.
    pub fn scrollback(&self) -> &VecDeque<Vec<Cell>> {
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

    /// Scroll to specific absolute offset.
    pub fn scroll_to_offset(&mut self, offset: usize) {
        self.scroll_offset = offset.min(self.max_scroll_offset());
    }

    /// Check if at bottom (live view).
    pub fn is_at_bottom(&self) -> bool {
        self.scroll_offset == 0
    }

    /// Get visible rows for rendering, combining scrollback and current screen.
    /// Returns rows from (scrollback + cells) based on scroll_offset.
    /// Uses ring buffer for correct logical row ordering.
    ///
    /// DEPRECATED: Use `visible_row()` and `visible_row_count()` for zero-allocation access.
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
                // Access cells via ring buffer (logical to physical mapping)
                let logical_idx = i - self.scrollback.len();
                if let Some(row) = self.row(logical_idx) {
                    result.push(row.as_slice());
                }
            }
        }

        result
    }

    /// Get the number of visible rows (for zero-allocation iteration).
    #[inline]
    #[must_use]
    pub fn visible_row_count(&self) -> usize {
        let total = self.scrollback.len() + self.cells.len();
        let visible_count = self.rows as usize;
        let end = total.saturating_sub(self.scroll_offset);
        let start = end.saturating_sub(visible_count);
        end - start
    }

    /// Get a single visible row by index (for zero-allocation iteration).
    /// Index 0 is the topmost visible row.
    ///
    /// NOTE: For iterating all visible rows, prefer `visible_rows_iter()` which
    /// calculates bounds once instead of on every call.
    #[inline]
    #[must_use]
    pub fn visible_row(&self, visible_idx: usize) -> Option<&[Cell]> {
        let total = self.scrollback.len() + self.cells.len();
        let visible_count = self.rows as usize;
        let end = total.saturating_sub(self.scroll_offset);
        let start = end.saturating_sub(visible_count);

        let absolute_idx = start + visible_idx;
        if absolute_idx >= end {
            return None;
        }

        if absolute_idx < self.scrollback.len() {
            Some(self.scrollback[absolute_idx].as_slice())
        } else {
            let logical_idx = absolute_idx - self.scrollback.len();
            self.row(logical_idx).map(|r| r.as_slice())
        }
    }

    /// Create an iterator over visible rows (zero-allocation, calculates bounds once).
    ///
    /// This is more efficient than calling `visible_row(idx)` in a loop because
    /// it calculates `start`, `end`, and `scrollback.len()` only once.
    #[inline]
    #[must_use]
    pub fn visible_rows_iter(&self) -> VisibleRowsIter<'_> {
        let total = self.scrollback.len() + self.cells.len();
        let visible_count = self.rows as usize;
        let end = total.saturating_sub(self.scroll_offset);
        let start = end.saturating_sub(visible_count);

        VisibleRowsIter {
            grid: self,
            end,
            scrollback_len: self.scrollback.len(),
            current: start,
        }
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

        // Put character at cursor position (via ring buffer)
        // Copy values before mutable borrow to satisfy borrow checker
        let cursor_row = self.cursor_row as usize;
        let cursor_col = self.cursor_col as usize;
        let fg = self.current_fg;
        let bg = self.current_bg;
        let attrs = self.current_attrs;
        if let Some(row) = self.row_mut(cursor_row)
            && let Some(cell) = row.get_mut(cursor_col)
        {
            cell.ch = c;
            cell.fg = fg;
            cell.bg = bg;
            cell.attrs = attrs;
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
    /// Uses O(1) ring buffer rotation for full-screen scroll.
    pub fn scroll_up(&mut self, n: u16) {
        let top = self.scroll_top as usize;
        let bottom = self.scroll_bottom as usize;
        let is_full_screen = top == 0 && bottom == self.rows.saturating_sub(1) as usize;

        for _ in 0..n {
            if top >= bottom || bottom >= self.cells.len() {
                continue;
            }

            if is_full_screen {
                // O(1) ring buffer rotation for full-screen scroll
                let physical_top = self.cells_offset;
                let cols = self.cols as usize;

                // Move top line to scrollback
                // PERFORMANCE: vec![...; n] is faster than iterator-based allocation
                let top_row =
                    std::mem::replace(&mut self.cells[physical_top], vec![Cell::default(); cols]);
                self.scrollback.push_back(top_row);

                // Trim scrollback if too large - O(1) with VecDeque
                if self.scrollback.len() > MAX_SCROLLBACK {
                    self.scrollback.pop_front();
                }

                // Rotate the ring buffer - O(1)!
                self.cells_offset = (self.cells_offset + 1) % self.cells.len();
            } else {
                // Partial scroll region (vim, less, etc.) - O(region_size)
                // Shift rows within the region
                for row in top..bottom {
                    let src_physical = self.physical_row(row + 1);
                    let dst_physical = self.physical_row(row);
                    // Swap to avoid clone, then clear the source
                    self.cells.swap(src_physical, dst_physical);
                }
                // Clear the bottom row of the region
                if let Some(row) = self.row_mut(bottom) {
                    for cell in row.iter_mut() {
                        cell.reset();
                    }
                }
            }
        }
    }

    /// Scroll the scroll region down by n lines.
    /// Uses O(1) ring buffer rotation for full-screen scroll.
    pub fn scroll_down(&mut self, n: u16) {
        let top = self.scroll_top as usize;
        let bottom = self.scroll_bottom as usize;
        let is_full_screen = top == 0 && bottom == self.rows.saturating_sub(1) as usize;

        for _ in 0..n {
            if top >= bottom || bottom >= self.cells.len() {
                continue;
            }

            if is_full_screen {
                // O(1) ring buffer rotation for full-screen scroll down
                // Decrement offset (wrapping around)
                self.cells_offset = if self.cells_offset == 0 {
                    self.cells.len() - 1
                } else {
                    self.cells_offset - 1
                };

                // Clear the new top row
                let physical_top = self.cells_offset;
                for cell in self.cells[physical_top].iter_mut() {
                    cell.reset();
                }
            } else {
                // Partial scroll region - O(region_size)
                // Shift rows within the region (from bottom to top)
                for row in (top + 1..=bottom).rev() {
                    let src_physical = self.physical_row(row - 1);
                    let dst_physical = self.physical_row(row);
                    self.cells.swap(src_physical, dst_physical);
                }
                // Clear the top row of the region
                if let Some(row) = self.row_mut(top) {
                    for cell in row.iter_mut() {
                        cell.reset();
                    }
                }
            }
        }
    }

    /// Create an empty row with default cells.
    #[inline]
    fn create_empty_row(&self) -> Vec<Cell> {
        vec![Cell::default(); self.cols as usize]
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
                // Erase from cursor to end (via ring buffer)
                self.erase_line(0);
                for row in (self.cursor_row + 1) as usize..self.rows as usize {
                    if let Some(r) = self.row_mut(row) {
                        for cell in r.iter_mut() {
                            cell.reset();
                        }
                    }
                }
            }
            1 => {
                // Erase from start to cursor (via ring buffer)
                for row in 0..self.cursor_row as usize {
                    if let Some(r) = self.row_mut(row) {
                        for cell in r.iter_mut() {
                            cell.reset();
                        }
                    }
                }
                self.erase_line(1);
            }
            2 | 3 => {
                // Erase entire screen and scrollback
                // Mode 2: erase screen (we also clear scrollback for better UX)
                // Mode 3: erase screen + scrollback (xterm extension)
                // Direct iteration is fine since we're clearing all cells
                for row in &mut self.cells {
                    for cell in row.iter_mut() {
                        cell.reset();
                    }
                }
                self.scrollback.clear();
                self.scroll_offset = 0;
                self.cells_offset = 0; // Reset ring buffer offset
            }
            _ => {}
        }
    }

    /// Erase line (EL).
    /// mode: 0 = cursor to end, 1 = start to cursor, 2 = entire line
    pub fn erase_line(&mut self, mode: u16) {
        // Copy values before mutable borrow
        let cursor_row = self.cursor_row as usize;
        let cursor_col = self.cursor_col as usize;
        let cols = self.cols as usize;
        let (start, end) = match mode {
            0 => (cursor_col, cols),
            1 => (0, cursor_col + 1),
            2 => (0, cols),
            _ => return,
        };

        if let Some(row) = self.row_mut(cursor_row) {
            for col in start..end.min(row.len()) {
                if let Some(cell) = row.get_mut(col) {
                    cell.reset();
                }
            }
        }
    }

    /// Erase characters at cursor (ECH).
    pub fn erase_chars(&mut self, n: u16) {
        // Copy values before mutable borrow
        let cursor_row = self.cursor_row as usize;
        let start = self.cursor_col as usize;
        let end = (start + n as usize).min(self.cols as usize);

        if let Some(row) = self.row_mut(cursor_row) {
            for col in start..end {
                if let Some(cell) = row.get_mut(col) {
                    cell.reset();
                }
            }
        }
    }

    /// Delete characters at cursor, shift rest left (DCH).
    pub fn delete_chars(&mut self, n: u16) {
        // Copy values before mutable borrow
        let cursor_row = self.cursor_row as usize;
        let col = self.cursor_col as usize;
        let n = n as usize;

        if let Some(row) = self.row_mut(cursor_row) {
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
        // Copy values before mutable borrow
        let cursor_row = self.cursor_row as usize;
        let col = self.cursor_col as usize;
        let max_insert = self.cols;

        if let Some(row) = self.row_mut(cursor_row) {
            for _ in 0..n.min(max_insert) {
                if col < row.len() {
                    row.insert(col, Cell::default());
                    row.pop();
                }
            }
        }
    }

    /// Insert lines at cursor, shift rest down (IL).
    /// Uses ring buffer-aware swapping instead of remove/insert.
    pub fn insert_lines(&mut self, n: u16) {
        let cursor = self.cursor_row as usize;
        let bottom = self.scroll_bottom as usize;

        if cursor > bottom {
            return;
        }

        for _ in 0..n.min(self.rows) {
            // Shift rows down within region (from bottom towards cursor)
            for r in (cursor + 1..=bottom).rev() {
                let src_physical = self.physical_row(r - 1);
                let dst_physical = self.physical_row(r);
                self.cells.swap(src_physical, dst_physical);
            }
            // Clear the row at cursor position
            if let Some(row) = self.row_mut(cursor) {
                for cell in row.iter_mut() {
                    cell.reset();
                }
            }
        }
    }

    /// Delete lines at cursor, shift rest up (DL).
    /// Uses ring buffer-aware swapping instead of remove/insert.
    pub fn delete_lines(&mut self, n: u16) {
        let cursor = self.cursor_row as usize;
        let bottom = self.scroll_bottom as usize;

        if cursor > bottom {
            return;
        }

        for _ in 0..n.min(self.rows) {
            // Shift rows up within region (from cursor towards bottom)
            for r in cursor..bottom {
                let src_physical = self.physical_row(r + 1);
                let dst_physical = self.physical_row(r);
                self.cells.swap(src_physical, dst_physical);
            }
            // Clear the bottom row
            if let Some(row) = self.row_mut(bottom) {
                for cell in row.iter_mut() {
                    cell.reset();
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
        self.current_attrs.set_bold(on);
    }

    /// Set italic attribute.
    pub fn set_italic(&mut self, on: bool) {
        self.current_attrs.set_italic(on);
    }

    /// Set underline attribute.
    pub fn set_underline(&mut self, on: bool) {
        self.current_attrs.set_underline(on);
    }

    /// Set dim attribute.
    pub fn set_dim(&mut self, on: bool) {
        self.current_attrs.set_dim(on);
    }

    /// Set reverse attribute.
    pub fn set_reverse(&mut self, on: bool) {
        self.current_attrs.set_reverse(on);
    }

    /// Set hidden attribute.
    pub fn set_hidden(&mut self, on: bool) {
        self.current_attrs.set_hidden(on);
    }

    /// Set strikethrough attribute.
    pub fn set_strikethrough(&mut self, on: bool) {
        self.current_attrs.set_strikethrough(on);
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

        // Save current state including ring buffer offset
        let alt_state = AltScreenState {
            cells: std::mem::replace(
                &mut self.cells,
                Self::create_empty_grid(self.rows, self.cols),
            ),
            cells_offset: self.cells_offset,
            cursor_row: self.cursor_row,
            cursor_col: self.cursor_col,
            saved_cursor: self.saved_cursor.take(),
        };
        self.alt_screen = Some(alt_state);

        // Reset cursor and ring buffer offset for alt screen
        self.cursor_row = 0;
        self.cursor_col = 0;
        self.cells_offset = 0;
    }

    /// Exit alternate screen buffer (DECRST 1049).
    pub fn exit_alt_screen(&mut self) {
        if let Some(alt_state) = self.alt_screen.take() {
            self.cells = alt_state.cells;
            self.cells_offset = alt_state.cells_offset;
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

    // ========== Selection Support ==========

    /// Extract text from a selection range.
    /// The start/end points should already be normalized (start before end).
    /// Returns the selected text with newlines between rows.
    #[must_use]
    pub fn extract_selection_text(
        &self,
        start_row: usize,
        start_col: usize,
        end_row: usize,
        end_col: usize,
    ) -> String {
        let mut result = String::new();
        // PERFORMANCE: Use visible_row() instead of visible_rows() to avoid Vec allocation
        let visible_count = self.visible_row_count();

        for row_idx in start_row..=end_row {
            if row_idx >= visible_count {
                break;
            }

            let Some(row) = self.visible_row(row_idx) else {
                continue;
            };
            let (col_start, col_end) = if start_row == end_row {
                // Single line selection
                (start_col, end_col)
            } else if row_idx == start_row {
                // First line: from start_col to end of line
                (start_col, row.len().saturating_sub(1))
            } else if row_idx == end_row {
                // Last line: from start of line to end_col
                (0, end_col)
            } else {
                // Middle lines: entire line
                (0, row.len().saturating_sub(1))
            };

            // Extract characters from the row
            let mut line = String::new();
            for col in col_start..=col_end.min(row.len().saturating_sub(1)) {
                if let Some(cell) = row.get(col) {
                    line.push(cell.ch);
                }
            }

            // Trim trailing spaces from each line
            let trimmed = line.trim_end();
            result.push_str(trimmed);

            // Add newline between rows (not after the last row)
            if row_idx < end_row {
                result.push('\n');
            }
        }

        result
    }

    // ========== Resize ==========

    /// Resize the terminal grid.
    ///
    /// For horizontal-only resize (rows unchanged): just update cols, preserve all cells.
    /// For vertical resize: copy content from BOTTOM of old grid to BOTTOM of new grid.
    pub fn resize(&mut self, rows: u16, cols: u16) {
        if rows == self.rows && cols == self.cols {
            return;
        }

        // Horizontal-only resize: don't recreate grid, just update cols
        // This preserves content during shrink, restored when expanding
        if rows == self.rows {
            self.cols = cols;
            self.cursor_col = self.cursor_col.min(cols.saturating_sub(1));
            self.scroll_bottom = rows.saturating_sub(1);
            self.tab_stops = (0..cols).filter(|c| c % 8 == 0).collect();
            return;
        }

        let mut new_cells = Self::create_empty_grid(rows, cols);

        // Copy from bottom of old grid to bottom of new grid
        let old_rows = self.rows as usize;
        let new_rows = rows as usize;
        let copy_rows = old_rows.min(new_rows);
        let src_start = old_rows.saturating_sub(copy_rows);
        let dst_start = new_rows.saturating_sub(copy_rows);

        for i in 0..copy_rows {
            if let Some(src_row) = self.row(src_start + i) {
                for (c, cell) in src_row.iter().enumerate().take(cols as usize) {
                    new_cells[dst_start + i][c] = cell.clone();
                }
            }
        }

        // If expanding rows, fill empty top rows from scrollback
        if new_rows > old_rows && !self.scrollback.is_empty() {
            let empty_rows = dst_start;
            let fill_count = empty_rows.min(self.scrollback.len());
            let scrollback_start = self.scrollback.len() - fill_count;

            // Pull from end of scrollback (most recent) to top of grid
            for (i, new_row) in new_cells.iter_mut().take(fill_count).enumerate() {
                if let Some(src_row) = self.scrollback.get(scrollback_start + i) {
                    for (c, cell) in src_row.iter().enumerate().take(cols as usize) {
                        new_row[c] = cell.clone();
                    }
                }
            }

            // Remove those lines from scrollback (they're now in grid)
            self.scrollback.truncate(scrollback_start);
        }

        // Adjust cursor to stay at same position relative to bottom
        let cursor_from_bottom = old_rows
            .saturating_sub(1)
            .saturating_sub(self.cursor_row as usize);
        let new_cursor = new_rows
            .saturating_sub(1)
            .saturating_sub(cursor_from_bottom.min(new_rows - 1));

        self.cells = new_cells;
        self.cells_offset = 0;
        self.scroll_offset = 0; // Always show live view after resize
        self.rows = rows;
        self.cols = cols;
        self.cursor_row = new_cursor as u16;
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

/// Iterator over visible rows that caches bounds calculation.
///
/// More efficient than calling `visible_row(idx)` in a loop because bounds
/// are calculated only once during iterator creation.
#[derive(Debug)]
pub struct VisibleRowsIter<'a> {
    grid: &'a TerminalGrid,
    end: usize,
    scrollback_len: usize,
    current: usize,
}

impl<'a> Iterator for VisibleRowsIter<'a> {
    type Item = &'a [Cell];

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        if self.current >= self.end {
            return None;
        }

        let absolute_idx = self.current;
        self.current += 1;

        if absolute_idx < self.scrollback_len {
            Some(self.grid.scrollback[absolute_idx].as_slice())
        } else {
            let logical_idx = absolute_idx - self.scrollback_len;
            self.grid.row(logical_idx).map(|r| r.as_slice())
        }
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.end.saturating_sub(self.current);
        (remaining, Some(remaining))
    }
}

impl ExactSizeIterator for VisibleRowsIter<'_> {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_alt_screen_preserves_ring_buffer_offset() {
        let mut grid = TerminalGrid::new(10, 80);

        // Scroll to create non-zero offset
        for _ in 0..5 {
            grid.scroll_up(1);
        }
        let offset_before = grid.cells_offset;
        assert!(
            offset_before > 0,
            "Should have non-zero offset after scrolling"
        );

        // Enter alt screen - offset should be reset
        grid.enter_alt_screen();
        assert_eq!(
            grid.cells_offset, 0,
            "Alt screen should start with zero offset"
        );

        // Scroll in alt screen
        grid.scroll_up(3);
        assert!(grid.cells_offset > 0, "Alt screen should allow scrolling");

        // Exit alt screen - original offset should be restored
        grid.exit_alt_screen();
        assert_eq!(
            grid.cells_offset, offset_before,
            "Original offset should be restored after exiting alt screen"
        );
    }
}
