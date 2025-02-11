use crate::error::ChonkitError;
use serde::{Deserialize, Serialize};
use std::future::Future;

#[derive(Debug, Deserialize, Serialize, utoipa::ToSchema)]
pub struct OAuthExchangeRequest {
    pub grant_type: String,
    pub code: String,
    pub redirect_uri: String,
    pub code_verifier: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, utoipa::ToSchema)]
pub struct OAuthToken {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_in: usize,
}

pub trait OAuth {
    fn exchange_code(
        &self,
        request: OAuthExchangeRequest,
    ) -> impl Future<Output = Result<OAuthToken, ChonkitError>> + Send + Sync;
}

#[async_trait::async_trait]
pub trait Authorize {
    async fn verify(&self, token: &str) -> Result<(), ChonkitError>;
}
