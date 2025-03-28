use crate::config::{
    DEFAULT_COLLECTION_EMBEDDING_MODEL, DEFAULT_COLLECTION_SIZE, FEMBED_EMBEDDER_ID,
};
use crate::core::embeddings::Embeddings;
use crate::core::provider::Identity;
use crate::{core::embeddings::Embedder, error::ChonkitError, map_err};

pub use chonkit_embedders::fembed::local::LocalFastEmbedder;

impl Identity for LocalFastEmbedder {
    fn id(&self) -> &'static str {
        FEMBED_EMBEDDER_ID
    }
}

#[async_trait::async_trait]
impl Embedder for LocalFastEmbedder {
    fn default_model(&self) -> (String, usize) {
        (
            String::from(DEFAULT_COLLECTION_EMBEDDING_MODEL),
            DEFAULT_COLLECTION_SIZE,
        )
    }

    async fn list_embedding_models(&self) -> Result<Vec<(String, usize)>, ChonkitError> {
        Ok(self
            .list_models()
            .into_iter()
            .map(|m| (m.model_code, m.dim))
            .collect())
    }

    async fn embed(&self, content: &[&str], model: &str) -> Result<Embeddings, ChonkitError> {
        // TODO: Token usage
        Ok(Embeddings::new(map_err!(self.embed(content, model)), None))
    }
}
