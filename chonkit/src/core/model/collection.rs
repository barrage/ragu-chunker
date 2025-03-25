use crate::search_column;

use super::document::DocumentShort;
use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::prelude::FromRow;
use uuid::Uuid;

/// Vector collection model.
#[derive(Debug, Serialize, FromRow, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Collection {
    /// Primary key.
    pub id: Uuid,
    /// Collection name. Unique in combination with provider.
    pub name: String,
    /// Embedding model used for the collection.
    pub model: String,
    /// Embedding provider ID.
    pub embedder: String,
    /// Vector database provider ID.
    pub provider: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

search_column! {
    CollectionSearchColumn,
    Name => "name",
    Model => "model",
    Embedder => "embedder",
    Provider => "provider",
}

pub struct CollectionInsert<'a> {
    pub id: Uuid,
    pub name: &'a str,
    pub model: &'a str,
    pub embedder: &'a str,
    pub provider: &'a str,
}

impl<'a> CollectionInsert<'a> {
    pub fn new(name: &'a str, model: &'a str, embedder: &'a str, provider: &'a str) -> Self {
        Self {
            id: Uuid::new_v4(),
            name,
            model,
            embedder,
            provider,
        }
    }
}

/// Collection struct for display purposes when listing documents.
#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CollectionShort {
    pub id: Uuid,
    pub name: String,
    pub model: String,
    pub embedder: String,
    pub provider: String,
}

impl CollectionShort {
    pub fn new(id: Uuid, name: String, model: String, embedder: String, provider: String) -> Self {
        Self {
            id,
            name,
            model,
            embedder,
            provider,
        }
    }
}

/// Aggregate version of [Collection] with the documents it contains.
#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CollectionDisplay {
    pub collection: Collection,
    pub total_documents: usize,
    pub documents: Vec<DocumentShort>,
}

impl CollectionDisplay {
    pub fn new(
        collection: Collection,
        total_documents: usize,
        documents: Vec<DocumentShort>,
    ) -> Self {
        Self {
            collection,
            total_documents,
            documents,
        }
    }
}
