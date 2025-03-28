use crate::{
    openai_common::{EmbeddingResponse, OpenAIEmbeddingResponse, OpenAIError, EMBEDDING_MODELS},
    EmbeddingError,
};
use serde::Serialize;
use std::error::Error;
use tracing::debug;

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
            tracing::error!(
                "Request to {} failed with status {}",
                response.url(),
                response.status()
            );

            let Some(ct) = response.headers().get(reqwest::header::CONTENT_TYPE) else {
                return Err(EmbeddingError::Response(
                    "missing content-type header in response".to_owned(),
                ));
            };

            let ct = match ct.to_str() {
                Ok(ct) => ct,
                Err(e) => {
                    tracing::error!("Error reading content-type header: {}", e);
                    return Err(EmbeddingError::Response(
                        "malformed content-type header".to_owned(),
                    ));
                }
            };

            if !ct.contains("application/json") {
                let response = match response.text().await {
                    Ok(r) => r,
                    Err(e) => return Err(EmbeddingError::Reqwest(e)),
                };
                return Err(EmbeddingError::Response(response));
            }

            let response = match response.json::<OpenAIError>().await {
                Ok(res) => res,
                Err(e) => {
                    tracing::error!("Error reading OpenAI response: {}", e);
                    tracing::error!("Source: {:?}", e.source());
                    return Err(EmbeddingError::Reqwest(e));
                }
            };

            tracing::error!("Response: {response:?}");

            return Err(EmbeddingError::OpenAI(response));
        }

        let response = match response.json::<OpenAIEmbeddingResponse>().await {
            Ok(res) => res,
            Err(e) => {
                tracing::error!("Error decoding OpenAI response: {}", e);
                tracing::error!("Source: {:?}", e.source());
                return Err(EmbeddingError::Reqwest(e));
            }
        };

        debug!(
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
