use crate::core::{
    auth::OAuth, document::store::external::ExternalDocumentStorage, provider::ProviderState,
    repo::Repository,
};
use auth::ExternalAuthorizationService;
use file::ExternalFileService;

pub mod auth;
pub mod file;

#[derive(Clone)]
pub struct ServiceFactory {
    repo: Repository,
    providers: ProviderState,
}

impl ServiceFactory {
    pub fn new(repo: Repository, providers: ProviderState) -> Self {
        Self { repo, providers }
    }

    /// Create an instance of [ExternalFileService] using the provided storage API.
    pub fn storage<T: ExternalDocumentStorage>(&self, api: T) -> ExternalFileService<T> {
        ExternalFileService::new(self.repo.clone(), self.providers.clone(), api)
    }

    /// Create an instance of [ExternalAuthorizationService] using the provided OAuth API.
    pub fn authorization<T: OAuth>(&self, api: T) -> ExternalAuthorizationService<T> {
        ExternalAuthorizationService::new(api)
    }
}
