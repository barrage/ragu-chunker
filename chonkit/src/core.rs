//! The core module defines the business logic of chonkit.
//! It provides the traits and models upstream adapters need to implement.

/// [chunx] wrappers for executing chunkers and managing and storing chunk configurations.
pub mod chunk;

/// Document processing functionality.
pub mod document;

/// Embedding interfaces and data structs.
pub mod embeddings;

/// Embedding cache.
pub mod cache;

/// Database models.
pub mod model;

/// Provider infrastructure.
pub mod provider;

/// Database implementation.
pub mod repo;

/// High level chonkit APIs.
pub mod service;

/// Vector DB interfaces.
pub mod vector;

/// Tokenizer utilities.
pub mod token;

/// Utility macro for timing short expressions so we don't polute the codebase.
///
/// Logs the amount of milliseconds the expression took to complete.
#[macro_export]
macro_rules! timed {
    ($msg:literal, $expr:expr) => {{
        let start = std::time::Instant::now();
        let result = $expr;
        let elapsed = start.elapsed().as_millis();
        tracing::debug!("{} ({elapsed}ms)", $msg);
        result
    }};
}
