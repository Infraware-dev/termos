#![expect(
    clippy::mod_module_files,
    reason = "embeddings is a multi-file module requiring directory structure"
)]
mod noop;

pub use noop::NoopEmbedding;
