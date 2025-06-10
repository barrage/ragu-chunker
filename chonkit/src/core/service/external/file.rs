use crate::{
    core::{
        chunk::ChunkConfig,
        document::{parser::TextParseConfig, sha256, store::external::ExternalDocumentStorage},
        model::document::{Document, DocumentInsert},
        provider::ProviderState,
        repo::Repository,
    },
    error::ChonkitError,
};
use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

/// Ephemeral service used as an abstraction for operations on external APIs.
#[derive(Clone)]
pub struct ExternalFileService<T> {
    repo: Repository,
    providers: ProviderState,
    api: T,
}

impl<T> ExternalFileService<T> {
    pub fn new(repo: Repository, providers: ProviderState, api: T) -> Self {
        Self {
            repo,
            providers,
            api,
        }
    }
}

impl<T> ExternalFileService<T>
where
    T: ExternalDocumentStorage + Sync,
{
    /// Import files from an external API. All files imported via this function
    /// will be downloaded and stored locally.
    ///
    /// * `file_ids`: List of external file identifiers to retrieve info for.
    pub async fn import_documents(
        &self,
        file_ids: Vec<String>,
    ) -> Result<ImportResult, ChonkitError> {
        let storage = &self.providers.document.get_provider(self.api.id())?;

        let mut results = ImportResult::default();

        // Files from GDrive have their external file ID as the path
        for file_id in file_ids {
            let file = match self.api.get_file(&file_id).await {
                Ok(file) => file,
                Err(e) => {
                    results.failed.push(ImportFailure::new(
                        file_id,
                        "Unknown".to_string(),
                        e.to_string(),
                    ));
                    continue;
                }
            };

            let content = match self.api.download(&file.path.0).await {
                Ok(content) => content,
                Err(e) => {
                    results
                        .failed
                        .push(ImportFailure::new(file.path.0, file.name, e.to_string()));
                    continue;
                }
            };

            let hash = sha256(&content);

            if let Some(existing) = self.repo.get_document_by_hash(&hash).await? {
                results.skipped.push(existing);
                continue;
            }

            let path = storage.absolute_path(&file.name, file.ext);
            let name = file.name.clone();

            let document = self
                .repo
                .transaction(|tx| {
                    Box::pin(async move {
                        let insert =
                            DocumentInsert::new(&name, &path, file.ext, &hash, self.api.id());

                        let document = self
                            .repo
                            .insert_document_with_configs(
                                insert,
                                TextParseConfig::default(),
                                ChunkConfig::snapping_default(),
                                tx,
                            )
                            .await?;

                        match storage.write(&path, &content).await {
                            Ok(_) => Ok(document),
                            Err(e) => Err(e),
                        }
                    })
                })
                .await?;

            results.success.push(document);
        }

        Ok(results)
    }

    /// Import a single file from an external API. All files imported via this function
    /// will be downloaded and stored locally.
    ///
    /// * `file_id`: The external file ID.
    pub async fn import_document(&self, file_id: &str) -> Result<Document, ChonkitError> {
        let file = self.api.get_file(file_id).await?;
        let storage = match self.providers.document.get_provider(self.api.id()) {
            Ok(store) => store,
            Err(e) => {
                tracing::error!(
                    "External API has no registered storage provider ({})",
                    self.api.id()
                );
                return Err(e);
            }
        };

        let local_path = storage.absolute_path(&file.name, file.ext);

        let content = self.api.download(file_id).await?;

        let hash = sha256(&content);

        if let Some(existing) = self.repo.get_document_by_hash(&hash).await? {
            return Ok(existing);
        }

        storage.write(&local_path, &content).await?;

        self.repo
            .insert_document(DocumentInsert::new(
                &file.name,
                &local_path,
                file.ext,
                &hash,
                self.api.id(),
            ))
            .await
    }

    /// Check the modification time of a document on the external API and compare it with the
    /// local modification time. Returns a list of all files whose external modification time
    /// is newer.
    pub async fn list_outdated_documents(&self) -> Result<Vec<OutdatedDocument>, ChonkitError> {
        let documents = self
            .repo
            .list_all_document_update_times(self.api.id())
            .await?;

        let ext_documents = self.api.list_files(None).await?;

        let storage = self.providers.document.get_provider(self.api.id())?;

        let mut outdated = vec![];

        for (id, path, updated_at) in documents {
            let Some(ext_document) = ext_documents.iter().find(|document| {
                path.ends_with(&storage.absolute_path(&document.name, document.ext))
            }) else {
                continue;
            };

            if let Some(ext_modified_at) = ext_document.modified_at {
                if ext_modified_at > updated_at {
                    outdated.push(OutdatedDocument::new(
                        id,
                        ext_document.path.0.clone(),
                        ext_document.name.clone(),
                        updated_at,
                        ext_modified_at,
                    ));
                }
            }
        }

        Ok(outdated)
    }
}

#[derive(Debug, Default, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ImportResult {
    /// Successfully imported files.
    success: Vec<Document>,

    /// Failed to import files.
    failed: Vec<ImportFailure>,

    /// Skipped files in case of path collisions.
    skipped: Vec<Document>,
}

#[derive(Debug, Default, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ImportFailure {
    /// External file ID.
    file_id: String,

    /// External file name.
    file_name: String,

    /// Error message.
    error: String,
}

impl ImportFailure {
    pub fn new(file_id: String, file_name: String, error: String) -> Self {
        Self {
            file_id,
            file_name,
            error,
        }
    }
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct OutdatedDocument {
    /// The local ID of the document.
    id: Uuid,

    /// The external ID of the document.
    external_id: String,

    /// The local name of the document.
    name: String,

    /// The local modification time of the document.
    local_updated_at: DateTime<Utc>,

    /// The external modification time of the document.
    external_updated_at: DateTime<Utc>,
}

impl OutdatedDocument {
    pub fn new(
        id: Uuid,
        external_id: String,
        name: String,
        local_updated_at: DateTime<Utc>,
        external_updated_at: DateTime<Utc>,
    ) -> Self {
        Self {
            id,
            external_id,
            name,
            local_updated_at,
            external_updated_at,
        }
    }
}
