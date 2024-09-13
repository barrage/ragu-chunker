use crate::{core::embedder::Embedder, error::ChonkitError};
use fastembed::{InitOptions, TextEmbedding};

#[derive(Debug, Clone)]
pub struct FastEmbedder;

const DEFAULT_COLLECTION_MODEL: &str = "Qdrant/all-MiniLM-L6-v2-onnx";
const DEFAULT_COLLECTION_SIZE: usize = 384;

#[async_trait::async_trait]
impl Embedder for FastEmbedder {
    fn id(&self) -> &'static str {
        "fastembed"
    }

    fn default_model(&self) -> (String, usize) {
        (
            String::from(DEFAULT_COLLECTION_MODEL),
            DEFAULT_COLLECTION_SIZE,
        )
    }

    fn list_embedding_models(&self) -> Vec<(String, usize)> {
        fastembed::TextEmbedding::list_supported_models()
            .into_iter()
            .map(|model| (model.model_code, model.dim))
            .collect()
    }

    async fn embed(&self, content: &[&str], model: &str) -> Result<Vec<Vec<f32>>, ChonkitError> {
        let model = fastembed::TextEmbedding::list_supported_models()
            .into_iter()
            .find(|m| m.model_code == model)
            .ok_or_else(|| {
                ChonkitError::InvalidEmbeddingModel(format!(
                    "{model} is not a valid fastembed model",
                ))
            })?;

        let embedder = TextEmbedding::try_new(InitOptions {
            model_name: model.model,
            show_download_progress: true,
            ..Default::default()
        })
        .map_err(|err| ChonkitError::Fastembed(err.to_string()))?;

        let embeddings = embedder
            .embed(content.to_vec(), None)
            .map_err(|err| ChonkitError::Fastembed(err.to_string()))?;

        debug_assert_eq!(
            embeddings.len(),
            content.len(),
            "Content length is different from embeddings!"
        );

        Ok(embeddings)
    }

    fn size(&self, model: &str) -> Option<usize> {
        fastembed::TextEmbedding::list_supported_models()
            .into_iter()
            .find(|m| m.model_code == model)
            .map(|m| m.dim)
    }
}
