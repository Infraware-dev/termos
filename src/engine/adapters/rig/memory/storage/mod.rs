#![expect(
    clippy::mod_module_files,
    reason = "storage is a multi-file module requiring directory structure"
)]
mod jsonl;

pub use jsonl::JsonlStorage;
