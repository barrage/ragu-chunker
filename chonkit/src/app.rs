//! Module containing concrete implementations from the [core](crate::core) module.

/// Batch embedder implementation.
pub mod batch;

/// Document storage implementations.
pub mod document;

/// Text embedder implementations.
pub mod embedder;

/// Application state configuration.
pub mod state;

/// Vector database implementations.
pub mod vector;

/// HTTP server implementation.
pub mod server;

/// Authentication implementations.
pub mod auth;

/// External API implementations.
pub mod external;

/// Embedding cache implementations.
pub mod cache;

#[cfg(test)]
pub mod test;
