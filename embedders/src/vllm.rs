use std::error::Error;

use crate::{
    openai_common::{
        handle_request_error, EmbeddingRequest, EmbeddingResponse, OpenAIEmbeddingResponse,
    },
    EmbeddingError,
};

pub struct VllmEmbeddings {
    endpoint: String,
    key: Option<String>,
    client: reqwest::Client,
}

impl VllmEmbeddings {
    pub fn new(endpoint: String, key: Option<String>) -> Self {
        Self {
            endpoint,
            key,
            client: reqwest::Client::new(),
        }
    }

    pub async fn embed(
        &self,
        input: &[&str],
        model: &str,
    ) -> Result<EmbeddingResponse, EmbeddingError> {
        let request = EmbeddingRequest { input };
        let url = format!("{}/{model}/v1/embeddings", self.endpoint);

        let mut req = self.client.post(url);

        if let Some(key) = &self.key {
            req = req.bearer_auth(key);
        }

        let response = match req.json(&request).send().await {
            Ok(res) => res,
            Err(e) => {
                tracing::error!("Error in Azure response: {e}");
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

    pub fn list_embedding_models(&self) -> &[(&str, usize)] {
        &[("qwen2-dse", 1536)]
    }
}
