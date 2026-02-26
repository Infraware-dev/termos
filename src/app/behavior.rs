//! egui_tiles Behavior implementation for terminal panes.
//!
//! Provides `TerminalBehavior` which implements the `egui_tiles::Behavior` trait
//! for rendering and managing terminal panes in the split view.

use egui::Color32;
use egui_tiles::{EditAction, Tiles, UiResponse};

use super::{InfrawareApp, TilesManager};
use crate::session::SessionId;

/// Behavior implementation for egui_tiles.
///
/// Wraps InfrawareApp for rendering terminal panes.
pub struct TerminalBehavior<'a> {
    app: &'a mut InfrawareApp,
    has_focus: bool,
}

impl<'a> TerminalBehavior<'a> {
    /// Creates a new terminal behavior.
    pub fn new(app: &'a mut InfrawareApp, has_focus: bool) -> Self {
        Self { app, has_focus }
    }
}

impl egui_tiles::Behavior<SessionId> for TerminalBehavior<'_> {
    fn tab_title_for_pane(&mut self, pane: &SessionId) -> egui::WidgetText {
        if let Some(session) = self.app.sessions().get(pane) {
            session.cached_title.as_str().into()
        } else {
            format!("Terminal {}", pane).into()
        }
    }

    fn tab_text_color(
        &self,
        _visuals: &egui::Visuals,
        _tiles: &Tiles<SessionId>,
        _tile_id: egui_tiles::TileId,
        state: &egui_tiles::TabState,
    ) -> Color32 {
        if state.active {
            Color32::WHITE
        } else {
            Color32::from_rgb(97, 97, 97)
        }
    }

    fn tab_bg_color(
        &self,
        visuals: &egui::Visuals,
        _tiles: &Tiles<SessionId>,
        _tile_id: egui_tiles::TileId,
        state: &egui_tiles::TabState,
    ) -> Color32 {
        if state.active {
            visuals.window_fill()
        } else {
            Color32::from_rgb(58, 58, 58)
        }
    }

    fn tab_ui(
        &mut self,
        tiles: &mut Tiles<SessionId>,
        ui: &mut egui::Ui,
        _id: egui::Id,
        id: egui_tiles::TileId,
        state: &egui_tiles::TabState,
    ) -> egui::Response {
        let text = self.tab_title_for_tile(tiles, id);
        let text_color = self.tab_text_color(ui.visuals(), tiles, id, state);
        let bg_color = self.tab_bg_color(ui.visuals(), tiles, id, state);

        let text = text.color(text_color);

        let button = if let Some(texture) = self.app.logo_texture() {
            egui::Button::image_and_text(egui::Image::new(texture).max_height(12.0), text)
        } else {
            egui::Button::new(text)
        };

        let button = button.fill(bg_color);
        ui.add(button)
    }

    fn on_tab_button(
        &mut self,
        tiles: &Tiles<SessionId>,
        tile_id: egui_tiles::TileId,
        button_response: egui::Response,
    ) -> egui::Response {
        if button_response.clicked()
            && let Some(session_id) = TilesManager::find_first_pane_session(tiles, tile_id)
            && self.app.active_session_id() != session_id
        {
            self.app.set_active_session_id(session_id);
            tracing::debug!("Tab clicked: switched to session {}", session_id);

            if let Some(session) = self.app.sessions().get(&session_id) {
                button_response
                    .ctx
                    .memory_mut(|mem| mem.request_focus(session.terminal_egui_id));
            }
        }
        button_response
    }

    fn tab_title_for_tile(
        &mut self,
        tiles: &Tiles<SessionId>,
        tile_id: egui_tiles::TileId,
    ) -> egui::WidgetText {
        if let Some(session_id) = TilesManager::find_first_pane_session(tiles, tile_id)
            && let Some(session) = self.app.sessions().get(&session_id)
        {
            return session.cached_title.as_str().into();
        }
        format!("Tab {:?}", tile_id).into()
    }

    fn pane_ui(
        &mut self,
        ui: &mut egui::Ui,
        _tile_id: egui_tiles::TileId,
        pane: &mut SessionId,
    ) -> UiResponse {
        let session_id = *pane;

        egui::Frame::NONE.outer_margin(2.0).show(ui, |ui| {
            let available = ui.available_size();
            let cols = ((available.x / self.app.render.char_width) as u16).max(20);
            let rows = ((available.y / self.app.render.char_height) as u16).max(5);

            let size_changed = self.app.resize_session_pty(session_id, cols, rows);
            if size_changed {
                ui.ctx().request_repaint();
            }

            let is_active = session_id == self.app.active_session_id();
            self.app
                .render_terminal(ui, session_id, self.has_focus && is_active);
        });

        UiResponse::None
    }

    fn gap_width(&self, _style: &egui::Style) -> f32 {
        4.0
    }

    fn simplification_options(&self) -> egui_tiles::SimplificationOptions {
        egui_tiles::SimplificationOptions {
            all_panes_must_have_tabs: false,
            ..Default::default()
        }
    }

    fn on_edit(&mut self, edit_action: EditAction) {
        tracing::debug!("TerminalBehavior::on_edit: {edit_action:?}");
        if matches!(edit_action, EditAction::TabSelected) {
            self.app.set_tab_selection_pending(true);
            tracing::debug!("on_edit: TabSelected event, set tab_selection_pending flag");
        }
    }
}
