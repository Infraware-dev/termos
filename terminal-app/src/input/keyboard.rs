//! Keyboard input handling for terminal emulator.
//!
//! Extracts keyboard events from egui and converts them to terminal actions.

use egui::Key;

/// Actions that can result from keyboard input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyboardAction {
    /// Send bytes to the PTY
    SendBytes(Vec<u8>),
    /// Send SIGINT signal (Ctrl+C)
    SendSigInt,
    /// Copy selected text to clipboard (Cmd+C on macOS, Ctrl+Shift+C on Linux)
    Copy,
    /// Paste from clipboard (Cmd+V on macOS, Ctrl+Shift+V on Linux)
    Paste,
}

/// Keyboard handler that processes egui input and returns terminal actions.
#[derive(Debug, Default)]
pub struct KeyboardHandler {
    /// Buffer for collecting actions (reused to avoid allocations)
    actions: Vec<KeyboardAction>,
}

impl KeyboardHandler {
    /// Create a new keyboard handler.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Process keyboard input from egui and return actions to execute.
    ///
    /// Returns a list of actions. The caller is responsible for executing them.
    pub fn process(&mut self, ctx: &egui::Context) -> Vec<KeyboardAction> {
        self.actions.clear();

        // Process clipboard shortcuts FIRST (Cmd+C/V on macOS, Ctrl+Shift+C/V on Linux)
        // These take priority over Ctrl+C (SIGINT)
        let clipboard_action = Self::process_clipboard_keys(ctx);
        if let Some(action) = clipboard_action {
            self.actions.push(action);
            return std::mem::take(&mut self.actions);
        }

        // Process Ctrl+key combinations via event iteration
        // (more reliable on Linux than modifiers + key_pressed)
        let ctrl_action = Self::process_ctrl_keys(ctx);
        if let Some(action) = ctrl_action {
            self.actions.push(action);
            return std::mem::take(&mut self.actions);
        }

        // Process other keys via key_pressed
        self.process_other_keys(ctx);

        // Process text input events
        self.process_text_input(ctx);

        std::mem::take(&mut self.actions)
    }

    /// Process clipboard shortcuts (Copy/Paste).
    ///
    /// Priority order:
    /// 1. egui native events (Event::Copy, Event::Paste) - most reliable
    /// 2. Manual key detection as fallback
    ///
    /// - macOS: Cmd+C for copy, Cmd+V for paste
    /// - Linux/Windows: Ctrl+Shift+C for copy, Ctrl+Shift+V for paste
    fn process_clipboard_keys(ctx: &egui::Context) -> Option<KeyboardAction> {
        ctx.input(|i| {
            // 1. Check egui native events FIRST (OS sends these reliably)
            for event in &i.events {
                match event {
                    egui::Event::Copy => {
                        log::info!("Event::Copy detected");
                        return Some(KeyboardAction::Copy);
                    }
                    egui::Event::Paste(_) => {
                        // We detect the event but use arboard for actual paste
                        log::info!("Event::Paste detected");
                        return Some(KeyboardAction::Paste);
                    }
                    _ => {}
                }
            }

            // 2. Fallback: Manual key detection (Cmd on macOS, Ctrl+Shift elsewhere)
            #[cfg(target_os = "macos")]
            {
                if i.modifiers.command && i.key_pressed(Key::V) {
                    log::info!("Cmd+V detected (macOS paste fallback)");
                    return Some(KeyboardAction::Paste);
                }
                if i.modifiers.command && i.key_pressed(Key::C) {
                    log::info!("Cmd+C detected (macOS copy fallback)");
                    return Some(KeyboardAction::Copy);
                }
            }

            #[cfg(not(target_os = "macos"))]
            {
                if i.modifiers.ctrl && i.modifiers.shift && i.key_pressed(Key::V) {
                    log::info!("Ctrl+Shift+V detected (paste fallback)");
                    return Some(KeyboardAction::Paste);
                }
                if i.modifiers.ctrl && i.modifiers.shift && i.key_pressed(Key::C) {
                    log::info!("Ctrl+Shift+C detected (copy fallback)");
                    return Some(KeyboardAction::Copy);
                }
            }

            None
        })
    }

    /// Process Ctrl+key combinations by iterating events directly.
    fn process_ctrl_keys(ctx: &egui::Context) -> Option<KeyboardAction> {
        let mut result = None;

        ctx.input(|i| {
            for event in &i.events {
                if let egui::Event::Key {
                    key,
                    pressed,
                    modifiers,
                    ..
                } = event
                {
                    // Log key events for debugging
                    log::debug!(
                        "Key event: {:?} pressed={} ctrl={} alt={} shift={}",
                        key,
                        pressed,
                        modifiers.ctrl,
                        modifiers.alt,
                        modifiers.shift
                    );

                    if modifiers.ctrl && result.is_none() {
                        // Ctrl+C: accept either press or release (Linux quirk)
                        // Other Ctrl keys: only accept press to avoid double-fire
                        let is_ctrl_c = *key == Key::C;
                        if is_ctrl_c || *pressed {
                            log::info!("Ctrl+{:?} detected (pressed={})", key, pressed);
                            result = match key {
                                Key::C => Some(KeyboardAction::SendSigInt),
                                Key::D => Some(KeyboardAction::SendBytes(vec![0x04])), // EOF
                                Key::L => Some(KeyboardAction::SendBytes(vec![0x0C])), // Clear screen
                                Key::A => Some(KeyboardAction::SendBytes(vec![0x01])), // Beginning of line
                                Key::E => Some(KeyboardAction::SendBytes(vec![0x05])), // End of line
                                Key::K => Some(KeyboardAction::SendBytes(vec![0x0B])), // Kill to end
                                Key::U => Some(KeyboardAction::SendBytes(vec![0x15])), // Kill to beginning
                                Key::W => Some(KeyboardAction::SendBytes(vec![0x17])), // Delete word
                                Key::R => Some(KeyboardAction::SendBytes(vec![0x12])), // Reverse search
                                Key::Z => Some(KeyboardAction::SendBytes(vec![0x1A])), // Suspend
                                _ => None,
                            };
                        }
                    }
                }
            }
        });

        result
    }

    /// Process Alt and special keys via key_pressed API.
    fn process_other_keys(&mut self, ctx: &egui::Context) {
        ctx.input(|i| {
            // Alt combinations (Meta)
            if i.modifiers.alt {
                if i.key_pressed(Key::B) {
                    self.actions
                        .push(KeyboardAction::SendBytes(b"\x1bb".to_vec()));
                    return;
                }
                if i.key_pressed(Key::F) {
                    self.actions
                        .push(KeyboardAction::SendBytes(b"\x1bf".to_vec()));
                    return;
                }
                if i.key_pressed(Key::D) {
                    self.actions
                        .push(KeyboardAction::SendBytes(b"\x1bd".to_vec()));
                    return;
                }
            }

            // Special keys
            if i.key_pressed(Key::Enter) {
                self.actions.push(KeyboardAction::SendBytes(b"\r".to_vec()));
                return;
            }
            if i.key_pressed(Key::Backspace) {
                self.actions.push(KeyboardAction::SendBytes(vec![0x7F]));
                return;
            }
            if i.key_pressed(Key::Tab) {
                self.actions.push(KeyboardAction::SendBytes(vec![0x09]));
                return;
            }
            if i.key_pressed(Key::Escape) {
                self.actions.push(KeyboardAction::SendBytes(vec![0x1B]));
                return;
            }

            // Arrow keys
            if i.key_pressed(Key::ArrowUp) {
                self.actions
                    .push(KeyboardAction::SendBytes(b"\x1b[A".to_vec()));
                return;
            }
            if i.key_pressed(Key::ArrowDown) {
                self.actions
                    .push(KeyboardAction::SendBytes(b"\x1b[B".to_vec()));
                return;
            }
            if i.key_pressed(Key::ArrowRight) {
                self.actions
                    .push(KeyboardAction::SendBytes(b"\x1b[C".to_vec()));
                return;
            }
            if i.key_pressed(Key::ArrowLeft) {
                self.actions
                    .push(KeyboardAction::SendBytes(b"\x1b[D".to_vec()));
                return;
            }

            // Navigation keys
            if i.key_pressed(Key::Home) {
                self.actions
                    .push(KeyboardAction::SendBytes(b"\x1b[H".to_vec()));
                return;
            }
            if i.key_pressed(Key::End) {
                self.actions
                    .push(KeyboardAction::SendBytes(b"\x1b[F".to_vec()));
                return;
            }
            if i.key_pressed(Key::PageUp) {
                self.actions
                    .push(KeyboardAction::SendBytes(b"\x1b[5~".to_vec()));
                return;
            }
            if i.key_pressed(Key::PageDown) {
                self.actions
                    .push(KeyboardAction::SendBytes(b"\x1b[6~".to_vec()));
                return;
            }
            if i.key_pressed(Key::Insert) {
                self.actions
                    .push(KeyboardAction::SendBytes(b"\x1b[2~".to_vec()));
                return;
            }
            if i.key_pressed(Key::Delete) {
                self.actions
                    .push(KeyboardAction::SendBytes(b"\x1b[3~".to_vec()));
                return;
            }

            // Function keys
            if i.key_pressed(Key::F1) {
                self.actions
                    .push(KeyboardAction::SendBytes(b"\x1bOP".to_vec()));
                return;
            }
            if i.key_pressed(Key::F2) {
                self.actions
                    .push(KeyboardAction::SendBytes(b"\x1bOQ".to_vec()));
                return;
            }
            if i.key_pressed(Key::F3) {
                self.actions
                    .push(KeyboardAction::SendBytes(b"\x1bOR".to_vec()));
                return;
            }
            if i.key_pressed(Key::F4) {
                self.actions
                    .push(KeyboardAction::SendBytes(b"\x1bOS".to_vec()));
                return;
            }
            if i.key_pressed(Key::F5) {
                self.actions
                    .push(KeyboardAction::SendBytes(b"\x1b[15~".to_vec()));
                return;
            }
            if i.key_pressed(Key::F6) {
                self.actions
                    .push(KeyboardAction::SendBytes(b"\x1b[17~".to_vec()));
                return;
            }
            if i.key_pressed(Key::F7) {
                self.actions
                    .push(KeyboardAction::SendBytes(b"\x1b[18~".to_vec()));
                return;
            }
            if i.key_pressed(Key::F8) {
                self.actions
                    .push(KeyboardAction::SendBytes(b"\x1b[19~".to_vec()));
                return;
            }
            if i.key_pressed(Key::F9) {
                self.actions
                    .push(KeyboardAction::SendBytes(b"\x1b[20~".to_vec()));
                return;
            }
            if i.key_pressed(Key::F10) {
                self.actions
                    .push(KeyboardAction::SendBytes(b"\x1b[21~".to_vec()));
                return;
            }
            if i.key_pressed(Key::F11) {
                self.actions
                    .push(KeyboardAction::SendBytes(b"\x1b[23~".to_vec()));
                return;
            }
            if i.key_pressed(Key::F12) {
                self.actions
                    .push(KeyboardAction::SendBytes(b"\x1b[24~".to_vec()));
            }
            // Note: Space is handled by process_text_input() via Event::Text
        });
    }

    /// Process text input events for printable characters.
    fn process_text_input(&mut self, ctx: &egui::Context) {
        ctx.input(|i| {
            for event in &i.events {
                if let egui::Event::Text(text) = event {
                    for c in text.chars() {
                        if c.is_ascii() {
                            self.actions.push(KeyboardAction::SendBytes(vec![c as u8]));
                        } else {
                            self.actions
                                .push(KeyboardAction::SendBytes(c.to_string().into_bytes()));
                        }
                    }
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_keyboard_handler_creation() {
        let handler = KeyboardHandler::new();
        assert!(handler.actions.is_empty());
    }

    #[test]
    fn test_keyboard_action_equality() {
        assert_eq!(
            KeyboardAction::SendBytes(vec![0x03]),
            KeyboardAction::SendBytes(vec![0x03])
        );
        assert_eq!(KeyboardAction::SendSigInt, KeyboardAction::SendSigInt);
    }
}
