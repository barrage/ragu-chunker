use crate::config::AZURE_EMBEDDER_ID;
use crate::core::embeddings::{Embedder, EmbeddingModelRegistry, Embeddings};
use crate::core::provider::Identity;
use crate::error::ChonkitError;
use crate::map_err;
use chonkit_embedders::EmbeddingModel;

pub use chonkit_embedders::azure::AzureEmbeddings;

impl Identity for AzureEmbeddings {
    fn id(&self) -> &'static str {
        AZURE_EMBEDDER_ID
    }
}

#[async_trait::async_trait]
impl EmbeddingModelRegistry for AzureEmbeddings {
    async fn list_embedding_models(&self) -> Result<Vec<EmbeddingModel>, ChonkitError> {
        Ok(self.list_models())
    }
}

#[async_trait::async_trait]
impl Embedder for AzureEmbeddings {
    async fn embed_text(&self, content: &[&str], model: &str) -> Result<Embeddings, ChonkitError> {
        let embeddings = map_err!(self.embed(content, model).await);
        Ok(Embeddings::new(
            embeddings.embeddings,
            Some(embeddings.total_tokens),
        ))
    }
}
