use crate::{
    openai_common::{
        handle_request_error, EmbeddingRequest, EmbeddingResponse, OpenAIEmbeddingResponse,
    },
    EmbeddingError, EmbeddingModel,
};
use reqwest::header::HeaderMap;
use serde::Serialize;
use std::error::Error;

pub struct VllmEmbeddings {
    endpoint: String,
    client: reqwest::Client,
}

impl VllmEmbeddings {
    pub fn new(endpoint: String, key: Option<String>) -> Self {
        let mut client = reqwest::ClientBuilder::new();
        let mut default_headers = HeaderMap::new();

        if let Some(key) = &key {
            default_headers.append("Authorization", format!("Bearer {key}").parse().unwrap());
            client = client.default_headers(default_headers);
        }

        Self {
            endpoint,
            client: client.build().expect("unable to build http client"),
        }
    }

    pub async fn embed(
        &self,
        input: &[&str],
        model: &str,
    ) -> Result<EmbeddingResponse, EmbeddingError> {
        let request = EmbeddingRequest { input };
        let url = format!("{}/{model}/v1/embeddings", self.endpoint);

        let response = match self.client.post(url).json(&request).send().await {
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

    pub async fn embed_image(
        &self,
        system: Option<&str>,
        text: Option<&str>,
        image: &str,
        model: &str,
    ) -> Result<EmbeddingResponse, EmbeddingError> {
        let mut request = MultimodalRequest { messages: vec![] };

        if let Some(system) = system {
            request.messages.push(Message {
                role: "system",
                content: EmbeddingMessage::System(system),
            });
        }

        let content = vec![
            EmbeddingInput::Image(ImageInput {
                r#type: "image_url",
                url: image,
            }),
            EmbeddingInput::Text(TextInput {
                r#type: "text",
                text: text.unwrap_or("Represent the image."),
            }),
        ];

        request.messages.push(Message {
            role: "user",
            content: EmbeddingMessage::User(content),
        });

        let url = format!("{}/{model}/v1/embeddings", self.endpoint);

        let response = match self.client.post(url).json(&request).send().await {
            Ok(res) => res,
            Err(e) => {
                tracing::error!("Error in response: {e}");
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
            "Embedded image with '{}', used tokens {}-{} (prompt-total)",
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

    pub fn list_models(&self) -> Vec<EmbeddingModel> {
        vec![EmbeddingModel {
            name: "qwen2-dse".to_string(),
            size: 1536,
            provider: "vllm".to_string(),
            multimodal: true,
        }]
    }
}

#[derive(Debug, Serialize)]
struct MultimodalRequest<'a> {
    messages: Vec<Message<'a>>,
}

#[derive(Debug, Serialize)]
struct Message<'a> {
    role: &'static str,
    content: EmbeddingMessage<'a>,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
enum EmbeddingMessage<'a> {
    System(&'a str),
    User(Vec<EmbeddingInput<'a>>),
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
enum EmbeddingInput<'a> {
    Text(TextInput<'a>),
    Image(ImageInput<'a>),
}

#[derive(Debug, Serialize)]
struct TextInput<'a> {
    r#type: &'static str,
    text: &'a str,
}

#[derive(Debug, Serialize)]
struct ImageInput<'a> {
    r#type: &'static str,
    url: &'a str,
}
