//! Business logic.

pub mod document;
pub mod external;
pub mod token;
pub mod vector;

#[derive(Clone)]
pub struct ServiceState {
    pub document: document::DocumentService,

    pub vector: vector::VectorService,

    pub external: external::ServiceFactory,
}
