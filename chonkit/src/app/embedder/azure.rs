use crate::config::AZURE_EMBEDDER_ID;
use crate::core::embeddings::{Embedder, Embeddings};
use crate::core::provider::Identity;
use crate::error::ChonkitError;
use crate::{err, map_err};
use chonkit_embedders::EmbeddingModel;

pub use chonkit_embedders::azure::AzureEmbeddings;

impl Identity for AzureEmbeddings {
    fn id(&self) -> &'static str {
        AZURE_EMBEDDER_ID
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

    async fn list_embedding_models(&self) -> Result<Vec<EmbeddingModel>, ChonkitError> {
        Ok(self.list_models())
    }

    #[allow(unused_variables)]
    async fn embed_image(
        &self,
        system: Option<&str>,
        text: Option<&str>,
        image: &str,
        model: &str,
    ) -> Result<Embeddings, ChonkitError> {
        err!(
            OperationUnsupported,
            "Provider '{}' does not support multimodal embeddings",
            self.id()
        )
    }
}
