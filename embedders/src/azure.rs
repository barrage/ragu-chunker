use serde::Serialize;
use std::error::Error;
use tracing::debug;

use crate::{
    openai_common::{
        EmbeddingResponse, OpenAIEmbeddingResponse, OpenAIError, TEXT_EMBEDDING_ADA_002,
        TEXT_EMBEDDING_ADA_002_SIZE,
    },
    EmbeddingError,
};

pub struct AzureEmbeddings {
    endpoint: String,
    key: String,
    api_version: String,
    client: reqwest::Client,
}

impl AzureEmbeddings {
    pub fn new(endpoint: &str, api_key: &str, api_version: &str) -> Self {
        Self {
            endpoint: endpoint.to_string(),
            key: api_key.to_string(),
            api_version: api_version.to_string(),
            client: reqwest::Client::new(),
        }
    }

    pub async fn list_embedding_models(&self) -> &[(&str, usize)] {
        &[(TEXT_EMBEDDING_ADA_002, TEXT_EMBEDDING_ADA_002_SIZE)]
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
                    tracing::error!("Error reading Azure response: {}", e);
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
    input: &'i [&'i str],
}
