#![expect(clippy::mod_module_files, reason = "intent is a multi-file module requiring directory structure")]
mod regex_intent;

pub use regex_intent::RegexIntentGenerator;
