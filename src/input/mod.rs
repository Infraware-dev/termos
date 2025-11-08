pub mod classifier;
pub mod handler;
pub mod parser;

pub use classifier::{InputClassifier, InputType};

// Re-export handler types for external use (M2/M3)
#[allow(unused_imports)]
pub use handler::{
    ClassifierChain, CommandSyntaxHandler, DefaultHandler, EmptyInputHandler, InputHandler,
    KnownCommandHandler, NaturalLanguageHandler,
};
