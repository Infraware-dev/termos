//! Terminal rendering state and helpers.
//!
//! Provides `RenderState` which holds reusable buffers and font metrics
//! for efficient terminal rendering.

use egui::{Color32, FontFamily, FontId};

use crate::config::rendering;
use crate::input::SelectionPoint;
#[cfg(test)]
use crate::terminal::cell::NamedColor;
use crate::terminal::cell::{Cell, Color};

/// Reusable render buffers and font metrics.
///
/// Cleared each frame to avoid allocations.
#[derive(Debug)]
pub struct RenderState {
    /// Font metrics - character width
    pub char_width: f32,
    /// Font metrics - character height
    pub char_height: f32,
    /// Cached font for rendering
    pub font_id: FontId,
    /// Background rectangles buffer
    pub bg_rects: Vec<(f32, f32, Color32)>,
    /// Text runs buffer - (x_offset, end_index_in_text_buffer, color)
    pub text_runs: Vec<(f32, usize, Color32)>,
    /// Single text buffer for all runs in a row
    pub text_buffer: String,
    /// Decorations buffer - (x, underline, strikethrough, color)
    pub decorations: Vec<(f32, bool, bool, Color32)>,
}

/// Context for rendering a single row.
#[derive(Debug, Clone)]
pub struct RowRenderContext {
    /// Row index in visible area
    pub row_idx: usize,
    /// Number of columns to render
    pub cols: usize,
    /// Selection bounds (start, end) if any
    pub selection_bounds: Option<(SelectionPoint, SelectionPoint)>,
    /// Theme background color
    pub theme_background: Color32,
    /// Theme selection color
    pub theme_selection: Color32,
}

impl RenderState {
    /// Creates a new render state with default metrics.
    pub fn new() -> Self {
        Self {
            char_width: rendering::CHAR_WIDTH,
            char_height: rendering::CHAR_HEIGHT,
            font_id: FontId::new(rendering::FONT_SIZE, FontFamily::Monospace),
            bg_rects: Vec::with_capacity(32),
            text_runs: Vec::with_capacity(32),
            text_buffer: String::with_capacity(256),
            decorations: Vec::with_capacity(8),
        }
    }

    /// Clears all buffers for the next frame.
    pub fn clear_buffers(&mut self) {
        self.bg_rects.clear();
        self.text_runs.clear();
        self.text_buffer.clear();
        self.decorations.clear();
    }

    /// Renders a single row of cells into the buffers.
    ///
    /// Call `clear_buffers()` before this, then use the filled buffers
    /// to draw backgrounds, text, and decorations.
    pub fn render_row(&mut self, row: &[Cell], column_x_coords: &[f32], ctx: &RowRenderContext) {
        let mut bg_start: Option<(usize, Color32)> = None;
        let mut run_start: Option<(usize, Color32)> = None;
        let row_len = row.len().min(ctx.cols);

        for (col_idx, cell) in row.iter().take(row_len).enumerate() {
            let (cell_fg, cell_bg) = if cell.attrs.reverse() {
                (color_to_egui(cell.bg), color_to_egui(cell.fg))
            } else {
                (color_to_egui(cell.fg), color_to_egui(cell.bg))
            };

            let cell_fg = if cell.attrs.dim() {
                Color32::from_rgba_unmultiplied(cell_fg.r(), cell_fg.g(), cell_fg.b(), 128)
            } else {
                cell_fg
            };

            let is_selected = is_cell_selected(ctx.row_idx, col_idx, &ctx.selection_bounds);

            let bg = if is_selected {
                ctx.theme_selection
            } else {
                cell_bg
            };

            // Background batching
            if is_selected || bg != ctx.theme_background {
                match bg_start {
                    Some((_start, color)) if color == bg => {}
                    Some((start, color)) => {
                        let width = (col_idx - start) as f32 * self.char_width;
                        self.bg_rects.push((column_x_coords[start], width, color));
                        bg_start = Some((col_idx, bg));
                    }
                    None => {
                        bg_start = Some((col_idx, bg));
                    }
                }
            } else if let Some((start, color)) = bg_start.take() {
                let width = (col_idx - start) as f32 * self.char_width;
                self.bg_rects.push((column_x_coords[start], width, color));
            }

            // Text run batching
            if cell.ch == ' ' || cell.attrs.hidden() {
                if let Some((start, color)) = run_start.take() {
                    let end_idx = self.text_buffer.len();
                    if end_idx > 0 {
                        self.text_runs
                            .push((column_x_coords[start], end_idx, color));
                    }
                }
            } else {
                match run_start {
                    Some((_start, color)) if color == cell_fg => {
                        self.text_buffer.push(cell.ch);
                    }
                    Some((start, color)) => {
                        let end_idx = self.text_buffer.len();
                        if end_idx > 0 {
                            self.text_runs
                                .push((column_x_coords[start], end_idx, color));
                        }
                        run_start = Some((col_idx, cell_fg));
                        self.text_buffer.push(cell.ch);
                    }
                    None => {
                        run_start = Some((col_idx, cell_fg));
                        self.text_buffer.push(cell.ch);
                    }
                }
            }

            // Decorations
            if cell.attrs.underline() || cell.attrs.strikethrough() {
                self.decorations.push((
                    column_x_coords[col_idx],
                    cell.attrs.underline(),
                    cell.attrs.strikethrough(),
                    cell_fg,
                ));
            }
        }

        // Flush remaining background run
        if let Some((start, color)) = bg_start {
            let width = (row_len - start) as f32 * self.char_width;
            self.bg_rects.push((column_x_coords[start], width, color));
        }

        // Flush remaining text run
        if let Some((start, color)) = run_start {
            let end_idx = self.text_buffer.len();
            if end_idx > 0 {
                self.text_runs
                    .push((column_x_coords[start], end_idx, color));
            }
        }
    }
}

impl Default for RenderState {
    fn default() -> Self {
        Self::new()
    }
}

/// Converts terminal Color to egui Color32.
#[inline]
pub fn color_to_egui(color: Color) -> Color32 {
    color.to_egui(true)
}

/// Checks if a cell at (row_idx, col_idx) is within the selection bounds.
#[inline]
fn is_cell_selected(
    row_idx: usize,
    col_idx: usize,
    selection_bounds: &Option<(SelectionPoint, SelectionPoint)>,
) -> bool {
    let Some((start, end)) = selection_bounds else {
        return false;
    };

    if row_idx < start.row || row_idx > end.row {
        false
    } else if row_idx == start.row && row_idx == end.row {
        col_idx >= start.col && col_idx <= end.col
    } else if row_idx == start.row {
        col_idx >= start.col
    } else if row_idx == end.row {
        col_idx <= end.col
    } else {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terminal::cell::CellAttrs;

    #[test]
    fn test_render_state_new() {
        let state = RenderState::new();
        assert!(state.bg_rects.is_empty());
        assert!(state.text_runs.is_empty());
        assert!(state.text_buffer.is_empty());
        assert!(state.decorations.is_empty());
    }

    #[test]
    fn test_render_state_clear_buffers() {
        let mut state = RenderState::new();
        state.bg_rects.push((0.0, 10.0, Color32::RED));
        state.text_buffer.push_str("test");

        state.clear_buffers();

        assert!(state.bg_rects.is_empty());
        assert!(state.text_buffer.is_empty());
    }

    #[test]
    fn test_is_cell_selected_no_selection() {
        assert!(!is_cell_selected(0, 0, &None));
    }

    #[test]
    fn test_is_cell_selected_single_row() {
        let bounds = Some((
            SelectionPoint { row: 1, col: 2 },
            SelectionPoint { row: 1, col: 5 },
        ));

        assert!(!is_cell_selected(0, 3, &bounds)); // Row before
        assert!(!is_cell_selected(2, 3, &bounds)); // Row after
        assert!(!is_cell_selected(1, 1, &bounds)); // Col before
        assert!(!is_cell_selected(1, 6, &bounds)); // Col after
        assert!(is_cell_selected(1, 2, &bounds)); // Start
        assert!(is_cell_selected(1, 3, &bounds)); // Middle
        assert!(is_cell_selected(1, 5, &bounds)); // End
    }

    #[test]
    fn test_is_cell_selected_multi_row() {
        let bounds = Some((
            SelectionPoint { row: 1, col: 5 },
            SelectionPoint { row: 3, col: 3 },
        ));

        // First row: col >= 5
        assert!(!is_cell_selected(1, 4, &bounds));
        assert!(is_cell_selected(1, 5, &bounds));
        assert!(is_cell_selected(1, 10, &bounds));

        // Middle row: all cols selected
        assert!(is_cell_selected(2, 0, &bounds));
        assert!(is_cell_selected(2, 50, &bounds));

        // Last row: col <= 3
        assert!(is_cell_selected(3, 0, &bounds));
        assert!(is_cell_selected(3, 3, &bounds));
        assert!(!is_cell_selected(3, 4, &bounds));
    }

    #[test]
    fn test_render_row_empty() {
        let mut state = RenderState::new();
        let row: Vec<Cell> = vec![];
        let column_x_coords: Vec<f32> = vec![];
        let ctx = RowRenderContext {
            row_idx: 0,
            cols: 80,
            selection_bounds: None,
            theme_background: Color32::BLACK,
            theme_selection: Color32::BLUE,
        };

        state.render_row(&row, &column_x_coords, &ctx);

        assert!(state.bg_rects.is_empty());
        assert!(state.text_runs.is_empty());
    }

    #[test]
    fn test_render_row_with_text() {
        let mut state = RenderState::new();
        let row: Vec<Cell> = vec![
            Cell::new(
                'H',
                Color::Named(NamedColor::White),
                Color::Named(NamedColor::Background),
                CellAttrs::empty(),
            ),
            Cell::new(
                'i',
                Color::Named(NamedColor::White),
                Color::Named(NamedColor::Background),
                CellAttrs::empty(),
            ),
        ];
        let column_x_coords: Vec<f32> = vec![0.0, 8.4, 16.8];
        let ctx = RowRenderContext {
            row_idx: 0,
            cols: 80,
            selection_bounds: None,
            theme_background: Color32::from_rgb(27, 27, 27),
            theme_selection: Color32::BLUE,
        };

        state.render_row(&row, &column_x_coords, &ctx);

        assert_eq!(state.text_buffer, "Hi");
        assert_eq!(state.text_runs.len(), 1);
    }

    #[test]
    fn test_render_row_with_selection() {
        let mut state = RenderState::new();
        let row: Vec<Cell> = vec![
            Cell::new(
                'A',
                Color::Named(NamedColor::White),
                Color::Named(NamedColor::Background),
                CellAttrs::empty(),
            ),
            Cell::new(
                'B',
                Color::Named(NamedColor::White),
                Color::Named(NamedColor::Background),
                CellAttrs::empty(),
            ),
            Cell::new(
                'C',
                Color::Named(NamedColor::White),
                Color::Named(NamedColor::Background),
                CellAttrs::empty(),
            ),
        ];
        let column_x_coords: Vec<f32> = vec![0.0, 8.4, 16.8, 25.2];
        let ctx = RowRenderContext {
            row_idx: 0,
            cols: 80,
            selection_bounds: Some((
                SelectionPoint { row: 0, col: 1 },
                SelectionPoint { row: 0, col: 1 },
            )),
            theme_background: Color32::from_rgb(27, 27, 27),
            theme_selection: Color32::BLUE,
        };

        state.render_row(&row, &column_x_coords, &ctx);

        // Should have background rect for the selected cell
        assert!(!state.bg_rects.is_empty());
    }

    #[test]
    fn test_render_row_with_decorations() {
        let mut state = RenderState::new();
        let mut attrs = CellAttrs::empty();
        attrs.set_underline(true);
        let row: Vec<Cell> = vec![Cell::new(
            'X',
            Color::Named(NamedColor::White),
            Color::Named(NamedColor::Background),
            attrs,
        )];
        let column_x_coords: Vec<f32> = vec![0.0, 8.4];
        let ctx = RowRenderContext {
            row_idx: 0,
            cols: 80,
            selection_bounds: None,
            theme_background: Color32::from_rgb(27, 27, 27),
            theme_selection: Color32::BLUE,
        };

        state.render_row(&row, &column_x_coords, &ctx);

        assert_eq!(state.decorations.len(), 1);
        assert!(state.decorations[0].1); // underline = true
    }

    #[test]
    fn test_color_to_egui() {
        let white = color_to_egui(Color::Named(NamedColor::White));
        assert_eq!(white, NamedColor::White.to_egui());

        let rgb = color_to_egui(Color::Rgb(255, 128, 0));
        assert_eq!(rgb, Color32::from_rgb(255, 128, 0));
    }
}
