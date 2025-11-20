pub mod classifier;
pub mod discovery;
pub mod handler;
pub mod known_commands;
pub mod parser;
pub mod patterns;
pub mod shell_builtins;
pub mod typo_detection;

pub use classifier::{InputClassifier, InputType};

// Re-export handler types for external use (M2/M3)
#[allow(unused_imports)]
pub use handler::{
    ClassifierChain, CommandSyntaxHandler, DefaultHandler, EmptyInputHandler, InputHandler,
    KnownCommandHandler, NaturalLanguageHandler, PathCommandHandler,
};
#[allow(unused_imports)]
pub use shell_builtins::ShellBuiltinHandler;
