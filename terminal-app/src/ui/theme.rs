//! Terminal theme configuration.

use egui::Color32;

/// Theme colors for the terminal.
#[derive(Debug, Clone)]
pub struct Theme {
    /// Background color
    pub background: Color32,
    /// Default text color
    pub text: Color32,
    /// Prompt prefix color (|~|)
    pub prompt_prefix: Color32,
    /// Prompt path color (user@host:path)
    pub prompt_path: Color32,
    /// Cursor color
    pub cursor: Color32,
    /// Selection color
    pub selection: Color32,
    /// LLM response color
    pub llm_response: Color32,
    /// Error color
    pub error: Color32,
    /// Title bar background
    pub titlebar_bg: Color32,
    /// Title bar text
    pub titlebar_text: Color32,
}

impl Default for Theme {
    fn default() -> Self {
        Self::dark()
    }
}

impl Theme {
    /// Dark theme matching the UI mockups.
    pub fn dark() -> Self {
        Self {
            background: Color32::from_rgb(45, 45, 45),      // #2d2d2d
            text: Color32::from_rgb(204, 204, 204),          // #cccccc
            prompt_prefix: Color32::from_rgb(204, 204, 204), // #cccccc (same as text)
            prompt_path: Color32::from_rgb(152, 195, 121),   // #98c379 (green)
            cursor: Color32::from_rgb(204, 204, 204),        // #cccccc
            selection: Color32::from_rgba_unmultiplied(97, 175, 239, 100), // #61afef with alpha
            llm_response: Color32::from_rgb(204, 204, 204),  // #cccccc
            error: Color32::from_rgb(224, 108, 117),         // #e06c75 (red)
            titlebar_bg: Color32::from_rgb(37, 37, 38),      // #252526
            titlebar_text: Color32::from_rgb(204, 204, 204), // #cccccc
        }
    }

    /// Apply theme to egui context.
    pub fn apply(&self, ctx: &egui::Context) {
        let mut style = (*ctx.style()).clone();

        // Set dark visuals
        style.visuals = egui::Visuals::dark();

        // Customize colors
        style.visuals.panel_fill = self.background;
        style.visuals.window_fill = self.background;
        style.visuals.extreme_bg_color = self.background;
        style.visuals.faint_bg_color = Color32::from_rgb(55, 55, 55);
        style.visuals.code_bg_color = Color32::from_rgb(55, 55, 55);

        // Text selection
        style.visuals.selection.bg_fill = self.selection;
        style.visuals.selection.stroke = egui::Stroke::NONE;

        // Widget colors
        style.visuals.widgets.noninteractive.bg_fill = self.background;
        style.visuals.widgets.inactive.bg_fill = Color32::from_rgb(55, 55, 55);
        style.visuals.widgets.hovered.bg_fill = Color32::from_rgb(65, 65, 65);
        style.visuals.widgets.active.bg_fill = Color32::from_rgb(75, 75, 75);

        // Override text colors
        style.visuals.override_text_color = Some(self.text);

        ctx.set_style(style);
    }
}
