//! Terminal rendering handler.
//!
//! Provides `TerminalRenderer` which handles the pure rendering phase
//! of terminal display, separated from input handling.

use egui::{Color32, Painter, Pos2, Rect};

use super::TimingState;
use super::render::{RenderState, RowRenderContext};
use super::state::AppState;
use crate::session::SessionId;
use crate::ui::{
    SPINNER_FRAMES, Theme, render_backgrounds, render_cursor, render_decorations, render_scrollbar,
    render_text_runs_buffered,
};

/// Handles pure terminal rendering (no input mutation).
///
/// Created temporarily during render phase after input handling is complete.
pub struct TerminalRenderer<'a> {
    /// Application state (read-only for rendering)
    state: &'a AppState,
    /// Render buffers and font metrics
    render: &'a mut RenderState,
    /// Theme colors
    theme: &'a Theme,
    /// Timing state for cursor blink
    timing: &'a TimingState,
}

impl<'a> TerminalRenderer<'a> {
    /// Creates a new terminal renderer.
    pub fn new(
        state: &'a AppState,
        render: &'a mut RenderState,
        theme: &'a Theme,
        timing: &'a TimingState,
    ) -> Self {
        Self {
            state,
            render,
            theme,
            timing,
        }
    }

    /// Renders the terminal grid for a session.
    ///
    /// This handles only the drawing phase - input handling (selection, scroll)
    /// must be done before calling this method.
    pub fn draw(&mut self, painter: &Painter, rect: Rect, session_id: SessionId, has_focus: bool) {
        let session = match self.state.sessions.get(&session_id) {
            Some(s) => s,
            None => return,
        };

        let show_throbber = session.should_show_throbber();
        let shell_initialized = session.shell_initialized;

        let grid = session.terminal_handler.grid();
        let scroll_offset = grid.scroll_offset();
        let max_scroll = grid.max_scroll_offset();
        let visible_row_count = grid.visible_row_count();
        let (cursor_row, cursor_col) = grid.cursor_position();
        let cursor_visible = grid.cursor_visible();
        let cols = grid.size().1 as usize;
        let column_x_coords: &[f32] = &session.column_x_coords;

        // Fill background
        painter.rect_filled(rect, 0.0, self.theme.background);

        if !shell_initialized {
            return;
        }

        let selection_bounds = session.selection.as_ref().map(|sel| sel.normalized());

        let font_id = self.render.font_id.clone();
        let char_width = self.render.char_width;
        let char_height = self.render.char_height;

        let row_ctx = RowRenderContext {
            row_idx: 0,
            cols,
            selection_bounds,
            theme_background: self.theme.background,
            theme_selection: self.theme.selection,
        };

        // Render each visible row
        for (row_idx, row) in grid.visible_rows_iter().enumerate() {
            let y = rect.top() + row_idx as f32 * char_height;
            self.render.clear_buffers();

            let mut ctx = row_ctx.clone();
            ctx.row_idx = row_idx;

            self.render.render_row(row, column_x_coords, &ctx);

            render_backgrounds(painter, rect, y, char_height, &self.render.bg_rects);
            render_text_runs_buffered(
                painter,
                rect,
                y,
                &font_id,
                &self.render.text_buffer,
                &self.render.text_runs,
            );
            render_decorations(
                painter,
                rect,
                y,
                char_width,
                char_height,
                &self.render.decorations,
            );
        }

        // Render throbber or cursor
        if show_throbber && scroll_offset == 0 {
            self.draw_throbber(painter, rect, cursor_row, cursor_col, &font_id);
        } else if cursor_visible
            && self.timing.cursor_blink_visible
            && scroll_offset == 0
            && has_focus
            && shell_initialized
        {
            self.draw_cursor(painter, rect, cursor_row, cursor_col, column_x_coords);
        }

        // Render scrollbar
        if max_scroll > 0 {
            render_scrollbar(painter, rect, scroll_offset, max_scroll, visible_row_count);
        }
    }

    /// Draws the spinner/throbber at cursor position.
    fn draw_throbber(
        &self,
        painter: &Painter,
        rect: Rect,
        cursor_row: u16,
        cursor_col: u16,
        font_id: &egui::FontId,
    ) {
        let frame_idx =
            (self.timing.startup_time.elapsed().as_millis() / 250) as usize % SPINNER_FRAMES.len();
        let frame = SPINNER_FRAMES[frame_idx];
        let row_y = rect.top() + cursor_row as f32 * self.render.char_height;
        let spinner_x = rect.left() + cursor_col as f32 * self.render.char_width;

        painter.text(
            Pos2::new(spinner_x, row_y),
            egui::Align2::LEFT_TOP,
            frame,
            font_id.clone(),
            Color32::from_rgb(0, 255, 255),
        );
    }

    /// Draws the cursor at its position.
    fn draw_cursor(
        &self,
        painter: &Painter,
        rect: Rect,
        cursor_row: u16,
        cursor_col: u16,
        column_x_coords: &[f32],
    ) {
        let cursor_col_idx = cursor_col as usize;
        let cursor_x = rect.left()
            + if cursor_col_idx < column_x_coords.len() {
                column_x_coords[cursor_col_idx]
            } else {
                cursor_col as f32 * self.render.char_width
            };
        let cursor_y = rect.top() + cursor_row as f32 * self.render.char_height;
        render_cursor(
            painter,
            cursor_x,
            cursor_y,
            self.render.char_height,
            self.theme.cursor,
        );
    }
}
