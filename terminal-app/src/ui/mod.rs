//! UI module for terminal rendering.

#[allow(dead_code)]
mod prompt;
mod renderer;
mod theme;

pub use renderer::{
    render_backgrounds, render_cursor, render_decorations, render_scrollbar, render_text_runs,
};
pub use theme::Theme;
