/// Local or remote fastembed embeddings.
pub mod fembed;

/// Embeddings provided via the OpenAI API.
#[cfg(feature = "openai")]
pub mod openai;

/// Embeddings provided via the Azure OpenAI API.
#[cfg(feature = "azure")]
pub mod azure;

#[cfg(feature = "vllm")]
pub mod vllm;

#[cfg(any(feature = "azure", feature = "openai"))]
mod openai_common {
    use std::error::Error;

    use reqwest::Response;
    use serde::{Deserialize, Serialize};

    use crate::EmbeddingError;

    #[derive(Debug, Serialize)]
    pub struct EmbeddingResponse {
        pub embeddings: Vec<Vec<f64>>,
        pub prompt_tokens: usize,
        pub total_tokens: usize,
    }

    #[derive(Debug, Deserialize)]
    pub struct OpenAIEmbeddingResponse {
        pub data: Vec<EmbeddingObject>,
        pub model: String,
        pub usage: Usage,
    }

    #[derive(Debug, Deserialize)]
    pub struct EmbeddingObject {
        pub embedding: Vec<f64>,
    }

    #[derive(Debug, Deserialize)]
    pub struct Usage {
        pub prompt_tokens: usize,
        pub total_tokens: usize,
    }

    #[derive(Debug, Deserialize, thiserror::Error)]
    #[error("{message}, type: {r#type}, param: {param:?}, code: {code:?}")]
    pub struct OpenAIErrorParams {
        pub message: String,
        pub r#type: String,
        pub param: Option<String>,
        pub code: Option<usize>,
    }

    #[derive(Debug, Deserialize, thiserror::Error)]
    #[error("Open AI error response {{ {error} }}")]
    pub struct OpenAIError {
        pub error: OpenAIErrorParams,
    }

    pub const TEXT_EMBEDDING_3_LARGE: &str = "text-embedding-3-large";
    pub const TEXT_EMBEDDING_3_SMALL: &str = "text-embedding-3-small";
    pub const TEXT_EMBEDDING_ADA_002: &str = "text-embedding-ada-002";

    pub const TEXT_EMBEDDING_3_LARGE_SIZE: usize = 3072;
    pub const TEXT_EMBEDDING_3_SMALL_SIZE: usize = 1536;
    pub const TEXT_EMBEDDING_ADA_002_SIZE: usize = 1536;

    pub const EMBEDDING_MODELS: &[(&str, usize)] = &[
        (TEXT_EMBEDDING_3_LARGE, TEXT_EMBEDDING_3_LARGE_SIZE),
        (TEXT_EMBEDDING_3_SMALL, TEXT_EMBEDDING_3_SMALL_SIZE),
        (TEXT_EMBEDDING_ADA_002, TEXT_EMBEDDING_ADA_002_SIZE),
    ];

    #[derive(Debug, Serialize)]
    pub struct EmbeddingRequest<'i> {
        pub input: &'i [&'i str],
    }

    pub async fn handle_request_error(response: Response) -> EmbeddingError {
        tracing::error!(
            "Request to {} failed with status {}",
            response.url(),
            response.status()
        );

        let Some(ct) = response.headers().get(reqwest::header::CONTENT_TYPE) else {
            return EmbeddingError::Response("missing content-type header in response".to_owned());
        };

        let ct = match ct.to_str() {
            Ok(ct) => ct,
            Err(e) => {
                tracing::error!("Error reading content-type header: {}", e);
                return EmbeddingError::Response("malformed content-type header".to_owned());
            }
        };

        if !ct.contains("application/json") {
            let response = match response.text().await {
                Ok(r) => r,
                Err(e) => return EmbeddingError::Reqwest(e),
            };
            return EmbeddingError::Response(response);
        }

        let response = match response.json::<OpenAIError>().await {
            Ok(res) => res,
            Err(e) => {
                tracing::error!("Error reading response: {}", e);
                tracing::error!("Source: {:?}", e.source());
                return EmbeddingError::Reqwest(e);
            }
        };

        tracing::error!("Response: {response:?}");

        EmbeddingError::OpenAI(response)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum EmbeddingError {
    #[error("invalid model: {0}")]
    InvalidModel(String),

    #[error("invalid input: {0}")]
    InvalidInput(String),

    #[cfg(feature = "fe-local")]
    #[error(transparent)]
    Fastembed(#[from] fastembed::Error),

    #[cfg(any(feature = "openai", feature = "fe-remote", feature = "azure"))]
    #[error(transparent)]
    Reqwest(#[from] reqwest::Error),

    /// Contains the error response text in case of OpenAI errors.
    #[cfg(any(feature = "openai", feature = "azure"))]
    #[error(transparent)]
    OpenAI(openai_common::OpenAIError),

    /// Contains an error message in case of unexpected responses from downstream services,
    /// such as no content type headers.
    #[cfg(any(feature = "openai", feature = "fe-remote", feature = "azure"))]
    #[error("{0}")]
    Response(String),
}
