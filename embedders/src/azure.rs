use super::EmbeddingModel;
use crate::{
    openai_common::{
        handle_request_error, EmbeddingRequest, EmbeddingResponse, OpenAIEmbeddingResponse,
        TEXT_EMBEDDING_ADA_002, TEXT_EMBEDDING_ADA_002_SIZE,
    },
    EmbeddingError,
};
use std::error::Error;
use tracing::debug;

pub struct AzureEmbeddings {
    endpoint: String,
    key: String,
    api_version: String,
    client: reqwest::Client,
}

impl AzureEmbeddings {
    pub fn new(endpoint: String, api_key: String, api_version: String) -> Self {
        Self {
            endpoint,
            key: api_key,
            api_version,
            client: reqwest::Client::new(),
        }
    }

    pub fn list_models(&self) -> Vec<EmbeddingModel> {
        vec![EmbeddingModel {
            name: TEXT_EMBEDDING_ADA_002.to_string(),
            size: TEXT_EMBEDDING_ADA_002_SIZE,
            provider: "azure".to_string(),
            multimodal: false,
        }]
    }

    pub async fn embed(
        &self,
        input: &[&str],
        deployment: &str,
    ) -> Result<EmbeddingResponse, EmbeddingError> {
        let request = EmbeddingRequest { input };
        let url = format!(
            "{}/openai/deployments/{deployment}/embeddings",
            self.endpoint
        );

        let response = match self
            .client
            .post(url)
            .header("api-key", &self.key)
            .query(&[("api-version", &self.api_version)])
            .json(&request)
            .send()
            .await
        {
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
