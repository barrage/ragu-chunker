use super::{DocumentFile, ExternalPath};
use crate::{core::provider::Identity, error::ChonkitError};

/// Implement on API clients that obtain documents from an external source.
///
/// Every implementation of this trait must have a matching implementation
/// of [DocumentStorage][super::DocumentStorage] whose [Identity] implementation
/// returns the same identifier as the implementation of this trait.
#[async_trait::async_trait]
pub trait ExternalDocumentStorage: Identity {
    /// List file info based on the provided file identifiers.
    ///
    /// * `file_ids`: Optional list of file identifiers to filter by. If `None`, lists all files.
    async fn list_files(
        &self,
        file_ids: Option<&[String]>,
    ) -> Result<Vec<DocumentFile<ExternalPath>>, ChonkitError>;

    /// Get file info based on the provided file identifiers.
    ///
    /// * `file_id`: Storage specific file identifier.
    async fn get_file(&self, file_id: &str) -> Result<DocumentFile<ExternalPath>, ChonkitError>;

    /// Get the raw bytes of a document based on the `file_id`.
    ///
    /// * `file_id`: Unique identifier of the document.
    async fn download(&self, file_id: &str) -> Result<Vec<u8>, ChonkitError>;
}
