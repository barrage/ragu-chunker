use crate::{
    core::{
        chunk::ChunkConfig,
        document::{
            parser::{GenericParseConfig, ParseConfig, Parser},
            sha256,
            store::external::ExternalDocumentStorage,
        },
        model::document::{Document, DocumentInsert, DocumentParameterUpdate},
        provider::ProviderState,
        repo::{Atomic, Repository},
    },
    err,
    error::ChonkitError,
    transaction,
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
    T: ExternalDocumentStorage,
{
    /// Import files from an external API. All files imported via this function
    /// will be downloaded and stored locally.
    ///
    /// * `file_ids`: List of external file identifiers to retrieve info for.
    pub async fn import_documents(
        &self,
        file_ids: Vec<String>,
        force_download: bool,
    ) -> Result<ImportResult, ChonkitError> {
        let storage = &self.providers.storage.get_provider(self.api.id())?;

        let mut result = ImportResult::default();

        // Files from GDrive have their external file ID as the path
        for file_id in file_ids {
            let file = match self.api.get_file(&file_id).await {
                Ok(file) => file,
                Err(e) => {
                    result.failed.push(ImportFailure::new(
                        file_id,
                        "Unknown".to_string(),
                        e.to_string(),
                    ));
                    continue;
                }
            };

            let path = storage.absolute_path(&file.name, file.ext);

            // Check for path collision first to prevent downloading file in case it already
            // exists
            let existing_by_path = match self.repo.get_document_by_path(&path, self.api.id()).await
            {
                Ok(doc) => doc,
                Err(e) => {
                    // Handles database errors
                    result
                        .failed
                        .push(ImportFailure::new(file.path.0, file.name, e.to_string()));
                    continue;
                }
            };

            let document = if let Some(existing) = existing_by_path {
                // Document at path already exists, attempt redownload
                if !force_download {
                    tracing::info!(
                        "Document '{}' already exists ({})",
                        existing.name,
                        existing.id
                    );
                    result.skipped.push(existing);
                    continue;
                }

                // Redownload and rehash the content
                tracing::debug!("'{}' - downloading content", existing.name);

                let content = match self.api.download(&file.path.0).await {
                    Ok(content) => content,
                    Err(e) => {
                        result.failed.push(ImportFailure::new(
                            file.path.0,
                            file.name,
                            e.to_string(),
                        ));
                        continue;
                    }
                };

                // Attempt to parse with defaults to check for empty documents.
                if let Err(e) = Parser::default().parse(file.ext, content.as_slice()) {
                    result
                        .failed
                        .push(ImportFailure::new(file.path.0, file.name, e.to_string()));
                    continue;
                }

                let hash = sha256(&content);

                tracing::debug!(
                    "Redownloading document '{}' ({})",
                    existing.name,
                    existing.id
                );

                // Always return errors if there is a hash collision
                // Files from GDrive always have their hashes on them so it's ok to unwrap
                let existing_by_hash = match self.repo.get_document_by_hash(&hash).await {
                    Ok(existing) => existing,
                    Err(e) => {
                        result.failed.push(ImportFailure::new(
                            file.path.0,
                            file.name,
                            e.to_string(),
                        ));
                        continue;
                    }
                };

                if let Some(existing) = existing_by_hash {
                    result.failed.push(ImportFailure::new(
                        file.path.0,
                        file.name,
                        format!(
                            "New document has same hash as existing '{}' ({})",
                            existing.name, existing.id
                        ),
                    ));
                    continue;
                };

                // Write new contents with the overwrite flag enabled
                if let Err(e) = storage.write(&path, &content, true).await {
                    result
                        .failed
                        .push(ImportFailure::new(file.path.0, file.name, e.to_string()));
                    continue;
                };

                // Update the repository entry, its `updated_at` field is also updated
                let update = DocumentParameterUpdate::new(&path, &hash);

                match self
                    .repo
                    .update_document_parameters(existing.id, update)
                    .await
                {
                    Ok(document) => document,
                    Err(e) => {
                        result.failed.push(ImportFailure::new(
                            file.path.0,
                            file.name,
                            e.to_string(),
                        ));
                        continue;
                    }
                }
            } else {
                // Document does not exist, download as usual

                let content = match self.api.download(&file.path.0).await {
                    Ok(content) => content,
                    Err(e) => {
                        result.failed.push(ImportFailure::new(
                            file.path.0,
                            file.name,
                            e.to_string(),
                        ));
                        continue;
                    }
                };

                // Attempt to parse with defaults to check for empty documents.
                if let Err(e) = Parser::default().parse(file.ext, content.as_slice()) {
                    result
                        .failed
                        .push(ImportFailure::new(file.path.0, file.name, e.to_string()));
                    continue;
                }

                let hash = sha256(&content);
                let name = file.name.clone();

                let insert_result = transaction!(self.repo, |tx| async move {
                    let insert = DocumentInsert::new(&name, &path, file.ext, &hash, self.api.id());

                    let document = self
                        .repo
                        .insert_document_with_configs(
                            insert,
                            ParseConfig::Generic(GenericParseConfig::default()),
                            ChunkConfig::snapping_default(),
                            tx,
                        )
                        .await?;

                    match storage.write(&path, &content, false).await {
                        Ok(_) => Ok(document),
                        Err(e) => Err(e),
                    }
                });

                match insert_result {
                    Ok(document) => document,
                    Err(e) => {
                        result.failed.push(ImportFailure::new(
                            file.path.0,
                            file.name,
                            e.to_string(),
                        ));
                        continue;
                    }
                }
            };

            result.success.push(document);
        }

        Ok(result)
    }

    /// Import a single file from an external API. All files imported via this function
    /// will be downloaded and stored locally.
    ///
    /// * `file_id`: The external file ID.
    pub async fn import_document(
        &self,
        file_id: &str,
        force_download: bool,
    ) -> Result<Document, ChonkitError> {
        let file = self.api.get_file(file_id).await?;
        let storage = match self.providers.storage.get_provider(self.api.id()) {
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

        if let Some(doc) = self
            .repo
            .get_document_by_path(&local_path, self.api.id())
            .await?
        {
            if !force_download {
                tracing::error!("Document '{}' already exists ({})", doc.name, doc.id);
                return err!(AlreadyExists, "Document with ID '{}'", doc.id);
            }
            let content = self.api.download(&file.path.0).await?;
            let hash = sha256(&content);
            storage.write(&local_path, &content, true).await?;

            // Triggering updates will update the `updated_at` field
            return self
                .repo
                .update_document_parameters(
                    doc.id,
                    DocumentParameterUpdate::new(&local_path, &hash),
                )
                .await;
        }

        let content = self.api.download(file_id).await?;

        // Attempt to parse with defaults to check for empty documents.
        Parser::default().parse(file.ext, content.as_slice())?;

        let hash = sha256(&content);
        storage.write(&local_path, &content, force_download).await?;

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

        let storage = self.providers.storage.get_provider(self.api.id())?;

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
