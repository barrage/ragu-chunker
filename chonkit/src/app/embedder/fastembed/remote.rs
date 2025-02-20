use crate::config::{
    DEFAULT_COLLECTION_EMBEDDING_MODEL, DEFAULT_COLLECTION_SIZE, FEMBED_EMBEDDER_ID,
};
use crate::{
    core::{
        embeddings::{Embedder, Embeddings},
        provider::Identity,
    },
    error::ChonkitError,
    map_err,
};
pub use chonkit_embedders::fembed::remote::RemoteFastEmbedder;

impl Identity for RemoteFastEmbedder {
    fn id(&self) -> &'static str {
        FEMBED_EMBEDDER_ID
    }
}

#[async_trait::async_trait]
impl Embedder for RemoteFastEmbedder {
    fn default_model(&self) -> (String, usize) {
        (
            String::from(DEFAULT_COLLECTION_EMBEDDING_MODEL),
            DEFAULT_COLLECTION_SIZE,
        )
    }

    async fn list_embedding_models(&self) -> Result<Vec<(String, usize)>, ChonkitError> {
        Ok(map_err!(self.list_models().await))
    }

    async fn embed(&self, content: &[&str], model: &str) -> Result<Embeddings, ChonkitError> {
        // TODO: Token usage
        Ok(Embeddings::new(
            map_err!(self.embed(content, model).await),
            None,
        ))
    }
}
