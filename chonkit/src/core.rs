//! The core module defines the business logic of chonkit.
//! It provides the traits and models upstream adapters need to implement.

/// Standard OAuth structs.
pub mod auth;

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
