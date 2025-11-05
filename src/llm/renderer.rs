/// Markdown and code rendering for LLM responses
use syntect::easy::HighlightLines;
use syntect::highlighting::{Style, ThemeSet};
use syntect::parsing::SyntaxSet;
use syntect::util::{as_24_bit_terminal_escaped, LinesWithEndings};

/// Renderer for LLM responses with basic markdown support
pub struct ResponseRenderer {
    syntax_set: SyntaxSet,
    theme_set: ThemeSet,
}

impl ResponseRenderer {
    /// Create a new response renderer
    pub fn new() -> Self {
        Self {
            syntax_set: SyntaxSet::load_defaults_newlines(),
            theme_set: ThemeSet::load_defaults(),
        }
    }

    /// Render an LLM response with basic markdown formatting
    pub fn render(&self, text: &str) -> Vec<String> {
        let mut output = Vec::new();
        let mut in_code_block = false;
        let mut code_lang = String::new();
        let mut code_lines = Vec::new();

        for line in text.lines() {
            // Detect code block start/end
            if line.starts_with("```") {
                if in_code_block {
                    // End of code block - apply syntax highlighting
                    let highlighted = self.highlight_code(&code_lines, &code_lang);
                    output.extend(highlighted);
                    code_lines.clear();
                    in_code_block = false;
                } else {
                    // Start of code block
                    code_lang = line.trim_start_matches("```").trim().to_string();
                    in_code_block = true;
                }
                continue;
            }

            if in_code_block {
                code_lines.push(line.to_string());
            } else {
                // Basic inline formatting
                let formatted = self.format_inline(line);
                output.push(formatted);
            }
        }

        // Handle unclosed code block
        if in_code_block && !code_lines.is_empty() {
            let highlighted = self.highlight_code(&code_lines, &code_lang);
            output.extend(highlighted);
        }

        output
    }

    /// Apply basic inline formatting
    fn format_inline(&self, line: &str) -> String {
        let mut result = line.to_string();

        // Simple bold formatting (limited support for M1)
        // This is a simplified version - full markdown parsing would be in M2/M3
        if result.contains("**") {
            result = result.replace("**", "\x1b[1m"); // Bold toggle
        }

        // Inline code formatting
        if result.contains('`') {
            // Simple replacement - proper parsing would handle escape sequences
            let parts: Vec<&str> = result.split('`').collect();
            if parts.len() > 1 {
                result = parts
                    .iter()
                    .enumerate()
                    .map(|(i, part)| {
                        if i % 2 == 1 {
                            format!("\x1b[36m{}\x1b[0m", part) // Cyan for inline code
                        } else {
                            part.to_string()
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("");
            }
        }

        result
    }

    /// Highlight code with syntax highlighting
    fn highlight_code(&self, lines: &[String], lang: &str) -> Vec<String> {
        // Find syntax definition
        let syntax = self
            .syntax_set
            .find_syntax_by_extension(lang)
            .unwrap_or_else(|| self.syntax_set.find_syntax_plain_text());

        let theme = &self.theme_set.themes["base16-ocean.dark"];
        let mut highlighter = HighlightLines::new(syntax, theme);

        let mut output = Vec::new();

        // Add code block header
        output.push(format!("\x1b[90m┌─ {} ─\x1b[0m", lang));

        for line in lines {
            let ranges = highlighter
                .highlight_line(line, &self.syntax_set)
                .unwrap_or_default();
            let escaped = as_24_bit_terminal_escaped(&ranges[..], false);
            output.push(format!("\x1b[90m│\x1b[0m {}", escaped));
        }

        // Add code block footer
        output.push("\x1b[90m└─\x1b[0m".to_string());

        output
    }
}

impl Default for ResponseRenderer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_plain_text() {
        let renderer = ResponseRenderer::new();
        let text = "Hello world\nThis is a test";
        let result = renderer.render(text);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_render_code_block() {
        let renderer = ResponseRenderer::new();
        let text = "Here is code:\n```rust\nfn main() {\n    println!(\"Hello\");\n}\n```\nDone.";
        let result = renderer.render(text);
        assert!(!result.is_empty());
        // Should have the "Here is code:" line, code lines, and "Done." line
        assert!(result.len() > 3);
    }

    #[test]
    fn test_format_inline_code() {
        let renderer = ResponseRenderer::new();
        let formatted = renderer.format_inline("Use `ls -la` to list files");
        assert!(formatted.contains("\x1b[36m")); // Should have color code
    }
}
