use crate::{
    config::VLLM_EMBEDDER_ID,
    core::{
        embeddings::{Embedder, Embeddings},
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
impl Embedder for VllmEmbeddings {
    fn default_model(&self) -> (String, usize) {
        (String::from("text-embedding-ada-002"), 1536)
    }

    async fn list_embedding_models(&self) -> Result<Vec<EmbeddingModel>, ChonkitError> {
        Ok(self.list_models())
    }

    async fn embed_text(&self, content: &[&str], model: &str) -> Result<Embeddings, ChonkitError> {
        let embeddings = map_err!(self.embed(content, model).await);
        Ok(Embeddings::new(
            embeddings.embeddings,
            Some(embeddings.total_tokens),
        ))
    }
}
