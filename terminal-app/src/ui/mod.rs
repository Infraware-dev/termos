pub mod renderer;
pub mod scrollbar;
pub mod theme;

pub use renderer::{
    render_backgrounds, render_cursor, render_decorations, render_scrollbar,
    render_text_runs_buffered,
};
pub use theme::Theme;

/// Dots spinner frames for the LLM throbber
pub const SPINNER_FRAMES: &[&str] = &[".", "..", "...", "...."];
