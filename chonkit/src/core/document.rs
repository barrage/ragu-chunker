use std::{collections::HashSet, sync::Arc};

use crate::{
    core::{
        chunk::ChunkConfig,
        document::{parser::ParseConfig, store::DocumentStorage},
        image::ImageStorage,
        model::{
            document::{Document, DocumentInsert},
            image::{Image, ImageModel, InsertImage},
        },
        repo::Repository,
    },
    err,
    error::ChonkitError,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use uuid::Uuid;

/// Parsing implementations for various file types.
pub mod parser;

/// File system storage implementations.
pub mod store;

/// All possible file types chonkit can process.
#[derive(Debug, Clone, Copy, Serialize)]
pub enum DocumentType {
    /// Encapsulates any files that can be read as strings.
    /// Does not necessarily have to be `.txt`, could be `.json`, `.csv`, etc.
    Text(TextDocumentType),

    /// Microschlong steaming pile of garbage document.
    Docx,

    /// PDF document.
    Pdf,

    Excel,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub enum TextDocumentType {
    Md,
    Xml,
    Json,
    Csv,
    Txt,
}

impl DocumentType {
    pub fn try_from_file_name(name: &str) -> Result<Self, ChonkitError> {
        let Some((_, ext)) = name.rsplit_once('.') else {
            return err!(UnsupportedFileType, "{name} - missing extension");
        };
        Self::try_from(ext)
    }
}

impl std::fmt::Display for DocumentType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DocumentType::Text(ty) => match ty {
                TextDocumentType::Md => write!(f, "md"),
                TextDocumentType::Xml => write!(f, "xml"),
                TextDocumentType::Json => write!(f, "json"),
                TextDocumentType::Csv => write!(f, "csv"),
                TextDocumentType::Txt => write!(f, "txt"),
            },
            DocumentType::Docx => write!(f, "docx"),
            DocumentType::Pdf => write!(f, "pdf"),
            DocumentType::Excel => write!(f, "xlsx"),
        }
    }
}

impl TryFrom<&str> for DocumentType {
    type Error = ChonkitError;

    /// Implementation that should be given either the document extension or a mime type.
    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "xlsx" | "application/vnd.google-apps.spreadsheet" => Ok(Self::Excel),
            "md" => Ok(Self::Text(TextDocumentType::Md)),
            "xml" => Ok(Self::Text(TextDocumentType::Xml)),
            "json" | "application/json" => Ok(Self::Text(TextDocumentType::Json)),
            "csv" => Ok(Self::Text(TextDocumentType::Csv)),
            "txt" | "text/plain" => Ok(Self::Text(TextDocumentType::Txt)),
            "pdf" | "application/pdf" => Ok(Self::Pdf),
            "docx"
            | "application/vnd.google-apps.document"
            | "application/vnd.openxmlformats-officedocument.wordprocessingml.document" => {
                Ok(Self::Docx)
            }
            _ => err!(UnsupportedFileType, "{value}"),
        }
    }
}

impl TryFrom<String> for DocumentType {
    type Error = ChonkitError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.as_str().try_into()
    }
}

/// Return a SHA256 hash of the input.
///
/// * `input`: Input bytes.
pub fn sha256(input: &[u8]) -> String {
    let mut hasher = Sha256::new();
    Digest::update(&mut hasher, input);
    let out = hasher.finalize();
    hex::encode(out)
}

pub async fn get_image(
    repo: Repository,
    provider: &(dyn ImageStorage + Send + Sync),
    id: Uuid,
) -> Result<(Image, ImageModel), ChonkitError> {
    let Some(meta) = repo.get_image_by_id(id).await? else {
        return err!(DoesNotExist, "Image with ID {id}");
    };

    if meta.src != provider.id() {
        return err!(DoesNotExist, "Image with ID {id}");
    }

    let format = match image::ImageFormat::from_path(&meta.path) {
        Ok(f) => f,
        Err(e) => {
            tracing::error!("Failed to parse image format: {e}");
            return err!(InvalidFile, "Failed to parse image format: {e}");
        }
    };

    let bytes = provider.get_image(&meta.path).await?;

    let data = Image::new(
        meta.page_number.map(|p| p as usize),
        meta.image_number.map(|p| p as usize),
        bytes,
        format,
        meta.width as u32,
        meta.height as u32,
    );

    Ok((data, meta))
}

pub(in crate::core) async fn store_document(
    repo: &Repository,
    store: &(dyn DocumentStorage + Send + Sync),
    name: &str,
    ty: DocumentType,
    file: &[u8],
) -> Result<Document, ChonkitError> {
    let path = store.absolute_path(name, ty);
    let hash = sha256(file);

    // Always return errors if there is a hash collision
    if let Some(existing) = repo.get_document_by_hash(&hash).await? {
        return err!(
            AlreadyExists,
            "New document '{name}' has same hash as existing '{}' ({})",
            existing.name,
            existing.id
        );
    };

    repo.transaction(|tx| {
        Box::pin(async {
            let insert = DocumentInsert::new(name, &path, ty, &hash, store.id());

            let document = repo
                .insert_document_with_configs(
                    insert,
                    ParseConfig::default(),
                    ChunkConfig::snapping_default(),
                    tx,
                )
                .await?;

            store.write(&path, file).await?;

            Ok(document)
        })
    })
    .await
}

/// Process document images in a background tokio job.
pub(in crate::core) async fn process_document_images(
    repo: Repository,
    storage: Arc<dyn ImageStorage + Send + Sync>,
    document_id: Uuid,
    ty: DocumentType,
    file: Vec<u8>,
) -> Result<(), ChonkitError> {
    let existing_images = repo
        .list_all_document_images(document_id, storage.id())
        .await?
        .into_iter()
        .filter_map(|image| Some((image.page_number? as usize, image.image_number? as usize)))
        .collect::<HashSet<_>>();

    let existing_amount = existing_images.len();

    tokio::spawn(async move {
        let images = match tokio::task::spawn_blocking(move || {
            match parser::parse_images(ty, &file, &existing_images) {
                Ok(i) => i,
                Err(e) => {
                    tracing::error!("error parsing images: {}", e);
                    vec![]
                }
            }
        })
        .await
        {
            Ok(i) => i,
            Err(e) => {
                tracing::error!("error joining image parsing job: {}", e);
                return;
            }
        };

        match store_images(repo, storage, Some(document_id), images).await {
            Ok(i) => tracing::info!("Parsed {} images ({} skipped)", i.len(), existing_amount),
            Err(_) => todo!(),
        };
    });

    Ok(())
}

/// Store the provided images in document storage.
pub(in crate::core) async fn store_images(
    repo: Repository,
    storage: Arc<dyn ImageStorage + Send + Sync>,
    document_id: Option<Uuid>,
    images: Vec<Image>,
) -> Result<Vec<ImageModel>, ChonkitError> {
    if images.is_empty() {
        return Ok(vec![]);
    }

    let total = images.len();
    tracing::debug!("Processing {} images", total);

    let mut parsed_images = vec![];

    for (i, image) in images.into_iter().enumerate() {
        if let Err(e) = repo
            .transaction(|tx| {
                Box::pin(async {
                    let img = repo
                        .insert_image(
                            InsertImage {
                                document_id,
                                page_number: image.page_number,
                                image_number: image.image_number,
                                path: &image.path(),
                                hash: &image.hash().0,
                                src: storage.id(),
                                format: image.image.format.extensions_str()[0],
                                description: image.description.as_deref(),
                                width: image.image.width,
                                height: image.image.height,
                            },
                            Some(tx),
                        )
                        .await?;

                    storage.store_image(&image).await?;

                    parsed_images.push(img);

                    Ok(())
                })
            })
            .await
        {
            tracing::error!("Unable to store image ({}): {e}", image.path());
            continue;
        }

        if i % 100 == 0 || i == total.saturating_sub(1) {
            tracing::debug!("Uploaded image {}/{}", i + 1, total);
        }
    }

    Ok(parsed_images)
}
