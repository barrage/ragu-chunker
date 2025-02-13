use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

/// Embedding information model. Represents the existence of a document in a collection.
#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Embedding {
    /// Primary key.
    pub id: uuid::Uuid,

    /// Which document these embeddings belong to.
    pub document_id: uuid::Uuid,

    /// Collection name.
    pub collection_id: uuid::Uuid,

    pub created_at: DateTime<Utc>,

    pub updated_at: DateTime<Utc>,
}

/// DTO for inserting.
#[derive(Debug)]
pub struct EmbeddingInsert {
    pub id: Uuid,
    pub document_id: Uuid,
    pub collection_id: Uuid,
}

impl EmbeddingInsert {
    pub fn new(document_id: Uuid, collection_id: Uuid) -> Self {
        Self {
            id: uuid::Uuid::new_v4(),
            document_id,
            collection_id,
        }
    }
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct EmbeddingReport {
    pub id: i32,
    pub ty: String,
    pub document_id: Option<Uuid>,
    pub document_name: String,
    pub collection_id: Option<Uuid>,
    pub collection_name: String,
    /// Only Some if ty == addition
    pub model_used: Option<String>,
    /// Only Some if ty == addition
    pub vector_db: Option<String>,
    /// Only Some if ty == addition
    pub embedding_provider: Option<String>,
    /// Only Some if ty == addition
    pub total_vectors: Option<i32>,
    /// Only Some if ty == addition
    pub tokens_used: Option<i32>,
    pub started_at: chrono::DateTime<chrono::Utc>,
    pub finished_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize)]
pub struct EmbeddingReportInsert {
    pub document_id: Uuid,
    pub document_name: String,
    pub collection_id: Uuid,
    pub collection_name: String,
    pub model_used: String,
    pub embedding_provider: String,
    pub vector_db: String,
    pub total_vectors: usize,
    pub tokens_used: Option<usize>,
    pub started_at: chrono::DateTime<chrono::Utc>,
    pub finished_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize)]
pub struct EmbeddingRemovalReportInsert {
    pub document_id: Uuid,
    pub document_name: String,
    pub collection_id: Uuid,
    pub collection_name: String,
    pub started_at: chrono::DateTime<chrono::Utc>,
    pub finished_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug)]
pub struct EmbeddingReportBuilder {
    document_id: Uuid,
    document_name: String,
    collection_id: Uuid,
    collection_name: String,
    model_used: Option<String>,
    embedding_provider: Option<String>,
    vector_db: Option<String>,
    total_vectors: Option<usize>,
    tokens_used: Option<usize>,
    started_at: chrono::DateTime<chrono::Utc>,
    finished_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl EmbeddingReportBuilder {
    pub fn new(
        document_id: Uuid,
        document_name: String,
        collection_id: Uuid,
        collection_name: String,
    ) -> Self {
        Self {
            document_id,
            document_name,
            collection_id,
            collection_name,
            started_at: chrono::Utc::now(),
            embedding_provider: None,
            model_used: None,
            vector_db: None,
            total_vectors: None,
            tokens_used: None,
            finished_at: None,
        }
    }

    pub fn model_used(mut self, model_used: String) -> Self {
        self.model_used = Some(model_used);
        self
    }

    pub fn vector_db(mut self, vector_db: String) -> Self {
        self.vector_db = Some(vector_db);
        self
    }

    pub fn embedding_provider(mut self, embedding_provider: String) -> Self {
        self.embedding_provider = Some(embedding_provider);
        self
    }

    pub fn finished_at(mut self, finished_at: chrono::DateTime<chrono::Utc>) -> Self {
        self.finished_at = Some(finished_at);
        self
    }

    pub fn total_vectors(mut self, total_chunks: usize) -> Self {
        self.total_vectors = Some(total_chunks);
        self
    }

    pub fn tokens_used(mut self, tokens_used: Option<usize>) -> Self {
        self.tokens_used = tokens_used;
        self
    }

    pub fn build(self) -> EmbeddingReportInsert {
        EmbeddingReportInsert {
            document_id: self.document_id,
            document_name: self.document_name,
            collection_id: self.collection_id,
            collection_name: self.collection_name,
            model_used: self.model_used.unwrap(),
            embedding_provider: self.embedding_provider.unwrap(),
            vector_db: self.vector_db.unwrap(),
            total_vectors: self.total_vectors.unwrap(),
            tokens_used: self.tokens_used,
            started_at: self.started_at,
            finished_at: self.finished_at.unwrap(),
        }
    }
}

#[derive(Debug)]
pub struct EmbeddingRemovalReportBuilder {
    document_id: Uuid,
    document_name: String,
    collection_id: Uuid,
    collection_name: String,
    total_vectors_removed: usize,
    started_at: chrono::DateTime<chrono::Utc>,
    finished_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl EmbeddingRemovalReportBuilder {
    pub fn new(
        document_id: Uuid,
        document_name: String,
        collection_id: Uuid,
        collection_name: String,
    ) -> Self {
        Self {
            document_id,
            document_name,
            collection_id,
            collection_name,
            total_vectors_removed: 0,
            started_at: chrono::Utc::now(),
            finished_at: None,
        }
    }

    pub fn total_vectors_removed(mut self, total_vectors_removed: usize) -> Self {
        self.total_vectors_removed = total_vectors_removed;
        self
    }

    pub fn finished_at(mut self, finished_at: chrono::DateTime<chrono::Utc>) -> Self {
        self.finished_at = Some(finished_at);
        self
    }

    pub fn build(self) -> EmbeddingRemovalReportInsert {
        EmbeddingRemovalReportInsert {
            document_id: self.document_id,
            document_name: self.document_name,
            collection_id: self.collection_id,
            collection_name: self.collection_name,
            started_at: self.started_at,
            finished_at: self.finished_at.unwrap(),
        }
    }
}
