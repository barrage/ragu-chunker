use super::{parser::TextParseConfig, sha256, DocumentType};
use crate::{
    core::{
        chunk::ChunkConfig,
        model::document::{Document, DocumentInsert},
        provider::Identity,
        repo::Repository,
    },
    error::ChonkitError,
    map_err,
};
use chrono::{DateTime, Utc};

pub mod external;

/// Represents a file of the document obtained from storage providers and external APIs.
#[derive(Debug)]
pub struct DocumentFile<T> {
    /// The name of the file.
    pub name: String,

    /// File extension.
    pub ext: DocumentType,

    /// The location of the file, depending where it is obtained from.
    /// Document storage implementations will have this set to [LocalPath].
    /// External APIs will have this set to [ExternalPath].
    pub path: T,

    /// Last modification time of document. Used to sync.
    pub modified_at: Option<DateTime<Utc>>,
}

impl<T> DocumentFile<T> {
    pub fn new(
        name: String,
        ext: DocumentType,
        path: T,
        modified_at: Option<DateTime<Utc>>,
    ) -> Self {
        Self {
            name,
            ext,
            path,
            modified_at,
        }
    }
}

/// A path on the FS.
#[derive(Debug)]
pub struct LocalPath(pub String);

/// A _path_ on an external document provider.
/// For external APIs, these usually represent file IDs.
#[derive(Debug)]
pub struct ExternalPath(pub String);

/// Use on adapters that use the file system to store and read documents.
/// Serves to differentiate document sources.
#[async_trait::async_trait]
pub trait DocumentStorage: Identity {
    /// Used to format paths to absolute as stored in the repository.
    /// Returns the absolute path of the document without storing anything.
    ///
    /// * `name`: The name of the document.
    /// * `ext`: The extension of the document.
    fn absolute_path(&self, name: &str, ext: DocumentType) -> String;

    /// Get the content of a document located on `path` and parse it.
    ///
    /// * `path`: The unique path of the document. Implementation specific.
    async fn read(&self, path: &str) -> Result<Vec<u8>, ChonkitError>;

    /// List all files in the storage.
    async fn list_files(&self) -> Result<Vec<DocumentFile<LocalPath>>, ChonkitError>;

    /// Delete the document contents from the underlying storage.
    ///
    /// * `path`: The path to the file to delete.
    async fn delete(&self, path: &str) -> Result<(), ChonkitError>;

    /// Write `contents` to the storage implementation.
    /// Returns the absolute path of where the file was written.
    ///
    /// * `path`: The _absolute_ file path.
    /// * `content`: What to write.
    /// * `overwrite`: If `true`, overwrite the file if it already exists, return an error otherwise.
    async fn write(&self, path: &str, content: &[u8]) -> Result<(), ChonkitError>;

    /// Sync repository entries with the files currently located in the storage.
    /// Any existing files in the storage must be added to the repository if not
    /// present. Any files no longer on the file system must be removed from the
    /// repository.
    ///
    /// * `repository`: The repository to sync.
    async fn sync(&self, repo: &Repository) -> Result<(), ChonkitError> {
        // List all documents from repository
        let document_paths = repo.list_all_document_paths(self.id()).await?;

        // Prune any documents that no longer exist on the file system
        for (id, path) in document_paths.iter() {
            if let Err(e) = tokio::fs::metadata(path).await {
                match e.kind() {
                    std::io::ErrorKind::NotFound => {
                        tracing::info!("{path} - not found in storage, removing");
                        repo.remove_document_by_id(*id, None).await?;
                        continue;
                    }
                    _ => return map_err!(Err(e)),
                }
            }
        }

        // List all files from storage
        let files = self.list_files().await?;

        for file in files {
            // Check if document already exists
            // The path and provider combo ensure a document is unique
            let doc = repo.get_document_by_path(&file.path.0, self.id()).await?;

            if let Some(Document { id, name, .. }) = doc {
                tracing::info!("{id} - '{name}' already exists");
                if let Err(e) = repo.get_document_config_by_id(id).await {
                    tracing::error!("{id} - error loading config: {e}");
                    tracing::debug!("{id} - attempting to upsert configuration");

                    if let Err(e) = repo
                        .upsert_document_chunk_config(id, ChunkConfig::snapping_default())
                        .await
                    {
                        tracing::error!("{id} - error updating chunk config: {e}");
                    }

                    if let Err(e) = repo
                        .upsert_document_parse_config(id, TextParseConfig::default())
                        .await
                    {
                        tracing::error!("{id} - error updating parsing config: {e}");
                    }
                }
                continue;
            }

            let hash = sha256(&map_err!(tokio::fs::read(&file.path.0).await));

            let result = repo
                .transaction(|tx| {
                    Box::pin(async move {
                        let insert = DocumentInsert::new(
                            &file.name,
                            &file.path.0,
                            file.ext,
                            &hash,
                            self.id(),
                        );
                        repo.insert_document_with_configs(
                            insert,
                            TextParseConfig::default(),
                            ChunkConfig::snapping_default(),
                            tx,
                        )
                        .await
                    })
                })
                .await;

            match result {
                Ok(Document { id, name, .. }) => {
                    tracing::info!("{id} - successfully inserted '{name}'")
                }
                Err(e) => tracing::error!("{e}"),
            }
        }

        Ok(())
    }
}
