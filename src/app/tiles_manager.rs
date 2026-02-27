//! Split view and tab management using egui_tiles.
//!
//! Provides `TilesManager` for managing terminal pane splits and tabs.
//! This module encapsulates egui_tiles operations.

use std::collections::HashMap;

use egui_tiles::{Container, LinearDir, Tile, TileId, Tiles, Tree};

use crate::session::SessionId;

/// Manages egui_tiles tree for split views and tabs.
///
/// Provides static methods for pane splitting, tab creation, and navigation.
pub struct TilesManager;

impl TilesManager {
    /// Splits a pane in the specified direction.
    ///
    /// Creates a container holding [active_pane, new_pane] and replaces
    /// the active pane in the tree.
    pub fn split(
        tiles: &mut Option<Tree<SessionId>>,
        session_tile_ids: &mut HashMap<SessionId, TileId>,
        active_session_id: SessionId,
        new_session_id: SessionId,
        direction: LinearDir,
    ) {
        let Some(&active_tile_id) = session_tile_ids.get(&active_session_id) else {
            tracing::warn!(
                "Split failed: no tile found for active session {}",
                active_session_id
            );
            return;
        };

        let Some(tree) = tiles else {
            return;
        };

        // Get parent BEFORE inserting new container
        let parent_id = tree.tiles.parent_of(active_tile_id);

        // Insert the new pane
        let new_pane_id = tree.tiles.insert_pane(new_session_id);
        session_tile_ids.insert(new_session_id, new_pane_id);

        // Create container holding [active_pane, new_pane]
        let container = egui_tiles::Linear::new(direction, vec![active_tile_id, new_pane_id]);
        let container_id = tree.tiles.insert_container(container);

        // Replace active pane with container in the tree
        if let Some(parent_id) = parent_id {
            if let Some(Tile::Container(parent_container)) = tree.tiles.get_mut(parent_id) {
                Self::replace_child_in_container(parent_container, active_tile_id, container_id);
            }
        } else {
            // Active pane is root - make container the new root
            *tree = Tree::new(
                "terminal_tiles",
                container_id,
                std::mem::take(&mut tree.tiles),
            );
        }

        let dir_name = match direction {
            LinearDir::Horizontal => "horizontal",
            LinearDir::Vertical => "vertical",
        };
        tracing::info!(
            "Split {}: session {} in pane {:?}, container {:?}",
            dir_name,
            new_session_id,
            new_pane_id,
            container_id
        );
    }

    /// Creates a new tab at the root level.
    ///
    /// Returns the tile ID of the new pane if successful.
    pub fn create_tab(
        tiles: &mut Option<Tree<SessionId>>,
        session_tile_ids: &mut HashMap<SessionId, TileId>,
        new_session_id: SessionId,
    ) -> Option<TileId> {
        let tree = tiles.as_mut()?;
        let root_id = tree.root()?;

        let new_pane_id = tree.tiles.insert_pane(new_session_id);
        session_tile_ids.insert(new_session_id, new_pane_id);

        // If root is already a Tabs container, add the new pane there
        if let Some(Tile::Container(Container::Tabs(tabs))) = tree.tiles.get_mut(root_id) {
            tabs.children.push(new_pane_id);
            tracing::info!(
                "Added tab to root tabs container, session {}, tile {:?}",
                new_session_id,
                new_pane_id
            );
            return Some(new_pane_id);
        }

        // Root is not a Tabs container - wrap it in a new Tabs container
        let tabs = egui_tiles::Tabs::new(vec![root_id, new_pane_id]);
        let container_id = tree.tiles.insert_container(Container::Tabs(tabs));

        // Make the new Tabs container the root
        *tree = Tree::new(
            "terminal_tiles",
            container_id,
            std::mem::take(&mut tree.tiles),
        );

        tracing::info!(
            "Created root tab group, session {}, tile {:?}",
            new_session_id,
            new_pane_id
        );
        Some(new_pane_id)
    }

    /// Switches to the next tab at root level.
    ///
    /// Returns the session ID of the newly active tab.
    pub fn next_tab(tiles: &mut Option<Tree<SessionId>>) -> Option<SessionId> {
        let tree = tiles.as_mut()?;
        let root_id = tree.root()?;

        if let Some(Tile::Container(Container::Tabs(tabs))) = tree.tiles.get_mut(root_id)
            && let Some(current_idx) = tabs
                .active
                .and_then(|active| tabs.children.iter().position(|&id| id == active))
        {
            let next_idx = (current_idx + 1) % tabs.children.len();
            let next_tile_id = tabs.children[next_idx];
            tabs.active = Some(next_tile_id);

            // Find session in the next tab
            if let Some(tree_ref) = tiles.as_ref()
                && let Some(session_id) =
                    Self::find_first_pane_session(&tree_ref.tiles, next_tile_id)
            {
                tracing::debug!("Switched to next tab, session {}", session_id);
                return Some(session_id);
            }
        }
        None
    }

    /// Switches to the previous tab at root level.
    ///
    /// Returns the session ID of the newly active tab.
    pub fn prev_tab(tiles: &mut Option<Tree<SessionId>>) -> Option<SessionId> {
        let tree = tiles.as_mut()?;
        let root_id = tree.root()?;

        if let Some(Tile::Container(Container::Tabs(tabs))) = tree.tiles.get_mut(root_id)
            && let Some(current_idx) = tabs
                .active
                .and_then(|active| tabs.children.iter().position(|&id| id == active))
        {
            let prev_idx = if current_idx == 0 {
                tabs.children.len() - 1
            } else {
                current_idx - 1
            };
            let prev_tile_id = tabs.children[prev_idx];
            tabs.active = Some(prev_tile_id);

            // Find session in the prev tab
            if let Some(tree_ref) = tiles.as_ref()
                && let Some(session_id) =
                    Self::find_first_pane_session(&tree_ref.tiles, prev_tile_id)
            {
                tracing::debug!("Switched to prev tab, session {}", session_id);
                return Some(session_id);
            }
        }
        None
    }

    /// Finds the first pane's session ID within a tile (recursively searches containers).
    pub fn find_first_pane_session(tiles: &Tiles<SessionId>, tile_id: TileId) -> Option<SessionId> {
        match tiles.get(tile_id)? {
            Tile::Pane(session_id) => Some(*session_id),
            Tile::Container(container) => {
                let first_child = match container {
                    Container::Tabs(tabs) => tabs.children.first().copied(),
                    Container::Linear(linear) => linear.children.first().copied(),
                    Container::Grid(grid) => grid.children().next().copied(),
                };
                first_child.and_then(|child_id| Self::find_first_pane_session(tiles, child_id))
            }
        }
    }

    /// Replaces a child tile ID in a container's children list.
    fn replace_child_in_container(container: &mut Container, old_child: TileId, new_child: TileId) {
        match container {
            Container::Linear(linear) => {
                for child in &mut linear.children {
                    if *child == old_child {
                        *child = new_child;
                        return;
                    }
                }
            }
            Container::Tabs(tabs) => {
                let was_active = tabs.active == Some(old_child);

                for child in &mut tabs.children {
                    if *child == old_child {
                        *child = new_child;
                        if was_active {
                            tabs.active = Some(new_child);
                        }
                        return;
                    }
                }
            }
            Container::Grid(grid) => {
                let idx = grid.children().position(|&c| c == old_child);
                if let Some(idx) = idx {
                    let _ = grid.replace_at(idx, new_child);
                    return;
                }
            }
        }
        tracing::warn!(
            "Failed to replace child {:?} with {:?} in container",
            old_child,
            new_child
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_single_pane_tree() -> (Tree<SessionId>, HashMap<SessionId, TileId>) {
        let mut tiles = Tiles::default();
        let pane_id = tiles.insert_pane(0);
        let tree = Tree::new("test", pane_id, tiles);
        let mut session_tile_ids = HashMap::new();
        session_tile_ids.insert(0, pane_id);
        (tree, session_tile_ids)
    }

    #[test]
    fn test_find_first_pane_session_single_pane() {
        let (tree, _) = create_single_pane_tree();
        let root_id = tree.root().unwrap();

        let session = TilesManager::find_first_pane_session(&tree.tiles, root_id);
        assert_eq!(session, Some(0));
    }

    #[test]
    fn test_split_horizontal() {
        let (tree, mut session_tile_ids) = create_single_pane_tree();
        let mut tiles = Some(tree);

        TilesManager::split(
            &mut tiles,
            &mut session_tile_ids,
            0,
            1,
            LinearDir::Horizontal,
        );

        assert_eq!(session_tile_ids.len(), 2);
        assert!(session_tile_ids.contains_key(&0));
        assert!(session_tile_ids.contains_key(&1));

        let tree = tiles.unwrap();
        let root_id = tree.root().unwrap();

        // Root should now be a container
        assert!(matches!(
            tree.tiles.get(root_id),
            Some(Tile::Container(Container::Linear(_)))
        ));
    }

    #[test]
    fn test_create_tab_from_single_pane() {
        let (tree, mut session_tile_ids) = create_single_pane_tree();
        let mut tiles = Some(tree);

        let new_tile = TilesManager::create_tab(&mut tiles, &mut session_tile_ids, 1);
        assert!(new_tile.is_some());

        let tree = tiles.unwrap();
        let root_id = tree.root().unwrap();

        // Root should now be a Tabs container
        assert!(matches!(
            tree.tiles.get(root_id),
            Some(Tile::Container(Container::Tabs(_)))
        ));
    }

    #[test]
    fn test_create_tab_adds_to_existing_tabs() {
        let (tree, mut session_tile_ids) = create_single_pane_tree();
        let mut tiles = Some(tree);

        // Create first tab (wraps root in Tabs)
        TilesManager::create_tab(&mut tiles, &mut session_tile_ids, 1);
        // Create second tab (adds to existing Tabs)
        TilesManager::create_tab(&mut tiles, &mut session_tile_ids, 2);

        let tree = tiles.unwrap();
        let root_id = tree.root().unwrap();

        if let Some(Tile::Container(Container::Tabs(tabs))) = tree.tiles.get(root_id) {
            assert_eq!(tabs.children.len(), 3); // Original + 2 new tabs
        } else {
            panic!("Expected Tabs container");
        }
    }

    #[test]
    fn test_next_tab() {
        let (tree, mut session_tile_ids) = create_single_pane_tree();
        let mut tiles = Some(tree);

        TilesManager::create_tab(&mut tiles, &mut session_tile_ids, 1);
        TilesManager::create_tab(&mut tiles, &mut session_tile_ids, 2);

        // Set active to first tab
        if let Some(ref mut tree) = tiles
            && let Some(root_id) = tree.root()
            && let Some(Tile::Container(Container::Tabs(tabs))) = tree.tiles.get_mut(root_id)
        {
            tabs.active = tabs.children.first().copied();
        }

        let next_session = TilesManager::next_tab(&mut tiles);
        assert!(next_session.is_some());
    }

    #[test]
    fn test_prev_tab() {
        let (tree, mut session_tile_ids) = create_single_pane_tree();
        let mut tiles = Some(tree);

        TilesManager::create_tab(&mut tiles, &mut session_tile_ids, 1);
        TilesManager::create_tab(&mut tiles, &mut session_tile_ids, 2);

        // Set active to last tab
        if let Some(ref mut tree) = tiles
            && let Some(root_id) = tree.root()
            && let Some(Tile::Container(Container::Tabs(tabs))) = tree.tiles.get_mut(root_id)
        {
            tabs.active = tabs.children.last().copied();
        }

        let prev_session = TilesManager::prev_tab(&mut tiles);
        assert!(prev_session.is_some());
    }
}
