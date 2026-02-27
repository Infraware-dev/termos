//! Terminal emulation module with VTE parser and grid rendering.
//!
//! This module provides full terminal emulation including:
//! - ANSI escape sequence parsing via VTE
//! - Terminal grid with cursor positioning
//! - Color and attribute support
//! - Alternate screen buffer for vim/less
//! - Scroll regions

pub mod cell;
pub mod grid;
pub mod handler;

pub use handler::TerminalHandler;
