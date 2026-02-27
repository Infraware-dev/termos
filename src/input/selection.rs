//! Text selection handling for mouse-based text selection.

/// A point in the terminal grid (row, column).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SelectionPoint {
    /// Row index (0-indexed, absolute including scrollback)
    pub row: usize,
    /// Column index (0-indexed)
    pub col: usize,
}

impl SelectionPoint {
    /// Create a new selection point.
    #[must_use]
    pub fn new(row: usize, col: usize) -> Self {
        Self { row, col }
    }
}

/// Represents a text selection in the terminal grid.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextSelection {
    /// Selection start point (where drag started)
    pub start: SelectionPoint,
    /// Selection end point (current drag position)
    pub end: SelectionPoint,
    /// Whether selection is currently active (drag in progress)
    pub active: bool,
}

impl TextSelection {
    /// Create a new selection starting at the given position.
    #[must_use]
    pub fn new(row: usize, col: usize) -> Self {
        let point = SelectionPoint::new(row, col);
        Self {
            start: point,
            end: point,
            active: true,
        }
    }

    /// Update the end point of the selection (during drag).
    pub fn update_end(&mut self, row: usize, col: usize) {
        self.end = SelectionPoint::new(row, col);
    }

    /// Returns normalized selection where start is always before end.
    /// Returns (start, end) tuple with start <= end.
    #[must_use]
    pub fn normalized(&self) -> (SelectionPoint, SelectionPoint) {
        if self.start.row < self.end.row
            || (self.start.row == self.end.row && self.start.col <= self.end.col)
        {
            (self.start, self.end)
        } else {
            (self.end, self.start)
        }
    }

    /// Check if a cell at (row, col) is within the selection.
    #[must_use]
    #[allow(dead_code)]
    pub fn contains(&self, row: usize, col: usize) -> bool {
        let (start, end) = self.normalized();

        if row < start.row || row > end.row {
            return false;
        }

        if start.row == end.row {
            // Single line selection
            col >= start.col && col <= end.col
        } else if row == start.row {
            // First line: from start.col to end of line
            col >= start.col
        } else if row == end.row {
            // Last line: from start of line to end.col
            col <= end.col
        } else {
            // Middle lines: entire line is selected
            true
        }
    }

    /// Check if selection is empty (start == end).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.start == self.end
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_selection_new() {
        let sel = TextSelection::new(5, 10);
        assert_eq!(sel.start.row, 5);
        assert_eq!(sel.start.col, 10);
        assert!(sel.active);
        assert!(sel.is_empty());
    }

    #[test]
    fn test_selection_update_end() {
        let mut sel = TextSelection::new(5, 10);
        sel.update_end(7, 15);
        assert_eq!(sel.end.row, 7);
        assert_eq!(sel.end.col, 15);
        assert!(!sel.is_empty());
    }

    #[test]
    fn test_selection_normalized_forward() {
        let mut sel = TextSelection::new(2, 5);
        sel.update_end(4, 10);
        let (start, end) = sel.normalized();
        assert_eq!(start.row, 2);
        assert_eq!(start.col, 5);
        assert_eq!(end.row, 4);
        assert_eq!(end.col, 10);
    }

    #[test]
    fn test_selection_normalized_backward() {
        let mut sel = TextSelection::new(4, 10);
        sel.update_end(2, 5);
        let (start, end) = sel.normalized();
        assert_eq!(start.row, 2);
        assert_eq!(start.col, 5);
        assert_eq!(end.row, 4);
        assert_eq!(end.col, 10);
    }

    #[test]
    fn test_selection_contains_single_line() {
        let mut sel = TextSelection::new(3, 5);
        sel.update_end(3, 10);

        assert!(!sel.contains(3, 4)); // before
        assert!(sel.contains(3, 5)); // start
        assert!(sel.contains(3, 7)); // middle
        assert!(sel.contains(3, 10)); // end
        assert!(!sel.contains(3, 11)); // after
        assert!(!sel.contains(2, 7)); // wrong row
    }

    #[test]
    fn test_selection_contains_multi_line() {
        let mut sel = TextSelection::new(2, 5);
        sel.update_end(4, 10);

        // Row 2 (first line)
        assert!(!sel.contains(2, 4)); // before start col
        assert!(sel.contains(2, 5)); // start col
        assert!(sel.contains(2, 50)); // any col after start

        // Row 3 (middle line)
        assert!(sel.contains(3, 0)); // start of line
        assert!(sel.contains(3, 50)); // any col

        // Row 4 (last line)
        assert!(sel.contains(4, 0)); // start of line
        assert!(sel.contains(4, 10)); // end col
        assert!(!sel.contains(4, 11)); // after end col

        // Outside rows
        assert!(!sel.contains(1, 5));
        assert!(!sel.contains(5, 5));
    }
}
