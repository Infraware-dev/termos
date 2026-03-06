//! Terminal rendering utilities.
//!
//! Provides helper functions for rendering terminal cells and decorations.

use egui::{Color32, FontId, Painter, Pos2, Rect, Stroke, Vec2};

/// Render a batch of background rectangles.
pub fn render_backgrounds(
    painter: &Painter,
    rect: Rect,
    y: f32,
    char_height: f32,
    bg_rects: &[(f32, f32, Color32)],
) {
    for (start_x, width, color) in bg_rects {
        painter.rect_filled(
            Rect::from_min_size(
                Pos2::new(rect.left() + start_x, y),
                Vec2::new(*width, char_height),
            ),
            0.0,
            *color,
        );
    }
}

/// Render text runs from a shared buffer (zero-allocation version).
/// Each run is (x_offset, end_index_in_buffer, color).
/// Text is extracted from buffer using previous end index as start.
#[inline]
pub fn render_text_runs_buffered(
    painter: &Painter,
    rect: Rect,
    y: f32,
    font_id: &FontId,
    text_buffer: &str,
    text_runs: &[(f32, usize, Color32)],
) {
    let mut prev_end = 0;
    for &(start_x, end_idx, color) in text_runs {
        let text = &text_buffer[prev_end..end_idx];
        if !text.is_empty() {
            painter.text(
                Pos2::new(rect.left() + start_x, y),
                egui::Align2::LEFT_TOP,
                text,
                font_id.clone(),
                color,
            );
        }
        prev_end = end_idx;
    }
}

/// Render text decorations (underline, strikethrough).
pub fn render_decorations(
    painter: &Painter,
    rect: Rect,
    y: f32,
    char_width: f32,
    char_height: f32,
    decorations: &[(f32, bool, bool, Color32)],
) {
    for (x, underline, strikethrough, fg) in decorations {
        let abs_x = rect.left() + x;
        if *underline {
            let y_line = y + char_height - 2.0;
            painter.line_segment(
                [
                    Pos2::new(abs_x, y_line),
                    Pos2::new(abs_x + char_width, y_line),
                ],
                Stroke::new(1.0, *fg),
            );
        }
        if *strikethrough {
            let y_line = y + char_height / 2.0;
            painter.line_segment(
                [
                    Pos2::new(abs_x, y_line),
                    Pos2::new(abs_x + char_width, y_line),
                ],
                Stroke::new(1.0, *fg),
            );
        }
    }
}

/// Render a vertical bar cursor.
pub fn render_cursor(
    painter: &Painter,
    cursor_x: f32,
    cursor_y: f32,
    char_height: f32,
    color: Color32,
) {
    let bar_rect = Rect::from_min_size(Pos2::new(cursor_x, cursor_y), Vec2::new(2.0, char_height));
    painter.rect_filled(bar_rect, 0.0, color);
}

/// Render a scrollbar with track, thumb, and arrows.
pub fn render_scrollbar(
    painter: &Painter,
    rect: Rect,
    scroll_offset: usize,
    max_scroll: usize,
    visible_lines: usize,
) {
    if max_scroll == 0 {
        return;
    }

    let scrollbar_width = 12.0; // Slightly wider for better interaction
    let scrollbar_x = rect.right() - scrollbar_width - 2.0;

    let padding_top = 2.0;
    let padding_bottom = 12.0;
    let arrow_size = 12.0;

    let track_top = rect.top() + padding_top + arrow_size;
    let track_bottom = rect.bottom() - padding_bottom - arrow_size;
    let track_height = (track_bottom - track_top).max(0.0);

    // 1. Draw Scrollbar Track (Background)
    let track_rect = Rect::from_min_max(
        Pos2::new(scrollbar_x, track_top),
        Pos2::new(scrollbar_x + scrollbar_width, track_bottom),
    );
    painter.rect_filled(track_rect, 2.0, Color32::from_gray(30));

    // 2. Calculate and Draw Thumb (Handle)
    let total_lines = max_scroll + visible_lines;
    let thumb_height = (visible_lines as f32 / total_lines as f32 * track_height).max(20.0);
    let travel_range = track_height - thumb_height;
    let scroll_pct = scroll_offset as f32 / max_scroll as f32;
    let thumb_y = track_top + (1.0 - scroll_pct) * travel_range;

    let thumb_rect = Rect::from_min_size(
        Pos2::new(scrollbar_x + 2.0, thumb_y),
        Vec2::new(scrollbar_width - 4.0, thumb_height),
    );
    painter.rect_filled(thumb_rect, 4.0, Color32::from_gray(100));

    // 3. Draw Arrows (Top and Bottom)
    let arrow_color = Color32::from_gray(160);
    let stroke = Stroke::new(1.5, arrow_color);

    // Up Arrow
    let up_arrow_center = Pos2::new(
        scrollbar_x + scrollbar_width / 2.0,
        rect.top() + padding_top + arrow_size / 2.0,
    );
    // Use painter.arrow or simpler lines
    painter.line_segment(
        [
            Pos2::new(up_arrow_center.x - 3.0, up_arrow_center.y + 2.0),
            Pos2::new(up_arrow_center.x, up_arrow_center.y - 2.0),
        ],
        stroke,
    );
    painter.line_segment(
        [
            Pos2::new(up_arrow_center.x + 3.0, up_arrow_center.y + 2.0),
            Pos2::new(up_arrow_center.x, up_arrow_center.y - 2.0),
        ],
        stroke,
    );

    // Down Arrow
    let down_arrow_center = Pos2::new(
        scrollbar_x + scrollbar_width / 2.0,
        rect.bottom() - padding_bottom - arrow_size / 2.0,
    );
    painter.line_segment(
        [
            Pos2::new(down_arrow_center.x - 3.0, down_arrow_center.y - 2.0),
            Pos2::new(down_arrow_center.x, down_arrow_center.y + 2.0),
        ],
        stroke,
    );
    painter.line_segment(
        [
            Pos2::new(down_arrow_center.x + 3.0, down_arrow_center.y - 2.0),
            Pos2::new(down_arrow_center.x, down_arrow_center.y + 2.0),
        ],
        stroke,
    );
}
