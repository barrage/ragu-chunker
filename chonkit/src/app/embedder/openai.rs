use crate::config::OPENAI_EMBEDDER_ID;
use crate::core::embeddings::{Embedder, Embeddings};
use crate::core::provider::Identity;
use crate::error::ChonkitError;
use crate::{err, map_err};
use chonkit_embedders::EmbeddingModel;

pub use chonkit_embedders::openai::OpenAiEmbeddings;

impl Identity for OpenAiEmbeddings {
    fn id(&self) -> &'static str {
        OPENAI_EMBEDDER_ID
    }
}

#[async_trait::async_trait]
impl Embedder for OpenAiEmbeddings {
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
