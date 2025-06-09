use crate::search_column;
use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::{postgres::PgRow, FromRow, Row};
use uuid::Uuid;

/// The details of adding/removing embeddings from a collection.
#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct EmbeddingReport {
    /// Embedding report primary key.
    pub id: i32,

    /// The data of the report.
    pub report: EmbeddingReportType,
}

impl EmbeddingReport {
    const REPORT_TYPE_COLUMN: &str = "report_type";
}

impl<'r> FromRow<'r, PgRow> for EmbeddingReport {
    fn from_row(row: &'r PgRow) -> Result<Self, sqlx::Error> {
        fn report_base(row: &PgRow) -> Result<EmbeddingReportBase, sqlx::Error> {
            Ok(EmbeddingReportBase {
                collection_id: row.try_get("collection_id")?,
                collection_name: row.try_get("collection_name")?,
                vector_db: row.try_get("vector_db")?,
                started_at: row.try_get("started_at")?,
                finished_at: row.try_get("finished_at")?,
            })
        }

        fn addition_report_base(
            row: &PgRow,
            base: EmbeddingReportBase,
        ) -> Result<EmbeddingAdditionReport, sqlx::Error> {
            Ok(EmbeddingAdditionReport {
                model_used: row.try_get("model_used")?,
                tokens_used: row.try_get("tokens_used")?,
                embedding_provider: row.try_get("embedding_provider")?,
                total_vectors: row.try_get("total_vectors")?,
                cache: row.try_get("cache")?,
                base,
            })
        }

        let report_type = row.try_get::<&str, _>(EmbeddingReport::REPORT_TYPE_COLUMN)?;
        match report_type {
            "addition" => match row.try_get::<&str, _>("type")? {
                "text" => {
                    let document_id = row.try_get("document_id")?;
                    let document_name = row.try_get("document_name")?;
                    let base = report_base(row)?;
                    Ok(EmbeddingReport {
                        id: row.try_get("id")?,
                        report: EmbeddingReportType::TextAddition(TextEmbeddingAdditionReport {
                            document_id,
                            document_name,
                            report: addition_report_base(row, base)?,
                        }),
                    })
                }
                "image" => {
                    let image_id = row.try_get("image_id")?;
                    let base = report_base(row)?;
                    Ok(EmbeddingReport {
                        id: row.try_get("id")?,
                        report: EmbeddingReportType::ImageAddition(ImageEmbeddingAdditionReport {
                            image_id,
                            report: addition_report_base(row, base)?,
                        }),
                    })
                }
                _ => Err(sqlx::Error::InvalidArgument(format!(
                    "Invalid addition type '{}'",
                    row.try_get::<&str, _>("type")?
                ))),
            },
            "removal" => match row.try_get::<&str, _>("type")? {
                "text" => {
                    let document_id = row.try_get("document_id")?;
                    let document_name = row.try_get("document_name")?;
                    let base = report_base(row)?;
                    Ok(EmbeddingReport {
                        id: row.try_get("id")?,
                        report: EmbeddingReportType::TextRemoval(TextEmbeddingRemovalReport {
                            document_id,
                            document_name,
                            report: base,
                        }),
                    })
                }
                "image" => {
                    let image_id = row.try_get("image_id")?;
                    let base = report_base(row)?;
                    Ok(EmbeddingReport {
                        id: row.try_get("id")?,
                        report: EmbeddingReportType::ImageRemoval(ImageEmbeddingRemovalReport {
                            image_id,
                            report: base,
                        }),
                    })
                }
                _ => Err(sqlx::Error::InvalidArgument(format!(
                    "Invalid removal type '{}'",
                    row.try_get::<&str, _>("type")?
                ))),
            },
            _ => Err(sqlx::Error::InvalidArgument(format!(
                "Invalid report type '{report_type}'"
            ))),
        }
    }
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[serde(tag = "type")]
pub enum EmbeddingReportType {
    TextAddition(TextEmbeddingAdditionReport),
    ImageAddition(ImageEmbeddingAdditionReport),
    TextRemoval(TextEmbeddingRemovalReport),
    ImageRemoval(ImageEmbeddingRemovalReport),
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct TextEmbeddingAdditionReport {
    pub document_id: Uuid,
    pub document_name: String,
    pub report: EmbeddingAdditionReport,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ImageEmbeddingAdditionReport {
    pub image_id: Uuid,
    pub report: EmbeddingAdditionReport,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct TextEmbeddingRemovalReport {
    pub document_id: Uuid,
    pub document_name: String,
    pub report: EmbeddingReportBase,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ImageEmbeddingRemovalReport {
    pub image_id: Uuid,
    pub report: EmbeddingReportBase,
}

/// The details of adding embeddings to a collection.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct EmbeddingAdditionReport {
    /// The model used for embedding generation.
    pub model_used: String,

    /// The total tokens used to generate the embeddings, if applicable.
    pub tokens_used: Option<i32>,

    /// The embedding provider used that hosts the model.
    pub embedding_provider: String,

    /// The total vectors created. Always 1:1 with the original chunks if used for text embeddings.
    /// If used for image embeddings, this is the number of images.
    pub total_vectors: i32,

    /// Whether the embeddings were newly created (false) or were obtained from an embedding cache
    /// (true).
    pub cache: bool,

    /// Base parameters.
    pub base: EmbeddingReportBase,
}

/// The base struct for embedding reports.
///
/// Includes the collection details and the amount of time the action took.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct EmbeddingReportBase {
    /// The ID of the collection that the embeddings were added to.
    ///
    /// This is optional because we use the same struct for inserting and querying.
    ///
    /// During querying, if the collection no longer exists, this will be `None`.
    pub collection_id: Option<Uuid>,

    /// The name of the collection at the time of embedding. This is not updated if
    /// the original collection name changes.
    pub collection_name: String,

    /// The vector database where the embeddings are stored, i.e. the vector provider.
    pub vector_db: String,

    /// UTC datetime of when the embedding process started.
    pub started_at: chrono::DateTime<chrono::Utc>,

    /// UTC datetime of when the embedding process finished.
    pub finished_at: chrono::DateTime<chrono::Utc>,
}

/// TABLE: embeddings
///
/// Embedding information model. Represents the existence of a document in a collection.
#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TextEmbedding {
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
pub struct TextEmbeddingInsert {
    pub id: Uuid,
    pub document_id: Uuid,
    pub collection_id: Uuid,
}

impl TextEmbeddingInsert {
    pub fn new(document_id: Uuid, collection_id: Uuid) -> Self {
        Self {
            id: uuid::Uuid::new_v4(),
            document_id,
            collection_id,
        }
    }
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ImageEmbedding {
    pub id: Uuid,
    pub image_id: Uuid,
    pub collection_id: Uuid,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug)]
pub struct ImageEmbeddingInsert {
    pub id: Uuid,
    pub image_id: Uuid,
    pub collection_id: Uuid,
}

impl ImageEmbeddingInsert {
    pub fn new(image_id: Uuid, collection_id: Uuid) -> Self {
        Self {
            id: uuid::Uuid::new_v4(),
            image_id,
            collection_id,
        }
    }
}

search_column! {
    EmbeddingReportSearchColumn,
    // Common search fields
    Type => "ty",
    CollectionId => "collection_id",
    CollectionName => "collection_name",
    ModelUsed => "model_used",
    VectorDb => "vector_db",
    EmbeddingProvider => "embedding_provider",

    // Document search fields for text embeddings
    DocumentId => "document_id",
    DocumentName => "document_name",

    // Image search fields for image embeddings
    ImageId => "image_id",
}
