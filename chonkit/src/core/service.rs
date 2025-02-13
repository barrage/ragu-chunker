//! Business logic.

pub mod document;
pub mod embedding;
pub mod external;
pub mod token;
pub mod vector;

#[derive(Clone)]
pub struct ServiceState {
    pub document: document::DocumentService,

    pub collection: vector::CollectionService,

    pub external: external::ServiceFactory,

    pub embedding: embedding::EmbeddingService,
}
