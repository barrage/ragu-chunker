use crate::config::OPENAI_EMBEDDER_ID;
use crate::core::embedder::Embedder;
use crate::core::provider::Identity;
use crate::error::ChonkitError;
use crate::map_err;

pub use chonkit_embedders::openai::OpenAiEmbeddings;

impl Identity for OpenAiEmbeddings {
    fn id(&self) -> &'static str {
        OPENAI_EMBEDDER_ID
    }
}

#[async_trait::async_trait]
impl Embedder for OpenAiEmbeddings {
    fn default_model(&self) -> (String, usize) {
        (String::from("text-embedding-ada-002"), 1536)
    }

    async fn list_embedding_models(&self) -> Result<Vec<(String, usize)>, ChonkitError> {
        Ok(self.list_embedding_models())
    }

    async fn embed(&self, content: &[&str], model: &str) -> Result<Vec<Vec<f64>>, ChonkitError> {
        Ok(map_err!(self.embed(content, model).await))
    }
}
