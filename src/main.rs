//! Infraware Terminal - A hybrid command interpreter with AI assistance.
//!
//! This terminal uses egui for rendering and portable-pty for command execution.
//! All commands are sent to bash/zsh via PTY. When "command not found" is detected,
//! the input is sent to the LLM backend for assistance.

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

mod agent;
mod app;
mod args;
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
use clap::Parser;
use eframe::egui::{IconData, ViewportBuilder};

use crate::app::AppOptions;
use crate::args::Args;

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

    // load cli arguments (also sets up logging based on args)
    let args = Args::parse();

    // Initialize logging with sensible defaults
    // Priority: RUST_LOG > LOG_LEVEL > default (info)
    let filter = format!("infraware_terminal={}", args.log_level);

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

    let app_options = app_options(&args);
    // Run the application
    eframe::run_native(
        "Infraware Terminal",
        options,
        Box::new(|cc| Ok(Box::new(InfrawareApp::new(cc, app_options)))),
    )
}

fn app_options(_args: &Args) -> AppOptions {
    #[cfg(feature = "arena")]
    if let Some(scenario) = _args.arena {
        return AppOptions {
            api_key: _args.api_key.clone(),
            pty_provider: app::PtyProviderType::ArenaScenario(scenario),
        };
    }

    #[cfg(feature = "pty-test_container")]
    let pty_provider = if _args.use_pty_test_container {
        let (image, tag) = test_container_image_and_tag(_args);
        app::PtyProviderType::TestContainer { image, tag }
    } else {
        app::PtyProviderType::Local
    };
    #[cfg(not(feature = "pty-test_container"))]
    let pty_provider = app::PtyProviderType::Local;

    AppOptions {
        api_key: _args.api_key.clone(),
        pty_provider,
    }
}

/// Splits an image reference into `(image, tag)`.
///
/// Handles registry ports (e.g., `registry.example.com:5000/myimage:v1`)
/// by treating only the last colon as the tag separator when the segment
/// after it contains no `/`.
#[cfg(feature = "pty-test_container")]
fn test_container_image_and_tag(args: &Args) -> (String, String) {
    let input = &args.pty_test_container_image;
    // The tag separator is the last `:` whose right-hand side contains no `/`.
    if let Some(pos) = input.rfind(':') {
        let after_colon = &input[pos + 1..];
        if !after_colon.contains('/') && !after_colon.is_empty() {
            return (input[..pos].to_string(), after_colon.to_string());
        }
    }
    (input.to_string(), "latest".to_string())
}
