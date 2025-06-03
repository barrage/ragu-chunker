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
    /// Parsing configuration.
    pub parser: Option<ParseConfig>,

    /// Chunking configuration.
    pub chunker: Option<ChunkConfig>,
}

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
