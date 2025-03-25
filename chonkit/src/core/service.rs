//! Business logic.

pub mod collection;
pub mod document;
pub mod embedding;
pub mod external;

#[derive(Clone)]
pub struct ServiceState<Cache> {
    pub document: document::DocumentService,

    pub collection: collection::CollectionService,

    pub external: external::ServiceFactory,

    pub embedding: embedding::EmbeddingService<Cache>,
}
