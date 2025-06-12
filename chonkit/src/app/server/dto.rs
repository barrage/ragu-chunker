//! Http specific DTOs.

use crate::core::{
    chunk::ChunkConfig,
    document::parser::ParseConfig,
    model::{
        document::{Document, DocumentSearchColumn},
        Pagination, PaginationSort,
    },
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;
use validify::{schema_err, schema_validation, Validate, ValidationErrors};

// DOCUMENTS

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub(super) struct UploadResult {
    pub documents: Vec<Document>,

    /// Map form keys to errors
    pub errors: HashMap<String, Vec<String>>,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub(super) struct ConfigUpdatePayload {
    /// If provided, the configuration is uploaded in the context of the collection,
    /// i.e. the new config will apply only when parsing/chunking before embedding in that collection.
    ///
    /// If not provided, the new configuration is considered the default configuration when
    /// parsing/chunking before embedding for ALL collections.
    pub collection_id: Option<Uuid>,

    /// Parsing configuration.
    pub parser: Option<ParseConfig>,

    /// Chunking configuration.
    pub chunker: Option<ChunkConfig>,
}

#[derive(Debug, Default, Deserialize, Validate, ToSchema, IntoParams)]
#[serde(rename_all = "camelCase")]
pub(super) struct ListDocumentsPayload {
    /// Limit and offset
    #[validate]
    #[serde(flatten)]
    #[param(inline)]
    pub pagination: PaginationSort<DocumentSearchColumn>,

    /// Filter by file source.
    pub src: Option<String>,

    /// If given and `true`, only return documents that are ready for processing, i.e. that have
    /// their parser and chunker configured.
    pub ready: Option<bool>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub(super) struct UpdateImageDescription {
    pub description: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub(super) struct UpdateDocumentMetadata {
    pub name: Option<String>,
    pub label: Option<String>,
    pub tags: Option<Vec<String>>,
}

// EMBEDDINGS

/// Used for batch embedding of documents.
#[derive(Debug, Deserialize, Validate, ToSchema)]
#[serde(rename_all = "camelCase")]
#[validate(Self::validate_schema)]
pub struct EmbedBatchInput {
    /// The documents to embed and add to the collection.
    pub add: Vec<Uuid>,

    /// The documents to remove from the collection.
    pub remove: Vec<Uuid>,

    /// The ID of the collection in which to store/remove the embeddings to/from.
    pub collection: Uuid,
}

impl EmbedBatchInput {
    #[schema_validation]
    fn validate_schema(&self) -> Result<(), ValidationErrors> {
        if self.add.is_empty() && self.remove.is_empty() {
            schema_err! {
                "no_documents",
                "either `add` or `remove` must contain document IDs"
            }
        }
    }
}

#[derive(Debug, Deserialize, Validate, ToSchema, IntoParams)]
#[serde(rename_all = "camelCase")]
pub(super) struct ListEmbeddingsPayload {
    /// Limit and offset
    #[validate]
    #[serde(flatten)]
    #[param(inline)]
    pub pagination: Option<Pagination>,

    /// Filter by collection.
    pub collection: Option<Uuid>,
}
