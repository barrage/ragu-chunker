use crate::{
    config::FS_STORE_ID,
    core::{
        document::{
            store::{DocumentFile, DocumentStorage, LocalPath},
            DocumentType,
        },
        provider::Identity,
    },
    err,
    error::ChonkitError,
    map_err,
};
use chrono::DateTime;
use std::{os::unix::fs::MetadataExt, path::PathBuf};
use tracing::{debug, info};

/// Local FS based implementation of a document storage.
#[derive(Debug, Clone)]
pub struct FsDocumentStore {
    dir: TokioDirectory,
}

impl FsDocumentStore {
    pub async fn new(dir: &str) -> Self {
        Self {
            dir: TokioDirectory::new(dir).await,
        }
    }
}

impl Identity for FsDocumentStore {
    fn id(&self) -> &'static str {
        FS_STORE_ID
    }
}

#[async_trait::async_trait]
impl DocumentStorage for FsDocumentStore {
    fn absolute_path(&self, path: &str, ext: DocumentType) -> String {
        self.dir.absolute_path(path, ext)
    }

    async fn read(&self, path: &str) -> Result<Vec<u8>, ChonkitError> {
        self.dir.read(path).await
    }

    async fn list_files(&self) -> Result<Vec<DocumentFile<LocalPath>>, ChonkitError> {
        self.dir.list_files().await
    }

    async fn write(&self, path: &str, file: &[u8]) -> Result<(), ChonkitError> {
        self.dir.write(path, file).await
    }

    async fn delete(&self, path: &str) -> Result<(), ChonkitError> {
        self.dir.delete(path).await
    }
}

/// Simple FS operations on a directory based on tokio.
#[derive(Debug, Clone)]
pub struct TokioDirectory {
    /// The base directory for operations.
    base: PathBuf,
}

impl TokioDirectory {
    pub async fn new(path: &str) -> Self {
        info!("Initialising document store at {path}");

        if let Err(e) = tokio::fs::create_dir_all(path).await {
            match e.kind() {
                std::io::ErrorKind::AlreadyExists => {}
                _ => panic!("unable to create directory ({path}): {e}"),
            }
        }

        let base = std::path::absolute(path)
            .unwrap_or_else(|e| panic!("unable to determine absolute path ({path}): {e}"));

        if !base.is_dir() {
            panic!("not a directory: {path}");
        }

        Self { base }
    }

    pub async fn read(&self, path: &str) -> Result<Vec<u8>, ChonkitError> {
        Ok(map_err!(tokio::fs::read(&path).await))
    }

    /// Returns all files from the base directory this struct is instantiated with.
    /// The parameters are as follows:
    /// - `name`: The name of the file.
    /// - `ext`: The extension of the file.
    /// - `path`: The _absolute_ path to the file.
    /// - `modified_at`: The time the file was last modified on the file system.
    pub async fn list_files(&self) -> Result<Vec<DocumentFile<LocalPath>>, ChonkitError> {
        let mut files = vec![];

        let mut entries = map_err!(tokio::fs::read_dir(&self.base).await);

        while let Some(file) = map_err!(entries.next_entry().await) {
            let ext = match self.get_extension(file.path()).await {
                Ok(ext) => ext,
                Err(e) => {
                    tracing::warn!("{e}");
                    continue;
                }
            };

            let name = file.file_name().to_string_lossy().to_string();
            let path = file.path().display().to_string();
            let modified_at = map_err!(file
                .metadata()
                .await
                .map(|meta| DateTime::from_timestamp(meta.mtime(), 0)));

            files.push(DocumentFile::new(name, ext, LocalPath(path), modified_at));
        }

        Ok(files)
    }

    pub async fn write(&self, path: &str, file: &[u8]) -> Result<(), ChonkitError> {
        debug!("Writing {path}");
        match tokio::fs::read(&path).await {
            Ok(_) => {
                err!(AlreadyExists, "File '{path}' at {path}")
            }
            Err(e) => match e.kind() {
                std::io::ErrorKind::NotFound => {
                    map_err!(tokio::fs::write(&path, file).await);
                    Ok(())
                }
                _ => Err(map_err!(Err(e))),
            },
        }
    }

    pub async fn delete(&self, path: &str) -> Result<(), ChonkitError> {
        Ok(map_err!(tokio::fs::remove_file(path).await))
    }

    pub async fn get_extension(&self, pb: PathBuf) -> Result<DocumentType, ChonkitError> {
        if !pb.is_file() {
            return err!(InvalidFile, "not a file: {}", pb.display());
        }

        let Some(ext) = pb.extension() else {
            return err!(InvalidFile, "missing extension: {}", pb.display());
        };

        let Some(ext) = ext.to_str() else {
            return err!(InvalidFile, "extension invalid unicode: {:?}", ext);
        };

        DocumentType::try_from(ext)
    }

    /// Format the path of a document. This function should be used by all implementations
    /// which involve setting the path of a document before it is actually written there.
    #[inline(always)]
    pub fn absolute_path(&self, path: &str, ext: DocumentType) -> String {
        if path.ends_with(&ext.to_string()) {
            return format!("{}/{path}", self.base.display());
        }
        format!("{}/{path}.{ext}", self.base.display())
    }
}

#[cfg(test)]
mod tests {
    use super::FsDocumentStore;
    use crate::core::{
        document::{
            parser::{parse_text, ParseConfig, ParseOutput},
            store::DocumentStorage,
            DocumentType,
        },
        model::document::Document,
    };

    const DIR: &str = "__fs_doc_store_tests";
    const CONTENT: &str = "Hello world.";

    #[tokio::test]
    async fn works() {
        let _ = tokio::fs::remove_dir_all(DIR).await;

        let store = FsDocumentStore::new(DIR).await;

        let d = Document {
            name: "foo".to_string(),
            path: format!("{DIR}/foo"),
            ext: "txt".to_string(),
            ..Default::default()
        };

        let ext = DocumentType::try_from(d.ext).unwrap();
        let path = store.absolute_path(&d.name, ext);
        store.write(&path, CONTENT.as_bytes()).await.unwrap();

        let file = tokio::fs::read_to_string(&path).await.unwrap();
        assert_eq!(CONTENT, file);

        let read = store.read(&path).await.unwrap();
        let content = parse_text(ParseConfig::default(), ext, &read).unwrap();

        assert_eq!(ParseOutput::String(CONTENT.to_string()), content);

        store.delete(&path).await.unwrap();

        tokio::fs::remove_dir(DIR).await.unwrap();
    }
}
