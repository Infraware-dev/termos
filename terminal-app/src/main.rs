//! Infraware Terminal - A hybrid command interpreter with AI assistance.
//!
//! This terminal uses egui for rendering and portable-pty for command execution.
//! All commands are sent to bash/zsh via PTY. When "command not found" is detected,
//! the input is sent to the LLM backend for assistance.

mod app;
mod llm;
mod pty;
mod state;
mod ui;

use app::InfrawareApp;
use eframe::egui::ViewportBuilder;

fn main() -> eframe::Result<()> {
    // Initialize logging
    env_logger::init();

    // Window options
    let options = eframe::NativeOptions {
        viewport: ViewportBuilder::default()
            .with_title("Infraware Terminal")
            .with_inner_size([800.0, 600.0])
            .with_min_inner_size([400.0, 300.0]),
        ..Default::default()
    };

    // Run the application
    eframe::run_native(
        "Infraware Terminal",
        options,
        Box::new(|cc| Ok(Box::new(InfrawareApp::new(cc)))),
    )
}
