use crate::{
    config::VLLM_EMBEDDER_ID,
    core::{
        embeddings::{Embedder, EmbeddingModelRegistry, Embeddings},
        provider::Identity,
    },
    error::ChonkitError,
    map_err,
};
use chonkit_embedders::EmbeddingModel;

pub use chonkit_embedders::vllm::VllmEmbeddings;

impl Identity for VllmEmbeddings {
    fn id(&self) -> &'static str {
        VLLM_EMBEDDER_ID
    }
}

#[async_trait::async_trait]
impl EmbeddingModelRegistry for VllmEmbeddings {
    async fn list_embedding_models(&self) -> Result<Vec<EmbeddingModel>, ChonkitError> {
        Ok(self.list_models())
    }
}

#[async_trait::async_trait]
impl Embedder for VllmEmbeddings {
    async fn embed_text(&self, content: &[&str], model: &str) -> Result<Embeddings, ChonkitError> {
        let embeddings = map_err!(self.embed(content, model).await);
        Ok(Embeddings::new(
            embeddings.embeddings,
            Some(embeddings.total_tokens),
        ))
    }
}
