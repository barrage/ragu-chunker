use crate::config::FEMBED_EMBEDDER_ID;
use crate::{
    core::{
        embeddings::{Embedder, EmbeddingModelRegistry, Embeddings},
        provider::Identity,
    },
    error::ChonkitError,
    map_err,
};
use chonkit_embedders::EmbeddingModel;

pub use chonkit_embedders::fembed::remote::RemoteFastEmbedder;

impl Identity for RemoteFastEmbedder {
    fn id(&self) -> &'static str {
        FEMBED_EMBEDDER_ID
    }
}

#[async_trait::async_trait]
impl EmbeddingModelRegistry for RemoteFastEmbedder {
    async fn list_embedding_models(&self) -> Result<Vec<EmbeddingModel>, ChonkitError> {
        Ok(map_err!(self.list_models().await))
    }
}

#[async_trait::async_trait]
impl Embedder for RemoteFastEmbedder {
    async fn embed_text(&self, content: &[&str], model: &str) -> Result<Embeddings, ChonkitError> {
        // TODO: Token usage
        Ok(Embeddings::new(
            map_err!(self.embed(content, model).await),
            None,
        ))
    }
}
