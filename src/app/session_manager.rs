//! Session lifecycle management.
//!
//! Provides `SessionManager` for creating, closing, and initializing terminal sessions.
//! This module is designed to be testable with minimal egui dependencies.

use std::collections::HashMap;

use egui_tiles::{TileId, Tree};

use super::TilesManager;
use super::state::AppState;
use crate::config::timing;
use crate::session::{SessionId, TerminalSession};

/// Result of closing a session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CloseResult {
    /// Session was closed successfully
    Closed {
        /// The next session to activate, if any
        next_active: Option<SessionId>,
    },
    /// The last session was closed, application should quit
    LastSessionClosed,
    /// Session was not found
    NotFound,
}

/// Session lifecycle management.
///
/// Provides static methods for session creation, closing, and initialization.
/// Designed to work with `AppState` and egui_tiles tree.
pub struct SessionManager;

impl SessionManager {
    /// Creates a new terminal session and returns its ID.
    pub fn create(state: &mut AppState, runtime: &tokio::runtime::Handle) -> SessionId {
        let id = state.allocate_session_id();
        let session = TerminalSession::new(id, runtime);
        state.sessions.insert(id, session);
        tracing::info!("Created new session {}", id);
        id
    }

    /// Closes a session by ID.
    ///
    /// When closing a tab:
    /// - Activates the tab to the left (or first remaining if closing leftmost)
    /// - If only one tab remains, unwraps it from Tabs container
    pub fn close(
        state: &mut AppState,
        tiles: &mut Option<Tree<SessionId>>,
        session_tile_ids: &mut HashMap<SessionId, TileId>,
        session_id: SessionId,
    ) -> CloseResult {
        if state.sessions.remove(&session_id).is_none() {
            return CloseResult::NotFound;
        }

        tracing::info!("Closed session {}", session_id);

        let Some(tile_id) = session_tile_ids.remove(&session_id) else {
            return Self::handle_post_close(state);
        };

        let Some(tree) = tiles else {
            return Self::handle_post_close(state);
        };

        let Some(root_id) = tree.root() else {
            return Self::handle_post_close(state);
        };

        // Handle tab-specific logic
        let mut next_active_tile: Option<TileId> = None;
        let mut should_unwrap_single_tab = false;
        let mut single_remaining_tile: Option<TileId> = None;

        if let Some(egui_tiles::Tile::Container(egui_tiles::Container::Tabs(tabs))) =
            tree.tiles.get_mut(root_id)
            && let Some(closing_idx) = tabs.children.iter().position(|&id| id == tile_id)
        {
            tabs.children.remove(closing_idx);

            if !tabs.children.is_empty() {
                let next_idx = if closing_idx > 0 { closing_idx - 1 } else { 0 };
                let next_tile_id = tabs.children[next_idx];
                tabs.active = Some(next_tile_id);
                next_active_tile = Some(next_tile_id);

                if tabs.children.len() == 1 {
                    should_unwrap_single_tab = true;
                    single_remaining_tile = Some(tabs.children[0]);
                }
            }
        }

        // Find session for next active tab
        let next_active_session = next_active_tile
            .and_then(|tid| TilesManager::find_first_pane_session(&tree.tiles, tid));

        // Remove the closed tile
        tree.tiles.remove(tile_id);
        tracing::debug!("Removed tile {:?} for session {}", tile_id, session_id);

        // Unwrap single remaining tab
        if should_unwrap_single_tab && let Some(remaining_tile_id) = single_remaining_tile {
            tree.tiles.remove(root_id);
            *tree = Tree::new(
                "terminal_tiles",
                remaining_tile_id,
                std::mem::take(&mut tree.tiles),
            );
            tracing::info!("Unwrapped single tab to single pane view");
        }

        // Update active session
        if let Some(new_session_id) = next_active_session {
            state.active_session_id = new_session_id;
            tracing::info!("Switched to session {}", new_session_id);
            return CloseResult::Closed {
                next_active: Some(new_session_id),
            };
        } else if state.active_session_id == session_id
            && let Some(&new_id) = state.sessions.keys().next()
        {
            state.active_session_id = new_id;
            tracing::info!("Fallback: switched to session {}", new_id);
            return CloseResult::Closed {
                next_active: Some(new_id),
            };
        }

        Self::handle_post_close(state)
    }

    /// Handles post-close cleanup and determines result.
    fn handle_post_close(state: &mut AppState) -> CloseResult {
        if state.sessions.is_empty() {
            tracing::info!("All sessions closed, quitting application");
            CloseResult::LastSessionClosed
        } else {
            // Mark remaining sessions for repaint
            for session in state.sessions.values_mut() {
                session.needs_repaint = true;
            }
            CloseResult::Closed { next_active: None }
        }
    }

    /// Initializes shell with custom prompt for a session (two-phase).
    ///
    /// Phase 1: After SHELL_INIT_DELAY, send init commands
    /// Phase 2: After INIT_COMMANDS_DELAY, enable rendering
    pub fn initialize_shell(state: &mut AppState, session_id: SessionId) {
        let session = match state.sessions.get_mut(&session_id) {
            Some(s) => s,
            None => return,
        };

        if session.shell_initialized {
            return;
        }

        // Phase 1: Send init commands after shell startup delay
        if session.init_commands_sent_at.is_none() {
            if session.startup_time.elapsed() < timing::SHELL_INIT_DELAY {
                return;
            }

            let init_commands = if session.shell == "zsh" {
                "export PROMPT=$'%{\\e[38;2;198;208;214m%}|~| %n@%m:%~%# %{\\e[0m%}'\n\
                 command_not_found_handler() { printf \"\\033]777;CommandNotFound;%s\\033\\\\\" \"$1\"; return 127; }\n\
                 clear\n"
            } else {
                "export PS1=$'\\[\\e[38;2;198;208;214m\\]|~| \\u@\\h:\\w\\$ \\[\\e[0m\\]'\n\
                 command_not_found_handle() { printf \"\\033]777;CommandNotFound;%s\\033\\\\\" \"$1\"; return 127; }\n\
                 clear\n"
            };

            session.send_to_pty(init_commands.as_bytes());
            session.init_commands_sent_at = Some(std::time::Instant::now());
            return;
        }

        // Phase 2: Enable rendering after init commands have been processed
        if let Some(sent_at) = session.init_commands_sent_at
            && sent_at.elapsed() < timing::INIT_COMMANDS_DELAY
        {
            return;
        }

        session.shell_initialized = true;
        tracing::info!(
            "Session {}: Shell initialized with custom prompt",
            session_id
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_empty_state() -> AppState {
        AppState {
            sessions: HashMap::new(),
            active_session_id: 0,
            next_session_id: 0,
            current_input_buffer: String::new(),
            current_command_buffer: String::new(),
            should_quit: false,
        }
    }

    #[test]
    fn test_close_not_found() {
        let mut state = create_empty_state();
        let mut tiles = None;
        let mut session_tile_ids = HashMap::new();

        let result = SessionManager::close(&mut state, &mut tiles, &mut session_tile_ids, 999);
        assert_eq!(result, CloseResult::NotFound);
    }

    #[test]
    fn test_handle_post_close_empty() {
        let mut state = create_empty_state();
        let result = SessionManager::handle_post_close(&mut state);
        assert_eq!(result, CloseResult::LastSessionClosed);
    }
}
