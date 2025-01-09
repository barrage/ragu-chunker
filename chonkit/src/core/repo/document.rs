use super::Atomic;
use crate::{
    core::{
        chunk::ChunkConfig,
        document::parser::ParseConfig,
        model::{
            document::{
                config::{DocumentChunkConfig, DocumentParseConfig},
                Document, DocumentConfig, DocumentDisplay, DocumentInsert, DocumentUpdate,
            },
            List, PaginationSort,
        },
    },
    error::ChonkitError,
};
use uuid::Uuid;

/// Keep tracks of documents and their chunking/parsing configurations.
/// Info obtained from here is usually used to load files.
#[async_trait::async_trait]
pub trait DocumentRepo {
    /// Get document metadata based on ID.
    ///
    /// * `id`: Document ID.
    async fn get_document_by_id(&self, id: uuid::Uuid) -> Result<Option<Document>, ChonkitError>;

    /// Get full document configuration based on ID (including chunker and parser).
    ///
    /// * `id`: Document ID.
    async fn get_document_config_by_id(
        &self,
        id: uuid::Uuid,
    ) -> Result<Option<DocumentConfig>, ChonkitError>;

    /// Get document metadata by path and source.
    ///
    /// * `path`: Document path.
    async fn get_document_by_path(
        &self,
        path: &str,
        src: &str,
    ) -> Result<Option<Document>, ChonkitError>;

    /// Get a documents's path. A document path can also be a URL,
    /// depending on the storage.
    ///
    /// * `id`: Document ID.
    async fn get_document_path(&self, id: uuid::Uuid) -> Result<Option<String>, ChonkitError>;

    /// Get a document by its content hash.
    ///
    /// * `hash`: Document content hash.
    async fn get_document_by_hash(&self, hash: &str) -> Result<Option<Document>, ChonkitError>;

    async fn get_document_count(&self) -> Result<usize, ChonkitError>;

    /// List documents with limit and offset
    ///
    /// * `p`: Pagination params.
    async fn list_documents(
        &self,
        p: PaginationSort,
        src: Option<&str>,
        ready: Option<bool>,
    ) -> Result<List<Document>, ChonkitError>;

    /// List all document paths from the repository based on the source.
    /// Returns a list of tuples of document ID and their path.
    async fn list_all_document_paths(&self, src: &str)
        -> Result<Vec<(Uuid, String)>, ChonkitError>;

    /// List documents with limit and offset with additional relations for embeddings.
    ///
    /// * `p`: Pagination params.
    /// * `src`: Optional source to filter by.
    /// * `document_id`: Optional document ID to filter by.
    async fn list_documents_with_collections(
        &self,
        p: PaginationSort,
        src: Option<&str>,
        document_id: Option<Uuid>,
    ) -> Result<List<DocumentDisplay>, ChonkitError>;

    /// Insert document metadata.
    ///
    /// * `document`: Insert payload.
    async fn insert_document(&self, document: DocumentInsert<'_>)
        -> Result<Document, ChonkitError>;

    /// Update document metadata.
    ///
    /// * `id`: Document ID.
    /// * `document`: Update payload.
    async fn update_document(
        &self,
        id: uuid::Uuid,
        document: DocumentUpdate<'_>,
    ) -> Result<u64, ChonkitError>;

    /// Remove document metadata by id.
    ///
    /// * `id`: Document ID.
    async fn remove_document_by_id(
        &self,
        id: uuid::Uuid,
        tx: Option<&mut Self::Tx>,
    ) -> Result<u64, ChonkitError>
    where
        Self: Atomic;

    /// Remove document metadata by path.
    ///
    /// * `path`: Document path.
    async fn remove_document_by_path(&self, path: &str) -> Result<u64, ChonkitError>;

    /// Get the document's configuration for chunking.
    ///
    /// * `id`: Document ID.
    async fn get_document_chunk_config(
        &self,
        id: uuid::Uuid,
    ) -> Result<Option<DocumentChunkConfig>, ChonkitError>;

    /// Get the document's configuration for parsing.
    ///
    ///
    /// * `id`: Document ID.
    async fn get_document_parse_config(
        &self,
        id: uuid::Uuid,
    ) -> Result<Option<DocumentParseConfig>, ChonkitError>;

    /// Insert or update the document's configuration for chunking.
    ///
    /// * `document_id`: Document ID.
    /// * `chunker`: Chunking configuration.
    async fn upsert_document_chunk_config(
        &self,
        document_id: uuid::Uuid,
        chunker: ChunkConfig,
    ) -> Result<DocumentChunkConfig, ChonkitError>;

    /// Insert or update the document's configuration for parsing.
    ///
    /// * `document_id`: Document ID.
    /// * `config`: Parsing configuration.
    async fn upsert_document_parse_config(
        &self,
        document_id: uuid::Uuid,
        config: ParseConfig,
    ) -> Result<DocumentParseConfig, ChonkitError>;

    /// Insert document metadata and the configurations for parsing and chunking in a transaction.
    ///
    /// * `document`: Document insert payload.
    /// * `parse_config`: Parsing configuration.
    /// * `chunk_config`: Chunking configuration.
    /// * `tx`: The transaction to run in.
    async fn insert_document_with_configs(
        &self,
        document: DocumentInsert<'_>,
        parse_config: ParseConfig,
        chunk_config: ChunkConfig,
        tx: &mut <Self as Atomic>::Tx,
    ) -> Result<DocumentConfig, ChonkitError>
    where
        Self: Atomic;

    /// Get all the collection name and provider pairs which contain this document.
    /// Returns a list of tuples of collection name and provider.
    ///
    /// * `document_id`: Document ID.
    async fn get_document_assigned_collection_names(
        &self,
        document_id: Uuid,
    ) -> Result<Vec<(String, String)>, ChonkitError>;
}
