/// Markdown and code rendering for LLM responses
use syntect::easy::HighlightLines;
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSet;
use syntect::util::as_24_bit_terminal_escaped;

/// Renderer for LLM responses with basic markdown support
pub struct ResponseRenderer {
    syntax_set: SyntaxSet,
    theme_set: ThemeSet,
}

impl std::fmt::Debug for ResponseRenderer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ResponseRenderer")
            .field("syntax_set", &"<SyntaxSet>")
            .field("theme_set", &"<ThemeSet>")
            .finish()
    }
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
    #[cfg(test)]
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
    #[cfg(test)]
    fn format_inline(&self, line: &str) -> String {
        let mut result = line.to_string();

        // Simple bold formatting (limited support for M1)
        // This is a simplified version - full markdown parsing would be in M2/M3
        if result.contains("**") {
            let parts: Vec<&str> = result.split("**").collect();
            result = parts
                .iter()
                .enumerate()
                .map(|(i, part)| {
                    if i % 2 == 1 {
                        // Odd indices are inside **...** pairs
                        format!("\x1b[1m{part}\x1b[0m")
                    } else {
                        (*part).to_string()
                    }
                })
                .collect::<String>();
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
                            format!("\x1b[36m{part}\x1b[0m") // Cyan for inline code
                        } else {
                            (*part).to_string()
                        }
                    })
                    .collect::<String>();
            }
        }

        result
    }

    /// Highlight code with syntax highlighting
    pub fn highlight_code(&self, lines: &[String], lang: &str) -> Vec<String> {
        // Find syntax definition
        let syntax = self
            .syntax_set
            .find_syntax_by_extension(lang)
            .unwrap_or_else(|| self.syntax_set.find_syntax_plain_text());

        let theme = &self.theme_set.themes["base16-ocean.dark"];
        let mut highlighter = HighlightLines::new(syntax, theme);

        let mut output = Vec::new();

        // Add simple language label if present
        if !lang.is_empty() {
            output.push(format!("\x1b[90m[{lang}]\x1b[0m"));
        }

        for line in lines {
            let ranges = highlighter
                .highlight_line(line, &self.syntax_set)
                .unwrap_or_default();
            let escaped = as_24_bit_terminal_escaped(&ranges[..], false);
            output.push(format!("  {escaped}"));
        }

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
    fn test_renderer_creation() {
        let renderer = ResponseRenderer::new();
        let debug_str = format!("{:?}", renderer);
        assert!(debug_str.contains("ResponseRenderer"));
    }

    #[test]
    fn test_renderer_default() {
        let renderer = ResponseRenderer::default();
        let debug_str = format!("{:?}", renderer);
        assert!(debug_str.contains("ResponseRenderer"));
    }

    #[test]
    fn test_render_empty_string() {
        let renderer = ResponseRenderer::new();
        let lines = renderer.render("");
        assert!(lines.is_empty());
    }

    #[test]
    fn test_render_plain_text() {
        let renderer = ResponseRenderer::new();
        let lines = renderer.render("Hello, world!");
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0], "Hello, world!");
    }

    #[test]
    fn test_render_multiline() {
        let renderer = ResponseRenderer::new();
        let lines = renderer.render("Line 1\nLine 2\nLine 3");
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0], "Line 1");
        assert_eq!(lines[1], "Line 2");
        assert_eq!(lines[2], "Line 3");
    }

    #[test]
    fn test_render_bold_text() {
        let renderer = ResponseRenderer::new();
        let lines = renderer.render("This is **bold** text");
        assert_eq!(lines.len(), 1);
        assert!(lines[0].contains("\x1b[1m")); // Bold escape code
        assert!(lines[0].contains("bold"));
        assert!(lines[0].contains("\x1b[0m")); // Reset code
    }

    #[test]
    fn test_render_multiple_bold() {
        let renderer = ResponseRenderer::new();
        let lines = renderer.render("**first** and **second**");
        assert_eq!(lines.len(), 1);
        // Count occurrences of bold start
        let bold_count = lines[0].matches("\x1b[1m").count();
        assert_eq!(bold_count, 2);
    }

    #[test]
    fn test_render_inline_code() {
        let renderer = ResponseRenderer::new();
        let lines = renderer.render("Use `ls -la` command");
        assert_eq!(lines.len(), 1);
        assert!(lines[0].contains("\x1b[36m")); // Cyan escape code
        assert!(lines[0].contains("ls -la"));
        assert!(lines[0].contains("\x1b[0m")); // Reset code
    }

    #[test]
    fn test_render_multiple_inline_code() {
        let renderer = ResponseRenderer::new();
        let lines = renderer.render("Use `ls` or `dir` commands");
        assert_eq!(lines.len(), 1);
        // Count occurrences of cyan color
        let cyan_count = lines[0].matches("\x1b[36m").count();
        assert_eq!(cyan_count, 2);
    }

    #[test]
    fn test_render_code_block_basic() {
        let renderer = ResponseRenderer::new();
        let lines = renderer.render("```\ncode here\n```");

        // Should have code line (no header for empty lang, no footer)
        assert!(!lines.is_empty());

        // Should NOT have box drawing characters
        let joined = lines.join("");
        assert!(!joined.contains("┌"));
        assert!(!joined.contains("│"));
        assert!(!joined.contains("└"));
    }

    #[test]
    fn test_render_code_block_with_language() {
        let renderer = ResponseRenderer::new();
        let lines = renderer.render("```rust\nlet x = 5;\n```");

        // First line should contain language label
        assert!(lines[0].contains("[rust]"));
    }

    #[test]
    fn test_render_code_block_multiline() {
        let renderer = ResponseRenderer::new();
        let lines = renderer.render("```bash\necho hello\necho world\n```");

        // Should have header + 2 code lines = 3 lines
        assert!(lines.len() >= 3);

        // Should NOT have box drawing characters
        let joined = lines.join("");
        assert!(!joined.contains("│"));

        // Code lines should be indented
        let indented_lines: Vec<_> = lines.iter().filter(|l| l.starts_with("  ")).collect();
        assert_eq!(indented_lines.len(), 2);
    }

    #[test]
    fn test_render_unclosed_code_block() {
        let renderer = ResponseRenderer::new();
        // Unclosed code block should still render
        let lines = renderer.render("```python\nprint('hello')");

        // Should still produce output
        assert!(!lines.is_empty());
        // Should contain the code
        let joined = lines.join("\n");
        assert!(joined.contains("print"));
    }

    #[test]
    fn test_render_mixed_content() {
        let renderer = ResponseRenderer::new();
        let text = "Here is some **bold** text.\n\n```bash\nls -la\n```\n\nAnd `inline` code.";
        let lines = renderer.render(text);

        // Should have multiple lines
        assert!(lines.len() > 3);

        // Check for bold
        assert!(lines[0].contains("\x1b[1m"));

        // Check for inline code in last line
        let last_line = lines.last().unwrap();
        assert!(last_line.contains("\x1b[36m"));
    }

    #[test]
    fn test_render_no_formatting_passthrough() {
        let renderer = ResponseRenderer::new();
        let lines = renderer.render("No special formatting here.");
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0], "No special formatting here.");
    }

    #[test]
    fn test_render_unmatched_backticks() {
        let renderer = ResponseRenderer::new();
        // Single backtick should be treated as inline code start without end
        let lines = renderer.render("Single `backtick");
        assert_eq!(lines.len(), 1);
        // The behavior with unmatched backtick depends on implementation
        // but it shouldn't panic
    }

    #[test]
    fn test_render_empty_code_block() {
        let renderer = ResponseRenderer::new();
        let lines = renderer.render("```\n```");

        // Empty code block with no language should produce empty output
        assert!(lines.is_empty());
    }
}
