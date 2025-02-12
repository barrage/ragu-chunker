use crate::{
    core::auth::{OAuth, OAuthExchangeRequest, OAuthToken},
    error::ChonkitError,
};

#[derive(Clone)]
pub struct ExternalAuthorizationService<T> {
    api: T,
}

impl<T> ExternalAuthorizationService<T> {
    pub fn new(api: T) -> Self {
        Self { api }
    }
}

impl<T> ExternalAuthorizationService<T>
where
    T: OAuth,
{
    pub async fn exchange_code(
        &self,
        request: OAuthExchangeRequest,
    ) -> Result<OAuthToken, ChonkitError> {
        self.api.exchange_code(request).await
    }
}
