//! The core module defines the business logic of chonkit.
//! It provides the traits and models upstream adapters need to implement.

/// Standard OAuth structs.
pub mod auth;

/// [chunx] wrappers for executing chunkers and managing and storing chunk configurations.
pub mod chunk;

/// Document processing functionality.
pub mod document;

pub mod embedder;
pub mod model;
pub mod provider;
pub mod repo;
pub mod service;
pub mod vector;
