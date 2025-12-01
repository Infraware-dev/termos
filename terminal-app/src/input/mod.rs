pub mod application_builtins;
pub mod classifier;
pub mod discovery;
pub mod handler;
pub mod history_expansion;
pub mod known_commands;
pub mod parser;
pub mod patterns;
pub mod shell_builtins;
pub mod typo_detection;

pub use classifier::{InputClassifier, InputType};
pub use handler::HandlerPosition;

// Re-export handler types for external use (M2/M3)
