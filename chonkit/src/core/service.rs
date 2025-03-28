//! Business logic.

pub mod document;
pub mod embedding;
pub mod external;
pub mod vector;

#[derive(Clone)]
pub struct ServiceState<Cache> {
    pub document: document::DocumentService,

    pub collection: vector::CollectionService,

    pub external: external::ServiceFactory,

    pub embedding: embedding::EmbeddingService<Cache>,
}
