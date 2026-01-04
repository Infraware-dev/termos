//! Infraware Terminal - A hybrid command interpreter with AI assistance.
//!
//! This terminal uses egui for rendering and portable-pty for command execution.
//! All commands are sent to bash/zsh via PTY. When "command not found" is detected,
//! the input is sent to the LLM backend for assistance.

mod app;
mod config;
mod input;
mod llm;
mod orchestrators;
mod pty;
mod state;
mod terminal;
mod ui;

use app::InfrawareApp;
use eframe::egui::ViewportBuilder;
use std::sync::atomic::{AtomicBool, Ordering};

/// Global flag set when SIGINT (Ctrl+C) is received from the system
pub static SIGINT_RECEIVED: AtomicBool = AtomicBool::new(false);

fn main() -> eframe::Result<()> {
    // Initialize logging
    env_logger::init();

    // Set up Ctrl+C handler - intercepts SIGINT and sets flag
    // This works even when egui doesn't receive the key event
    ctrlc::set_handler(|| {
        log::info!("SIGINT received from system");
        SIGINT_RECEIVED.store(true, Ordering::SeqCst);
    })
    .expect("Error setting Ctrl+C handler");

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
