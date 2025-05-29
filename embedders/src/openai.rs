use crate::{
    openai_common::{
        handle_request_error, EmbeddingResponse, OpenAIEmbeddingResponse, EMBEDDING_MODELS,
    },
    EmbeddingError,
};
use serde::Serialize;
use std::error::Error;

const DEFAULT_OPENAI_ENDPOINT: &str = "https://api.openai.com";

pub struct OpenAiEmbeddings {
    endpoint: String,
    key: String,
    client: reqwest::Client,
}

impl OpenAiEmbeddings {
    pub fn new(api_key: &str) -> Self {
        Self {
            endpoint: DEFAULT_OPENAI_ENDPOINT.to_string(),
            key: api_key.to_string(),
            client: reqwest::Client::new(),
        }
    }

    pub fn list_embedding_models(&self) -> &[(&str, usize)] {
        EMBEDDING_MODELS
    }

    pub async fn embed(
        &self,
        input: &[&str],
        model: &str,
    ) -> Result<EmbeddingResponse, EmbeddingError> {
        let request = EmbeddingRequest { model, input };

        if input.is_empty() {
            return Err(EmbeddingError::InvalidInput(format!(
                "cannot be empty (len = {})",
                input.len()
            )));
        }

        let response = match self
            .client
            .post(format!("{}/v1/embeddings", self.endpoint))
            .bearer_auth(&self.key)
            .json(&request)
            .send()
            .await
        {
            Ok(res) => res,
            Err(e) => {
                tracing::error!("Error in OpenAI request: {e}");
                return Err(EmbeddingError::Reqwest(e));
            }
        };

        if response.status() != 200 {
            return Err(handle_request_error(response).await);
        }

        let response = match response.json::<OpenAIEmbeddingResponse>().await {
            Ok(res) => res,
            Err(e) => {
                tracing::error!("Error decoding OpenAI response: {}", e);
                tracing::error!("Source: {:?}", e.source());
                return Err(EmbeddingError::Reqwest(e));
            }
        };

        tracing::debug!(
            "Embedded {} chunk(s) with '{}', used tokens {}-{} (prompt-total)",
            input.len(),
            response.model,
            response.usage.prompt_tokens,
            response.usage.total_tokens
        );

        Ok(EmbeddingResponse {
            embeddings: response.data.into_iter().map(|o| o.embedding).collect(),
            prompt_tokens: response.usage.prompt_tokens,
            total_tokens: response.usage.total_tokens,
        })
    }
}

#[derive(Debug, Serialize)]
struct EmbeddingRequest<'i> {
    model: &'i str,
    input: &'i [&'i str],
}
