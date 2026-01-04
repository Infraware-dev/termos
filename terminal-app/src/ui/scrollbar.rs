//! Scrollbar logic and rendering.
//!
//! Handles interaction (dragging, clicking, auto-repeat) and visual representation
//! of the terminal scrollbar.

use egui::{Color32, Painter, Pos2, Rect, Stroke, Ui};
use std::time::Instant;

/// Action requested by the scrollbar interaction.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ScrollAction {
    /// Scroll up by N lines
    ScrollUp(usize),
    /// Scroll down by N lines
    ScrollDown(usize),
    /// Jump to specific offset (0 = bottom)
    ScrollTo(usize),
}

/// Scrollbar state and logic.
#[derive(Debug)]
pub struct Scrollbar {
    /// Is the user currently dragging the thumb?
    is_dragging: bool,
    /// Offset of the mouse click relative to the thumb top
    drag_offset: f32,
    /// Time of the last auto-repeat action
    last_action_time: Option<Instant>,
    /// Active auto-repeat direction (1 = up, -1 = down, 0 = none)
    active_direction: i8,
}

impl Default for Scrollbar {
    fn default() -> Self {
        Self {
            is_dragging: false,
            drag_offset: 0.0,
            last_action_time: None,
            active_direction: 0,
        }
    }
}

impl Scrollbar {
    pub fn new() -> Self {
        Self::default()
    }

    /// Check if the user is currently dragging the scrollbar thumb.
    pub fn is_dragging(&self) -> bool {
        self.is_dragging
    }

    /// Calculate scrollbar area (for exclusion from selection).
    pub fn area(&self, available_rect: Rect) -> Rect {
        let width = 12.0;
        let x = available_rect.right() - width - 2.0;
        // Includes padding and margins
        Rect::from_min_max(
            Pos2::new(x - 2.0, available_rect.top()),
            Pos2::new(available_rect.right(), available_rect.bottom())
        )
    }

    /// Update logic and render the scrollbar.
    /// Returns an optional scroll action to apply to the grid.
    pub fn show(
        &mut self,
        ui: &mut Ui,
        painter: &Painter,
        rect: Rect,
        scroll_offset: usize,
        max_scroll: usize,
        visible_lines: usize,
    ) -> Option<ScrollAction> {
        if max_scroll == 0 {
            return None;
        }

        // --- Geometry Constants ---
        let width = 12.0;
        let x = rect.right() - width - 2.0;
        let padding_top = 2.0;
        let padding_bottom = 12.0;
        let arrow_size = 12.0;

        let track_top = rect.top() + padding_top + arrow_size;
        let track_bottom = rect.bottom() - padding_bottom - arrow_size;
        let track_height = (track_bottom - track_top).max(0.0);

        // --- Layout Calculations ---
        let total_lines = max_scroll + visible_lines;
        let thumb_height = (visible_lines as f32 / total_lines as f32 * track_height).max(20.0);
        let travel_range = track_height - thumb_height;
        // Inverted logic: offset 0 is bottom (max Y), offset max is top (min Y)
        let scroll_pct = scroll_offset as f32 / max_scroll as f32;
        let thumb_y = track_top + (1.0 - scroll_pct) * travel_range;

        // --- Hit Testing Areas ---
        let thumb_rect = Rect::from_min_size(
            Pos2::new(x + 2.0, thumb_y),
            egui::Vec2::new(width - 4.0, thumb_height),
        );

        let track_rect = Rect::from_min_max(
            Pos2::new(x, track_top),
            Pos2::new(x + width, track_bottom),
        );

        let up_arrow_rect = Rect::from_min_max(
            Pos2::new(x, rect.top() + padding_top),
            Pos2::new(x + width, track_top),
        );

        let down_arrow_rect = Rect::from_min_max(
            Pos2::new(x, track_bottom),
            Pos2::new(x + width, rect.bottom() - padding_bottom),
        );

        // --- Interaction Handling ---
        let mut action = None;
        let pointer_pos = ui.input(|i| i.pointer.interact_pos());
        let pointer_down = ui.input(|i| i.pointer.primary_down());
        let pointer_pressed = ui.input(|i| i.pointer.primary_pressed());
        let pointer_released = ui.input(|i| i.pointer.primary_released());

        if pointer_released {
            self.is_dragging = false;
            self.active_direction = 0;
            self.last_action_time = None;
        }

        if let Some(pos) = pointer_pos {
            // 1. Dragging
            if thumb_rect.contains(pos) && pointer_pressed {
                self.is_dragging = true;
                self.drag_offset = pos.y - thumb_y;
            }

            if self.is_dragging && pointer_down {
                let new_thumb_y = (pos.y - self.drag_offset).clamp(track_top, track_top + travel_range);
                if travel_range > 0.0 {
                    // Convert Y back to offset
                    let new_pct = 1.0 - (new_thumb_y - track_top) / travel_range;
                    let new_offset = (new_pct * max_scroll as f32).round() as usize;
                    action = Some(ScrollAction::ScrollTo(new_offset));
                }
            }

            // 2. Track Click (Page Jump)
            if !self.is_dragging && track_rect.contains(pos) && !thumb_rect.contains(pos) && pointer_pressed {
                if pos.y < thumb_y {
                    action = Some(ScrollAction::ScrollUp(visible_lines));
                } else {
                    action = Some(ScrollAction::ScrollDown(visible_lines));
                }
            }

            // 3. Arrow Auto-Repeat
            if pointer_down && !self.is_dragging {
                let mut direction = 0;
                if up_arrow_rect.contains(pos) {
                    direction = 1;
                } else if down_arrow_rect.contains(pos) {
                    direction = -1;
                }

                if direction != 0 {
                    if self.active_direction != direction {
                        // New press
                        self.active_direction = direction;
                        self.last_action_time = Some(Instant::now());
                        action = Some(if direction == 1 {
                            ScrollAction::ScrollUp(1)
                        } else {
                            ScrollAction::ScrollDown(1)
                        });
                    } else if let Some(last_time) = self.last_action_time {
                        // Repeat logic
                        let initial_delay = std::time::Duration::from_millis(300);
                        let repeat_interval = std::time::Duration::from_millis(50);
                        
                        if last_time.elapsed() > initial_delay {
                            self.last_action_time = Some(Instant::now() - (initial_delay - repeat_interval));
                            action = Some(if direction == 1 {
                                ScrollAction::ScrollUp(1)
                            } else {
                                ScrollAction::ScrollDown(1)
                            });
                            ui.ctx().request_repaint();
                        }
                    }
                } else {
                    // Moved off arrow
                    self.active_direction = 0;
                    self.last_action_time = None;
                }
            }
        }

        // --- Rendering ---
        
        // Track
        painter.rect_filled(track_rect, 2.0, Color32::from_gray(30));

        // Thumb
        painter.rect_filled(thumb_rect, 4.0, if self.is_dragging {
            Color32::from_gray(140)
        } else {
            Color32::from_gray(100)
        });

        // Arrows
        let arrow_color = if self.active_direction != 0 { Color32::WHITE } else { Color32::from_gray(160) };
        let stroke = Stroke::new(1.5, arrow_color);

        // Up Arrow
        let up_center = up_arrow_rect.center();
        painter.line_segment([Pos2::new(up_center.x - 3.0, up_center.y + 2.0), Pos2::new(up_center.x, up_center.y - 2.0)], stroke);
        painter.line_segment([Pos2::new(up_center.x + 3.0, up_center.y + 2.0), Pos2::new(up_center.x, up_center.y - 2.0)], stroke);

        // Down Arrow
        let down_center = down_arrow_rect.center();
        painter.line_segment([Pos2::new(down_center.x - 3.0, down_center.y - 2.0), Pos2::new(down_center.x, down_center.y + 2.0)], stroke);
        painter.line_segment([Pos2::new(down_center.x + 3.0, down_center.y - 2.0), Pos2::new(down_center.x, down_center.y + 2.0)], stroke);

        action
    }
}
