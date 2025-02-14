use super::{embeddings::Embedder, provider::ProviderState};
use crate::{err, error::ChonkitError, map_err};
use chunx::ChunkerError;
use serde::{Deserialize, Serialize};

pub async fn chunk<'i>(
    providers: &ProviderState,
    config: ChunkConfig,
    input: &'i str,
) -> Result<ChunkedDocument<'i>, ChonkitError> {
    let chunks = match config {
        ChunkConfig::Sliding(config) => {
            let chunker = map_err!(chunx::SlidingWindow::new(config.size, config.overlap));
            let chunked = map_err!(chunker.chunk(input));

            ChunkedDocument::Ref(chunked)
        }
        ChunkConfig::Snapping(config) => {
            let SnappingWindowConfig {
                size,
                overlap,
                delimiter,
                skip_f,
                skip_b,
            } = config;

            let chunker = map_err!(chunx::Snapping::new(
                size, overlap, delimiter, skip_f, skip_b
            ));

            let chunked = map_err!(chunker.chunk(input));

            ChunkedDocument::Owned(chunked)
        }
        ChunkConfig::Semantic(config) => {
            let SemanticWindowConfig {
                size,
                threshold,
                distance_fn,
                delimiter,
                embedding_provider,
                embedding_model,
                skip_f,
                skip_b,
            } = config;

            let chunker =
                chunx::Semantic::new(size, threshold, distance_fn, delimiter, skip_f, skip_b);

            let embedder = providers.embedding.get_provider(&embedding_provider)?;

            if embedder.size(&embedding_model).await?.is_none() {
                return err!(
                    InvalidEmbeddingModel,
                    "Model '{embedding_model}' not supported by '{embedding_provider}'"
                );
            };

            let semantic_embedder = SemanticEmbedder(embedder.clone());

            let chunked = chunker
                .chunk(input, &semantic_embedder, &embedding_model)
                .await?;

            ChunkedDocument::Owned(chunked)
        }
    };

    if chunks.is_empty() {
        return err!(Chunks, "chunks cannot be empty");
    }

    Ok(chunks)
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum ChunkConfig {
    Sliding(SlidingWindowConfig),
    Snapping(SnappingWindowConfig),
    Semantic(SemanticWindowConfig),
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SlidingWindowConfig {
    pub size: usize,
    pub overlap: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SnappingWindowConfig {
    pub size: usize,
    pub overlap: usize,
    pub delimiter: char,
    pub skip_f: Vec<String>,
    pub skip_b: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SemanticWindowConfig {
    pub size: usize,
    pub threshold: f64,
    pub distance_fn: chunx::semantic::DistanceFn,
    pub delimiter: char,
    pub skip_f: Vec<String>,
    pub skip_b: Vec<String>,
    #[serde(alias = "embedModel")]
    pub embedding_model: String,
    #[serde(alias = "embedProvider")]
    pub embedding_provider: String,
}

impl ChunkConfig {
    /// Create a `SlidingWindow` chunker.
    ///
    /// * `size`: Chunk base size.
    /// * `overlap`: Chunk overlap.
    pub fn sliding(size: usize, overlap: usize) -> Result<Self, ChunkerError> {
        Ok(Self::Sliding(SlidingWindowConfig { size, overlap }))
    }

    /// Create a default `SlidingWindow` chunker.
    pub fn sliding_default() -> Self {
        let config = chunx::SlidingWindow::default();
        Self::Sliding(SlidingWindowConfig {
            size: config.size,
            overlap: config.overlap,
        })
    }

    /// Create a `SnappingWindow` chunker.
    ///
    /// * `size`: Chunk base size.
    /// * `overlap`: Chunk overlap.
    /// * `skip_f`: Patterns in front of delimiters to not treat as sentence stops.
    /// * `skip_b`: Patterns behind delimiters to not treat as sentence stops.
    pub fn snapping(
        size: usize,
        overlap: usize,
        skip_f: Vec<String>,
        skip_b: Vec<String>,
        delimiter: char,
    ) -> Result<Self, ChunkerError> {
        Ok(Self::Snapping(SnappingWindowConfig {
            size,
            overlap,
            skip_f,
            skip_b,
            delimiter,
        }))
    }

    /// Create a default `SnappingWindow` chunker.
    pub fn snapping_default() -> Self {
        let config = chunx::Snapping::default();
        Self::Snapping(SnappingWindowConfig {
            size: config.size,
            overlap: config.overlap,
            skip_f: config.skip_forward,
            skip_b: config.skip_back,
            delimiter: '.',
        })
    }

    /// Create a `SemanticWindow` chunker.
    ///
    /// See [SemanticWindow](chunx::semantic::SemanticWindow) for more details.
    #[allow(clippy::too_many_arguments)]
    pub fn semantic(
        size: usize,
        threshold: f64,
        delimiter: char,
        distance_fn: chunx::semantic::DistanceFn,
        embedding_provider: String,
        embedding_model: String,
        skip_f: Vec<String>,
        skip_b: Vec<String>,
    ) -> Self {
        Self::Semantic(SemanticWindowConfig {
            size,
            threshold,
            distance_fn,
            delimiter,
            embedding_provider,
            embedding_model,
            skip_f,
            skip_b,
        })
    }

    /// Create a default `SemanticWindow` chunker.
    ///
    /// * `embedder`: Embedder to use for embedding chunks, uses the default embedder model.
    pub fn semantic_default(embedding_provider: String, embedding_model: String) -> Self {
        let config = chunx::semantic::Semantic::default();
        Self::Semantic(SemanticWindowConfig {
            size: config.size,
            delimiter: config.delimiter,
            distance_fn: config.distance_fn,
            threshold: config.threshold,
            skip_f: config.skip_forward,
            skip_b: config.skip_back,
            embedding_provider,
            embedding_model,
        })
    }
}

impl std::fmt::Display for ChunkConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Sliding(_) => write!(f, "SlidingWindow"),
            Self::Snapping(_) => write!(f, "SnappingWindow"),
            Self::Semantic(_) => write!(f, "SemanticWindow"),
        }
    }
}

/// The result of chunking a document.
/// Some chunkers do not allocate.
pub enum ChunkedDocument<'content> {
    Ref(Vec<&'content str>),
    Owned(Vec<String>),
}

impl ChunkedDocument<'_> {
    pub fn is_empty(&self) -> bool {
        match self {
            Self::Ref(v) => v.is_empty(),
            Self::Owned(v) => v.is_empty(),
        }
    }
}

pub struct SemanticEmbedder(pub std::sync::Arc<dyn Embedder + Send + Sync>);

impl chunx::semantic::Embedder for SemanticEmbedder {
    type Error = ChonkitError;

    async fn embed(&self, input: &[&str], model: &str) -> Result<Vec<Vec<f64>>, Self::Error> {
        let embeddings = self.0.embed(input, model).await?;
        Ok(embeddings.embeddings)
    }
}
