use crate::config::AZURE_EMBEDDER_ID;
use crate::core::embeddings::{Embedder, Embeddings};
use crate::core::provider::Identity;
use crate::error::ChonkitError;
use crate::map_err;

pub use chonkit_embedders::azure::AzureEmbeddings;

impl Identity for AzureEmbeddings {
    fn id(&self) -> &'static str {
        AZURE_EMBEDDER_ID
    }
}

#[async_trait::async_trait]
impl Embedder for AzureEmbeddings {
    fn default_model(&self) -> (String, usize) {
        (String::from("text-embedding-ada-002"), 1536)
    }

    async fn list_embedding_models(&self) -> Result<Vec<(String, usize)>, ChonkitError> {
        Ok(self
            .list_embedding_models()
            .await
            .iter()
            .map(|(m, s)| (m.to_string(), *s))
            .collect())
    }

    async fn embed(&self, content: &[&str], model: &str) -> Result<Embeddings, ChonkitError> {
        let embeddings = map_err!(self.embed(content, model).await);
        Ok(Embeddings::new(
            embeddings.embeddings,
            Some(embeddings.total_tokens),
        ))
    }
}
