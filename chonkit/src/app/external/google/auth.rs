use crate::{
    core::auth::{OAuth, OAuthExchangeRequest, OAuthToken},
    err,
    error::{ChonkitErr, ChonkitError},
    map_err,
};
use serde::Serialize;
use std::sync::Arc;

use super::GoogleError;

const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";

/// Used for capturing Google access tokens in request extensions.
#[derive(Debug, Clone)]
pub struct GoogleAccessToken(pub(super) String);

impl GoogleAccessToken {
    pub fn new(token: String) -> Self {
        Self(token)
    }
}

pub struct GoogleOAuth {
    client: Arc<reqwest::Client>,
    config: GoogleOAuthConfig,
}

impl GoogleOAuth {
    pub fn new(client: Arc<reqwest::Client>, config: GoogleOAuthConfig) -> Self {
        Self { client, config }
    }
}

impl OAuth for GoogleOAuth {
    async fn exchange_code(
        &self,
        request: OAuthExchangeRequest,
    ) -> Result<OAuthToken, ChonkitError> {
        let request = FullExchangeRequest {
            params: request,
            config: self.config.clone(),
        };

        let response = map_err!(self.client.post(TOKEN_URL).form(&request).send().await);

        if !response.status().is_success() {
            let e = map_err!(response.json().await);
            return err!(ChonkitErr::GoogleApi(GoogleError::Auth(e)));
        }

        let token = map_err!(response.json().await);

        Ok(token)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct GoogleOAuthConfig {
    client_id: Arc<str>,
    client_secret: Arc<str>,
}

impl GoogleOAuthConfig {
    pub fn new(client_id: &str, client_secret: &str) -> Self {
        Self {
            client_id: client_id.into(),
            client_secret: client_secret.into(),
        }
    }
}

#[derive(Debug, Serialize)]
struct FullExchangeRequest {
    #[serde(flatten)]
    params: OAuthExchangeRequest,
    #[serde(flatten)]
    config: GoogleOAuthConfig,
}
