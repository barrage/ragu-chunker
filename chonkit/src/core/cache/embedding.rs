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
pub struct CachedEmbeddings {
    pub embeddings: Vec<Vec<f64>>,
    pub tokens_used: Option<usize>,
    pub chunks: Vec<String>,
}

impl CachedEmbeddings {
    pub fn new(embeddings: Vec<Vec<f64>>, tokens_used: Option<usize>, chunks: Vec<String>) -> Self {
        CachedEmbeddings {
            embeddings,
            tokens_used,
            chunks,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct EmbeddingCacheKey(String);

impl EmbeddingCacheKey {
    pub fn new(
        document_hash: &str,
        chunk_config: Option<&ChunkConfig>,
        parse_config: &ParseMode,
    ) -> Result<Self, ChonkitError> {
        Ok(EmbeddingCacheKey(
            EmbeddingCacheKeyInner::new(document_hash, chunk_config, parse_config)
                .into_cache_key()?,
        ))
    }

    pub fn key(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Serialize)]
struct EmbeddingCacheKeyInner<'a> {
    document_hash: &'a str,
    chunk_config: Option<&'a ChunkConfig>,
    parse_config: &'a ParseMode,
}

impl<'a> EmbeddingCacheKeyInner<'a> {
    fn new(
        document_hash: &'a str,
        chunk_config: Option<&'a ChunkConfig>,
        parse_config: &'a ParseMode,
    ) -> Self {
        EmbeddingCacheKeyInner {
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
