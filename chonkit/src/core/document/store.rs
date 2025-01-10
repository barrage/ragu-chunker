use chrono::{DateTime, Utc};

use super::{parser::Parser, sha256};
use crate::{
    core::{
        model::document::{Document, DocumentInsert, DocumentType},
        provider::Identity,
        repo::{document::DocumentRepo, Repository},
    },
    error::ChonkitError,
    map_err,
};

pub mod external;

#[derive(Debug)]
pub struct DocumentStoreFile {
    pub name: String,
    pub ext: DocumentType,
    pub path: String,
    pub modified_at: Option<DateTime<Utc>>,
    pub hash: Option<String>,
}

impl DocumentStoreFile {
    pub fn new(
        name: String,
        ext: DocumentType,
        path: String,
        modified_at: Option<DateTime<Utc>>,
        hash: Option<String>,
    ) -> Self {
        Self {
            name,
            ext,
            path,
            modified_at,
            hash,
        }
    }
}

/// Use on adapters that use the file system to store and read documents.
/// Serves as indirection to decouple the documents from their source.
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
    /// * `parser`: Parser to use for obtaining the text content.
    async fn read(&self, path: &str, parser: &Parser) -> Result<String, ChonkitError>;

    /// List all files in the storage.
    async fn list_files(&self) -> Result<Vec<DocumentStoreFile>, ChonkitError>;

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
    async fn write(&self, path: &str, content: &[u8], overwrite: bool) -> Result<(), ChonkitError>;

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
                        tracing::info!("Document '{}' not found in storage, removing", path);
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
            let doc = repo.get_document_by_path(&file.path, self.id()).await?;

            if let Some(Document { id, name, .. }) = doc {
                tracing::info!("Document '{name}' already exists ({id})");
                continue;
            }

            let hash = sha256(&map_err!(tokio::fs::read(&file.path).await));

            let insert = DocumentInsert::new(&file.name, &file.path, file.ext, &hash, self.id());

            match repo.insert_document(insert).await {
                Ok(Document { id, name, .. }) => {
                    tracing::info!("Successfully inserted '{name}' ({id})")
                }
                Err(e) => tracing::error!("{e}"),
            }
        }

        Ok(())
    }
}
