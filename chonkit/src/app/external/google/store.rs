use crate::{
    app::document::store::TokioDirectory,
    config::GOOGLE_STORE_ID,
    core::{
        document::{
            store::{DocumentFile, DocumentStorage, LocalPath},
            DocumentType,
        },
        provider::Identity,
    },
    error::ChonkitError,
};

/// Holds imported documents from Google Drive. Provides utilities
/// for accessing the Google Drive API and the local file system where
/// files from Drive are stored.
///
/// The [DocumentStorage] implementation for this struct is exclusively file
/// system based and all calls are delegated to the underlying [TokioDirectory].
/// Any operations that require access to the Drive API must be done beforehand
/// with `GoogleDriveApi`.
///
/// Imported files users import from their Drive will be downloaded to the file system.
/// This makes it easier to work with the existing core services and keeps any
/// API related operations isolated.
///
/// Corresponds to the [GoogleDriveApi][super::drive::GoogleDriveApi] external
/// storage implementation.
///
/// All file paths written by this store will be the file identifiers from the
/// API suffixed by the file's extension, i.e. `<FILE_ID>.<EXT>`.
#[derive(Debug, Clone)]
pub struct GoogleDriveStore {
    dir: TokioDirectory,
}

impl GoogleDriveStore {
    /// Construct a new instance of the store at the provided path.
    /// The path must be a directory.
    /// All files imported from Drive will be downloaded to this path.
    pub async fn new(base: &str) -> Self {
        Self {
            dir: TokioDirectory::new(base).await,
        }
    }
}

impl Identity for GoogleDriveStore {
    fn id(&self) -> &'static str {
        GOOGLE_STORE_ID
    }
}

#[async_trait::async_trait]
impl DocumentStorage for GoogleDriveStore {
    fn absolute_path(&self, path: &str, ext: DocumentType) -> String {
        self.dir.absolute_path(path, ext)
    }

    async fn read(&self, path: &str) -> Result<Vec<u8>, ChonkitError> {
        self.dir.read(path).await
    }

    async fn list_files(&self) -> Result<Vec<DocumentFile<LocalPath>>, ChonkitError> {
        self.dir.list_files().await
    }

    async fn delete(&self, path: &str) -> Result<(), ChonkitError> {
        self.dir.delete(path).await
    }

    async fn write(&self, path: &str, content: &[u8], overwrite: bool) -> Result<(), ChonkitError> {
        self.dir.write(path, content, overwrite).await
    }
}

#[cfg(test)]
mod tests {
    use super::GoogleDriveStore;
    use crate::core::{
        document::{
            parser::{parse, ParseConfig, ParseOutput},
            store::DocumentStorage,
            DocumentType,
        },
        model::document::Document,
    };

    const DIR: &str = "__gdrive_doc_store_tests";
    const CONTENT: &str = "Hello world.";

    #[tokio::test]
    async fn works() {
        let store = GoogleDriveStore::new(DIR).await;

        let d = Document {
            name: "foo".to_string(),
            path: format!("{DIR}/foo"),
            ext: "txt".to_string(),
            ..Default::default()
        };

        let ext = DocumentType::try_from(d.ext.clone()).unwrap();
        let path = store.absolute_path(&d.name, ext);
        store.write(&path, CONTENT.as_bytes(), false).await.unwrap();

        let file = tokio::fs::read_to_string(&path).await.unwrap();
        assert_eq!(CONTENT, file);
        let read = store.read(&path).await.unwrap();

        let content = parse(ParseConfig::default(), ext, read.as_slice())
            .await
            .unwrap();

        assert_eq!(
            content,
            ParseOutput::String {
                text: CONTENT.to_string(),
                images: vec![]
            }
        );

        store.delete(&path).await.unwrap();

        tokio::fs::remove_dir(DIR).await.unwrap();
    }
}
