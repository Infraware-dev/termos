//! Terminal theme configuration.

use egui::Color32;

/// Theme colors for the terminal.
#[derive(Debug, Clone)]
#[allow(dead_code)]
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
    /// Split separator color
    pub split_separator: Color32,
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
            background: Color32::from_rgb(27, 27, 27),       // #1b1b1b
            text: Color32::from_rgb(204, 204, 204),          // #cccccc
            prompt_prefix: Color32::from_rgb(198, 208, 214), // #c6d0d6
            prompt_path: Color32::from_rgb(198, 208, 214),   // #c6d0d6
            cursor: Color32::from_rgb(204, 204, 204),        // #cccccc
            selection: Color32::from_rgba_unmultiplied(97, 175, 239, 100), // #61afef with alpha
            llm_response: Color32::from_rgb(204, 204, 204),  // #cccccc
            error: Color32::from_rgb(224, 108, 117),         // #e06c75 (red)
            titlebar_bg: Color32::from_rgb(27, 27, 27),      // #1b1b1b
            titlebar_text: Color32::from_rgb(204, 204, 204), // #cccccc
            split_separator: Color32::from_rgb(97, 97, 97),  // #616161
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_theme_default_is_dark() {
        let default_theme = Theme::default();
        let dark_theme = Theme::dark();

        // Both should have same background
        assert_eq!(default_theme.background, dark_theme.background);
        assert_eq!(default_theme.text, dark_theme.text);
    }

    #[test]
    fn test_theme_dark_colors() {
        let theme = Theme::dark();

        // Background should be dark
        assert_eq!(theme.background, Color32::from_rgb(27, 27, 27));

        // Text should be light gray
        assert_eq!(theme.text, Color32::from_rgb(204, 204, 204));

        // Error should be reddish
        assert_eq!(theme.error, Color32::from_rgb(224, 108, 117));
    }

    #[test]
    fn test_theme_debug() {
        let theme = Theme::dark();
        let debug_str = format!("{:?}", theme);
        assert!(debug_str.contains("Theme"));
        assert!(debug_str.contains("background"));
    }

    #[test]
    fn test_theme_clone() {
        let theme1 = Theme::dark();
        let theme2 = theme1.clone();
        assert_eq!(theme1.background, theme2.background);
        assert_eq!(theme1.text, theme2.text);
    }
}
