//! Incremental markdown renderer for streaming LLM responses.
//!
//! Provides `IncrementalRenderer` which accumulates text chunks and produces
//! formatted lines suitable for immediate VTE display.

use super::renderer::ResponseRenderer;

/// Stateful renderer for incremental LLM response display.
///
/// Tracks accumulated text and render state across chunks, allowing
/// progressive display as content arrives.
#[derive(Debug)]
pub struct IncrementalRenderer {
    /// Underlying markdown renderer
    renderer: ResponseRenderer,
    /// Accumulated raw text from chunks
    accumulated: String,
    /// Number of characters already rendered (index into accumulated)
    rendered_pos: usize,
    /// Current line buffer for incomplete lines
    line_buffer: String,
    /// Whether we're currently inside a code block
    in_code_block: bool,
    /// Language for current code block (if any)
    code_lang: String,
    /// Accumulated code lines in current block
    code_lines: Vec<String>,
    /// Whether we've started outputting for the current response
    started: bool,
    /// Whether the previous chunk output ended with partial content on a new line
    had_partial_on_newline: bool,
}

impl IncrementalRenderer {
    /// Creates a new incremental renderer.
    pub fn new() -> Self {
        Self {
            renderer: ResponseRenderer::new(),
            accumulated: String::new(),
            rendered_pos: 0,
            line_buffer: String::new(),
            in_code_block: false,
            code_lang: String::new(),
            code_lines: Vec::new(),
            started: false,
            had_partial_on_newline: false,
        }
    }

    /// Resets the renderer for a new response.
    pub fn reset(&mut self) {
        self.accumulated.clear();
        self.rendered_pos = 0;
        self.line_buffer.clear();
        self.in_code_block = false;
        self.code_lang.clear();
        self.code_lines.clear();
        self.started = false;
        self.had_partial_on_newline = false;
    }

    /// Appends a chunk and returns complete lines ready for display.
    ///
    /// Returns a tuple of:
    /// - `Vec<String>`: Complete lines ready for VTE output (with ANSI formatting)
    /// - `Option<String>`: Partial line text that should be displayed but not yet confirmed
    ///
    /// The partial line is useful for showing text as it streams, but may be reformatted
    /// when more content arrives (e.g., if it becomes part of a code block).
    pub fn append(&mut self, chunk: &str) -> (Vec<String>, Option<String>) {
        self.accumulated.push_str(chunk);
        self.process_new_content()
    }

    /// Finalizes the stream and returns any remaining buffered content.
    ///
    /// Call this when the stream ends to flush any partial lines.
    pub fn finalize(&mut self) -> Vec<String> {
        let mut output = Vec::new();

        // If we're in a code block, close it
        if self.in_code_block && !self.code_lines.is_empty() {
            let highlighted = self.highlight_code_block();
            output.extend(highlighted);
            self.code_lines.clear();
            self.in_code_block = false;
        }

        // Flush any remaining line buffer
        if !self.line_buffer.is_empty() {
            let formatted = self.format_inline(&self.line_buffer);
            output.push(formatted);
            self.line_buffer.clear();
        }

        output
    }

    /// Processes new content and extracts complete lines.
    fn process_new_content(&mut self) -> (Vec<String>, Option<String>) {
        let mut output = Vec::new();

        // Get unprocessed content
        let unprocessed = &self.accumulated[self.rendered_pos..];
        self.line_buffer.push_str(unprocessed);
        self.rendered_pos = self.accumulated.len();

        // Process complete lines from buffer
        while let Some(newline_pos) = self.line_buffer.find('\n') {
            let line = self.line_buffer[..newline_pos].to_string();
            self.line_buffer = self.line_buffer[newline_pos + 1..].to_string();

            if let Some(formatted) = self.process_line(&line) {
                output.extend(formatted);
            }
        }

        // Return partial line if any (for preview)
        // Don't show partial lines that look like code block markers - wait for full line
        let partial = if !self.line_buffer.is_empty()
            && !self.in_code_block
            && !self.line_buffer.starts_with("```")
        {
            Some(self.format_inline(&self.line_buffer))
        } else if self.in_code_block
            && !self.line_buffer.is_empty()
            && !self.line_buffer.starts_with("```")
        {
            // In code block with incomplete line, show current partial
            // But not if it looks like a closing marker
            Some(format!("  {}", self.line_buffer))
        } else if self.in_code_block && !self.code_lines.is_empty() {
            // In code block with complete lines, show the last line as preview
            Some(format!("  {}", self.code_lines.last().unwrap()))
        } else {
            None
        };

        (output, partial)
    }

    /// Processes a single complete line.
    ///
    /// Returns formatted output lines, or None if the line is buffered (e.g., code block).
    fn process_line(&mut self, line: &str) -> Option<Vec<String>> {
        // Check for code block markers
        if line.starts_with("```") {
            if self.in_code_block {
                // End of code block - render it
                let highlighted = self.highlight_code_block();
                self.code_lines.clear();
                self.in_code_block = false;
                self.code_lang.clear();
                return Some(highlighted);
            } else {
                // Start of code block
                self.code_lang = line.trim_start_matches("```").trim().to_string();
                self.in_code_block = true;
                return None;
            }
        }

        if self.in_code_block {
            // Accumulate code lines
            self.code_lines.push(line.to_string());
            None
        } else {
            // Regular text - format and output immediately
            let formatted = self.format_inline(line);
            Some(vec![formatted])
        }
    }

    /// Formats inline markdown (bold, inline code).
    fn format_inline(&self, line: &str) -> String {
        let mut result = line.to_string();

        // Bold formatting
        if result.contains("**") {
            let parts: Vec<&str> = result.split("**").collect();
            result = parts
                .iter()
                .enumerate()
                .map(|(i, part)| {
                    if i % 2 == 1 {
                        format!("\x1b[1m{part}\x1b[0m")
                    } else {
                        (*part).to_string()
                    }
                })
                .collect::<String>();
        }

        // Inline code formatting
        if result.contains('`') {
            let parts: Vec<&str> = result.split('`').collect();
            if parts.len() > 1 {
                result = parts
                    .iter()
                    .enumerate()
                    .map(|(i, part)| {
                        if i % 2 == 1 {
                            format!("\x1b[36m{part}\x1b[0m")
                        } else {
                            (*part).to_string()
                        }
                    })
                    .collect::<String>();
            }
        }

        result
    }

    /// Highlights accumulated code lines.
    fn highlight_code_block(&self) -> Vec<String> {
        // Delegate to the underlying renderer's highlighting
        self.renderer
            .highlight_code(&self.code_lines, &self.code_lang)
    }

    /// Returns whether the renderer has started output.
    pub fn has_started(&self) -> bool {
        self.started
    }

    /// Marks the renderer as started.
    pub fn mark_started(&mut self) {
        self.started = true;
    }

    /// Returns whether the previous output ended with partial content on a new line.
    pub fn had_partial_on_newline(&self) -> bool {
        self.had_partial_on_newline
    }

    /// Sets whether partial content was output on a new line.
    pub fn set_partial_on_newline(&mut self, value: bool) {
        self.had_partial_on_newline = value;
    }
}

impl Default for IncrementalRenderer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_incremental_renderer_creation() {
        let renderer = IncrementalRenderer::new();
        assert!(!renderer.has_started());
    }

    #[test]
    fn test_incremental_renderer_default() {
        let renderer = IncrementalRenderer::default();
        assert!(!renderer.has_started());
    }

    #[test]
    fn test_simple_text_streaming() {
        let mut renderer = IncrementalRenderer::new();

        // First chunk: incomplete line
        let (lines, partial) = renderer.append("Hello, ");
        assert!(lines.is_empty());
        assert_eq!(partial, Some("Hello, ".to_string()));

        // Second chunk: completes the line
        let (lines, partial) = renderer.append("world!\n");
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0], "Hello, world!");
        assert!(partial.is_none());
    }

    #[test]
    fn test_multiple_complete_lines() {
        let mut renderer = IncrementalRenderer::new();

        let (lines, partial) = renderer.append("Line 1\nLine 2\nLine 3\n");
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0], "Line 1");
        assert_eq!(lines[1], "Line 2");
        assert_eq!(lines[2], "Line 3");
        assert!(partial.is_none());
    }

    #[test]
    fn test_bold_formatting() {
        let mut renderer = IncrementalRenderer::new();

        let (lines, _) = renderer.append("This is **bold** text\n");
        assert_eq!(lines.len(), 1);
        assert!(lines[0].contains("\x1b[1m"));
        assert!(lines[0].contains("bold"));
        assert!(lines[0].contains("\x1b[0m"));
    }

    #[test]
    fn test_inline_code_formatting() {
        let mut renderer = IncrementalRenderer::new();

        let (lines, _) = renderer.append("Use `ls -la` command\n");
        assert_eq!(lines.len(), 1);
        assert!(lines[0].contains("\x1b[36m"));
        assert!(lines[0].contains("ls -la"));
    }

    #[test]
    fn test_code_block_spanning_chunks() {
        let mut renderer = IncrementalRenderer::new();

        // Start code block
        let (lines, _) = renderer.append("```rust\n");
        assert!(lines.is_empty()); // Code block start doesn't produce output

        // Add code line
        let (lines, partial) = renderer.append("let x = 5;\n");
        assert!(lines.is_empty()); // Still buffering
        assert!(partial.is_some()); // But we show partial preview

        // End code block
        let (lines, _) = renderer.append("```\n");
        assert!(!lines.is_empty()); // Now we get the highlighted code
        assert!(lines[0].contains("[rust]")); // Language label
    }

    #[test]
    fn test_code_block_multiline() {
        let mut renderer = IncrementalRenderer::new();

        let (lines, _) = renderer.append("```bash\necho hello\necho world\n```\n");
        // Should have: language label + 2 code lines
        assert!(lines.len() >= 3);
        assert!(lines[0].contains("[bash]"));
    }

    #[test]
    fn test_finalize_flushes_buffer() {
        let mut renderer = IncrementalRenderer::new();

        // Incomplete line without newline
        let (lines, _) = renderer.append("Incomplete");
        assert!(lines.is_empty());

        // Finalize should flush it
        let final_lines = renderer.finalize();
        assert_eq!(final_lines.len(), 1);
        assert_eq!(final_lines[0], "Incomplete");
    }

    #[test]
    fn test_finalize_unclosed_code_block() {
        let mut renderer = IncrementalRenderer::new();

        renderer.append("```python\nprint('hello')\n");
        let final_lines = renderer.finalize();
        assert!(!final_lines.is_empty());
        // Should contain the code even though block wasn't closed
        let joined = final_lines.join("\n");
        assert!(joined.contains("print"));
    }

    #[test]
    fn test_reset() {
        let mut renderer = IncrementalRenderer::new();

        renderer.append("Some text\n");
        renderer.mark_started();
        assert!(renderer.has_started());

        renderer.reset();
        assert!(!renderer.has_started());

        // Should behave as fresh
        let (lines, partial) = renderer.append("New ");
        assert!(lines.is_empty());
        assert_eq!(partial, Some("New ".to_string()));
    }

    #[test]
    fn test_mixed_content_streaming() {
        let mut renderer = IncrementalRenderer::new();

        // Text chunk
        let (lines, _) = renderer.append("Here is some **bold** text.\n\n");
        assert!(!lines.is_empty());
        assert!(lines[0].contains("\x1b[1m"));

        // Code block
        let (lines, _) = renderer.append("```bash\nls -la\n```\n");
        assert!(!lines.is_empty());

        // More text
        let (lines, _) = renderer.append("And `inline` code.\n");
        assert_eq!(lines.len(), 1);
        assert!(lines[0].contains("\x1b[36m"));
    }

    #[test]
    fn test_partial_line_preview() {
        let mut renderer = IncrementalRenderer::new();

        // Partial line should be returned as preview
        let (_, partial) = renderer.append("Typing");
        assert_eq!(partial, Some("Typing".to_string()));

        // More partial content
        let (_, partial) = renderer.append(" more");
        assert_eq!(partial, Some("Typing more".to_string()));

        // Complete the line
        let (lines, partial) = renderer.append("...\n");
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0], "Typing more...");
        assert!(partial.is_none());
    }

    #[test]
    fn test_empty_chunk() {
        let mut renderer = IncrementalRenderer::new();

        let (lines, partial) = renderer.append("");
        assert!(lines.is_empty());
        assert!(partial.is_none());
    }

    #[test]
    fn test_only_newlines() {
        let mut renderer = IncrementalRenderer::new();

        let (lines, _) = renderer.append("\n\n\n");
        assert_eq!(lines.len(), 3);
        assert!(lines.iter().all(|l| l.is_empty()));
    }

    #[test]
    fn test_formatting_across_chunk_boundary() {
        let mut renderer = IncrementalRenderer::new();

        // Bold split across chunks - this is tricky
        // The current implementation will handle it when the line completes
        let (lines, partial) = renderer.append("This is **bo");
        assert!(lines.is_empty());
        // Partial might show unformatted text since ** isn't complete
        assert!(partial.is_some());

        let (lines, _) = renderer.append("ld** text\n");
        assert_eq!(lines.len(), 1);
        // Now the full line should be formatted
        assert!(lines[0].contains("\x1b[1m"));
    }

    #[test]
    fn test_code_block_partial_preview() {
        let mut renderer = IncrementalRenderer::new();

        renderer.append("```rust\n");

        // Partial code line should show preview with indentation
        let (_, partial) = renderer.append("let x = ");
        assert!(partial.is_some());
        assert!(partial.unwrap().starts_with("  ")); // Indented
    }

    #[test]
    fn test_partial_code_block_marker_not_shown() {
        let mut renderer = IncrementalRenderer::new();

        // Partial code block opening marker should not be shown
        let (lines, partial) = renderer.append("```");
        assert!(lines.is_empty());
        assert!(partial.is_none()); // Don't show partial ``` markers

        // When newline arrives, code block starts properly
        let (lines, _) = renderer.append("rust\nlet x = 5;\n```\n");
        // Should get highlighted code when block closes
        assert!(!lines.is_empty());
    }

    #[test]
    fn test_partial_closing_marker_not_shown() {
        let mut renderer = IncrementalRenderer::new();

        // Start a code block
        renderer.append("```rust\n");
        renderer.append("let x = 5;\n");

        // Partial closing marker should not be shown as "```"
        // Instead we continue showing the last code line as preview
        let (lines, partial) = renderer.append("```");
        assert!(lines.is_empty()); // Not closed yet (no newline)
        assert!(partial.is_some()); // Shows last code line as preview
        assert!(!partial.unwrap().contains("```")); // But NOT the ``` marker

        // When newline arrives, code block closes
        let (lines, _) = renderer.append("\n");
        assert!(!lines.is_empty()); // Now we get highlighted code
    }
}
