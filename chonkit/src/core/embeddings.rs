use super::provider::Identity;
use crate::error::ChonkitError;
use serde::{Deserialize, Serialize};

/// Operations related to embeddings and their models.
#[async_trait::async_trait]
pub trait Embedder: Identity {
    /// Used for creating the initial collection.
    fn default_model(&self) -> (String, usize);

    /// List all available models in the embedder and their sizes.
    async fn list_embedding_models(&self) -> Result<Vec<(String, usize)>, ChonkitError>;

    /// Return the size of the given model's embeddings
    /// if it is supported by the embedder.
    ///
    /// * `model`:
    async fn size(&self, model: &str) -> Result<Option<usize>, ChonkitError> {
        Ok(self
            .list_embedding_models()
            .await?
            .into_iter()
            .find(|m| m.0 == model)
            .map(|m| m.1))
    }

    /// Get the vectors for the elements in `content`.
    /// The content passed in can be a user's query,
    /// or a chunked document.
    ///
    /// * `content`: The text to embed.
    /// * `model`: The embedding model to use.
    async fn embed(&self, content: &[&str], model: &str) -> Result<Embeddings, ChonkitError>;
}

/// The result of embedding chunks.
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct Embeddings {
    /// The actual embedding. Indices are 1:1 with the original chunk vector (chunk[n] = embeddings[n]).
    pub embeddings: Vec<Vec<f64>>,

    /// Amount of tokens spent on the embedding, if applicable.
    pub tokens_used: Option<usize>,

    /// The source of the embeddings. Necessary because embeddings can be cached.
    pub source: EmbeddingSource,
}

impl Embeddings {
    /// Create a new embeddings struct with the source set to [EmbeddingSource::Model].
    pub fn new(embeddings: Vec<Vec<f64>>, tokens_used: Option<usize>) -> Self {
        Embeddings {
            embeddings,
            tokens_used,
            source: EmbeddingSource::Model,
        }
    }
}

/// Represents the origin of embeddings.
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub enum EmbeddingSource {
    /// The embeddings were obtained via the model.
    Model,

    /// The embeddings were obtained from the cache.
    Cache,
}

impl std::fmt::Display for Embeddings {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Embeddings {{ total embeddings: {}, tokens_used: {}, source: {} }}",
            self.embeddings.len(),
            self.tokens_used
                .map(|t| t.to_string())
                .unwrap_or(String::from("N/A")),
            self.source
        )
    }
}

impl std::fmt::Display for EmbeddingSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                EmbeddingSource::Model => "model",
                EmbeddingSource::Cache => "cache",
            }
        )
    }
}
