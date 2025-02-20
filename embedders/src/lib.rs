/// Local or remote fastembed embeddings.
pub mod fembed;

/// Embeddings provided via the OpenAI API.
#[cfg(feature = "openai")]
pub mod openai;

/// Embeddings provided via the Azure OpenAI API.
#[cfg(feature = "azure")]
pub mod azure;

#[cfg(any(feature = "azure", feature = "openai"))]
mod openai_common {
    use serde::{Deserialize, Serialize};

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
