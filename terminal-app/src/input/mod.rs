//! Input handling module.

mod classifier;
mod keyboard;
mod selection;

pub use classifier::{InputClassifier, InputType};
pub use keyboard::{KeyboardAction, KeyboardHandler};
pub use selection::TextSelection;
