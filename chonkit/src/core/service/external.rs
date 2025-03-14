use crate::core::{
    document::store::external::ExternalDocumentStorage, provider::ProviderState, repo::Repository,
};
use file::ExternalFileService;

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
}
