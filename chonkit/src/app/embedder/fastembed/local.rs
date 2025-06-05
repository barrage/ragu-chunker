use crate::config::FEMBED_EMBEDDER_ID;
use crate::core::embeddings::Embeddings;
use crate::core::provider::Identity;
use crate::err;
use crate::{core::embeddings::Embedder, error::ChonkitError, map_err};
use chonkit_embedders::EmbeddingModel;

pub use chonkit_embedders::fembed::local::LocalFastEmbedder;

impl Identity for LocalFastEmbedder {
    fn id(&self) -> &'static str {
        FEMBED_EMBEDDER_ID
    }
}

#[async_trait::async_trait]
impl Embedder for LocalFastEmbedder {
    async fn embed_text(&self, content: &[&str], model: &str) -> Result<Embeddings, ChonkitError> {
        // TODO: Token usage
        Ok(Embeddings::new(map_err!(self.embed(content, model)), None))
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
