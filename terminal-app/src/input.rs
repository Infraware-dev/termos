//! Input handling module.

mod classifier;
mod command_validator;
mod keyboard;
mod output_capture;
mod prompt_detector;
mod selection;

pub use classifier::{InputClassifier, InputType};
pub use command_validator::{ValidationResult, validate_command};
pub use keyboard::{KeyboardAction, KeyboardHandler};
pub use output_capture::OutputCapture;
pub use prompt_detector::PromptDetector;
pub use selection::{SelectionPoint, TextSelection};
