use crate::{
    core::{
        chunk::ChunkConfig,
        document::{parser::ParseConfig, sha256},
        model::image::ImageHash,
    },
    error::ChonkitError,
    map_err,
};
use serde::{Deserialize, Serialize};

/// Cached text embeddings with their chunks.
///
/// We keep the document chunks in the cache in order to skip processing it.
/// This is especially handy when semantic embedders are used.
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
        parse_config: &ParseConfig,
    ) -> Result<Self, ChonkitError> {
        Ok(TextEmbeddingCacheKey(
            TextEmbeddingCacheKeyInner::new(model_name, document_hash, chunk_config, parse_config)
                .into_cache_key()?,
        ))
    }

    pub(super) fn key(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Serialize)]
struct TextEmbeddingCacheKeyInner<'a> {
    model_name: &'a str,
    document_hash: &'a str,
    chunk_config: Option<&'a ChunkConfig>,
    parse_config: &'a ParseConfig,
}

impl<'a> TextEmbeddingCacheKeyInner<'a> {
    fn new(
        model_name: &'a str,
        document_hash: &'a str,
        chunk_config: Option<&'a ChunkConfig>,
        parse_config: &'a ParseConfig,
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

/// Contains only the image embeddings. Obtained from the cache by [ImageEmbeddingCacheKey].
///
/// The image bytes must be obtained from storage.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct CachedImageEmbeddings {
    pub embeddings: Vec<f64>,
    pub tokens_used: Option<usize>,
}

impl CachedImageEmbeddings {
    pub fn new(embeddings: Vec<f64>, tokens_used: Option<usize>) -> Self {
        Self {
            embeddings,
            tokens_used,
        }
    }
}

/// The key used to retrieve the image embeddings from the cache.
///
/// See [ImageHash].
#[derive(Debug)]
pub struct ImageEmbeddingCacheKey<'a> {
    hash: &'a str,
    model: &'a str,
}

impl<'a> ImageEmbeddingCacheKey<'a> {
    pub fn new(hash: &'a ImageHash, model: &'a str) -> Self {
        Self {
            hash: &hash.0,
            model,
        }
    }

    pub(super) fn key(&self) -> String {
        format!("{}-{}", self.hash, self.model)
    }
}
