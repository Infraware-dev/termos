//! Clipboard operations for copy and paste.
//!
//! Provides `ClipboardManager` for handling text selection copying
//! and paste operations using arboard for direct OS clipboard access.

use super::state::AppState;

/// Manages clipboard operations using arboard.
///
/// Uses arboard for immediate OS clipboard access, bypassing egui's delayed sync.
pub struct ClipboardManager {
    /// Clipboard instance (kept alive to avoid macOS issues)
    clipboard: Option<arboard::Clipboard>,
}

impl std::fmt::Debug for ClipboardManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClipboardManager")
            .field("clipboard", &self.clipboard.is_some())
            .finish()
    }
}

impl ClipboardManager {
    /// Creates a new clipboard manager.
    pub fn new() -> Self {
        let clipboard = arboard::Clipboard::new()
            .map_err(|e| log::error!("Failed to init clipboard: {}", e))
            .ok();

        Self { clipboard }
    }

    /// Copies selected text from the active session to the clipboard.
    pub fn copy_selection(&mut self, ctx: &egui::Context, state: &AppState) {
        let Some(session) = state.sessions.get(&state.active_session_id) else {
            log::info!("No active session for copy");
            return;
        };

        log::info!("copy_selection called, selection: {:?}", session.selection);

        let Some(ref sel) = session.selection else {
            log::info!("No selection exists");
            return;
        };

        if sel.is_empty() {
            log::info!("Selection is empty, nothing to copy");
            return;
        }

        let (start, end) = sel.normalized();
        log::info!(
            "Extracting text from ({},{}) to ({},{})",
            start.row,
            start.col,
            end.row,
            end.col
        );

        let text = session
            .terminal_handler
            .grid()
            .extract_selection_text(start.row, start.col, end.row, end.col);

        log::info!("Extracted text: '{}' ({} chars)", text, text.len());

        if !text.is_empty() {
            self.set_text(&text, ctx);
        }
    }

    /// Sets text to the clipboard.
    fn set_text(&mut self, text: &str, ctx: &egui::Context) {
        if let Some(ref mut cb) = self.clipboard {
            match cb.set_text(text) {
                Ok(()) => {
                    log::info!(
                        "Text copied to OS clipboard via arboard ({} chars)",
                        text.len()
                    );
                }
                Err(e) => {
                    log::error!("Arboard copy error: {}", e);
                    // Fallback to egui if arboard fails
                    ctx.copy_text(text.to_string());
                }
            }
        } else {
            // Fallback to egui if arboard init failed
            log::warn!("Arboard not available, using egui fallback");
            ctx.copy_text(text.to_string());
        }
    }

    /// Gets the paste payload, handling bracketed paste mode.
    ///
    /// Returns the bytes to send to PTY, or None if clipboard is empty.
    pub fn get_paste_payload(&mut self, state: &AppState) -> Option<Vec<u8>> {
        log::info!("get_paste_payload called");

        let cb = self.clipboard.as_mut()?;

        match cb.get_text() {
            Ok(text) if !text.is_empty() => {
                let use_bracketed = state
                    .active_session()
                    .is_some_and(|s| s.terminal_handler.bracketed_paste_enabled());

                let mut payload =
                    Vec::with_capacity(text.len() + if use_bracketed { 12 } else { 0 });

                if use_bracketed {
                    payload.extend_from_slice(b"\x1b[200~");
                }

                payload.extend_from_slice(text.as_bytes());

                if use_bracketed {
                    payload.extend_from_slice(b"\x1b[201~");
                }

                log::info!(
                    "Pasting {} bytes to PTY (bracketed: {}, text: '{}')",
                    payload.len(),
                    use_bracketed,
                    if text.len() > 50 {
                        format!("{}...", &text[..50])
                    } else {
                        text
                    }
                );

                Some(payload)
            }
            Ok(_) => {
                log::warn!("Clipboard is empty, nothing to paste");
                None
            }
            Err(e) => {
                log::error!("Failed to read clipboard via arboard: {}", e);
                None
            }
        }
    }
}

impl Default for ClipboardManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clipboard_manager_new() {
        // This test may fail on systems without clipboard support (CI)
        // but should at least not panic
        let manager = ClipboardManager::new();
        // Clipboard availability depends on the system
        let _ = manager.clipboard.is_some();
    }

    #[test]
    fn test_clipboard_manager_default() {
        let manager1 = ClipboardManager::new();
        let manager2 = ClipboardManager::default();
        // Both should have same availability
        assert_eq!(manager1.clipboard.is_some(), manager2.clipboard.is_some());
    }

    #[test]
    fn test_clipboard_manager_debug() {
        let manager = ClipboardManager::new();
        let debug_str = format!("{:?}", manager);
        assert!(debug_str.contains("ClipboardManager"));
        assert!(debug_str.contains("clipboard"));
    }
}
