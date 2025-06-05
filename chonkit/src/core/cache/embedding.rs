use crate::{
    core::{
        chunk::ChunkConfig,
        document::{parser::ParseMode, sha256},
    },
    error::ChonkitError,
    map_err,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct CachedTextEmbeddings {
    pub embeddings: Vec<Vec<f64>>,
    pub tokens_used: Option<usize>,
    pub chunks: Vec<String>,
}

impl CachedTextEmbeddings {
    pub fn new(embeddings: Vec<Vec<f64>>, tokens_used: Option<usize>, chunks: Vec<String>) -> Self {
        Self {
            embeddings,
            tokens_used,
            chunks,
        }
    }
}

/// A wrapper around the resulting cache key obtained via [TextEmbeddingCacheKey::new].
///
/// Always obtained from a combination of the document's hash, its chunking config and the parse mode.
#[derive(Debug)]
pub struct TextEmbeddingCacheKey(String);

impl TextEmbeddingCacheKey {
    pub fn new(
        model_name: &str,
        document_hash: &str,
        chunk_config: Option<&ChunkConfig>,
        parse_config: &ParseMode,
    ) -> Result<Self, ChonkitError> {
        Ok(TextEmbeddingCacheKey(
            TextEmbeddingCacheKeyInner::new(model_name, document_hash, chunk_config, parse_config)
                .into_cache_key()?,
        ))
    }

    pub fn key(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Serialize)]
struct TextEmbeddingCacheKeyInner<'a> {
    model_name: &'a str,
    document_hash: &'a str,
    chunk_config: Option<&'a ChunkConfig>,
    parse_config: &'a ParseMode,
}

impl<'a> TextEmbeddingCacheKeyInner<'a> {
    fn new(
        model_name: &'a str,
        document_hash: &'a str,
        chunk_config: Option<&'a ChunkConfig>,
        parse_config: &'a ParseMode,
    ) -> Self {
        TextEmbeddingCacheKeyInner {
            model_name,
            document_hash,
            chunk_config,
            parse_config,
        }
    }

    /// Transforms this key into a JSON string then hashes it with sha256.
    /// FIXME: There is *definitely* a more efficient way to do this, but it works for now.
    fn into_cache_key(self) -> Result<String, ChonkitError> {
        Ok(sha256(map_err!(serde_json::to_string(&self)).as_bytes()))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct CachedImageEmbeddings {
    pub embeddings: Vec<f64>,
    pub tokens_used: Option<usize>,
    pub image_b64: String,
    pub image_path: String,
    pub description: Option<String>,
}

impl CachedImageEmbeddings {
    pub fn new(
        embeddings: Vec<f64>,
        tokens_used: Option<usize>,
        image_b64: String,
        image_path: String,
        description: Option<String>,
    ) -> Self {
        Self {
            embeddings,
            tokens_used,
            image_b64,
            image_path,
            description,
        }
    }
}

#[derive(Debug)]
pub struct ImageEmbeddingCacheKey(String, String);

impl ImageEmbeddingCacheKey {
    pub fn new(image_path: String, model: String) -> Self {
        Self(image_path, model)
    }

    pub fn key(&self) -> String {
        format!("{}-{}", self.0, self.1)
    }
}
