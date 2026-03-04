//! Infraware Terminal - A hybrid command interpreter with AI assistance.
//!
//! This terminal uses egui for rendering and portable-pty for command execution.
//! All commands are sent to bash/zsh via PTY. When "command not found" is detected,
//! the input is sent to the LLM backend for assistance.

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

mod agent;
mod app;
mod config;
mod input;
mod markdown;
mod orchestrators;
mod pty;
mod session;
mod state;
mod terminal;
mod ui;

use std::sync::atomic::{AtomicBool, Ordering};

use app::InfrawareApp;
use eframe::egui::{IconData, ViewportBuilder};

/// Load the application icon from embedded PNG.
fn load_icon() -> Option<IconData> {
    let icon_bytes = include_bytes!("../resources/logo-corner.png");
    let image = image::load_from_memory(icon_bytes).ok()?.into_rgba8();
    let (width, height) = image.dimensions();
    Some(IconData {
        rgba: image.into_raw(),
        width,
        height,
    })
}

/// Global flag set when SIGINT (Ctrl+C) is received from the system
pub static SIGINT_RECEIVED: AtomicBool = AtomicBool::new(false);

fn main() -> eframe::Result<()> {
    // Load environment variables from .env file (if present)
    dotenvy::dotenv().ok();
    // Load secrets from .env.secrets file (if present)
    dotenvy::from_filename(".env.secrets").ok();

    // Initialize logging with sensible defaults
    // Priority: RUST_LOG > LOG_LEVEL > default (info)
    let filter = std::env::var("RUST_LOG").unwrap_or_else(|_| {
        std::env::var("LOG_LEVEL")
            .map(|l| format!("infraware_terminal={}", l))
            .unwrap_or_else(|_| "infraware_terminal=info".to_string())
    });

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(true)
        .init();

    // Set up Ctrl+C handler - intercepts SIGINT and sets flag
    // This works even when egui doesn't receive the key event
    ctrlc::set_handler(|| {
        tracing::info!("SIGINT received from system");
        SIGINT_RECEIVED.store(true, Ordering::SeqCst);
    })
    .expect("Error setting Ctrl+C handler");

    // Window options
    let icon = load_icon();
    let mut viewport = ViewportBuilder::default()
        .with_title("Infraware Terminal")
        .with_inner_size([800.0, 600.0])
        .with_min_inner_size([400.0, 300.0]);

    if let Some(icon_data) = icon {
        viewport = viewport.with_icon(std::sync::Arc::new(icon_data));
    }

    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    // Run the application
    eframe::run_native(
        "Infraware Terminal",
        options,
        Box::new(|cc| Ok(Box::new(InfrawareApp::new(cc)))),
    )
}
