use crate::core::document::parser::{parse, ParseConfig, ParseOutput};
use crate::core::model::document::DocumentSearchColumn;
use crate::core::model::image::{Image, ImageModel};
use crate::core::service::document::dto::{
    ParsedDocumentContent, ParsedDocumentPage, ParsedDocumentSection,
};
use crate::core::token::{TokenCount, Tokenizer};
use crate::{
    config::{DEFAULT_DOCUMENT_CONTENT, DEFAULT_DOCUMENT_NAME, FS_STORE_ID},
    core::{
        chunk::{ChunkConfig, ChunkedDocument},
        document::{sha256, DocumentType, TextDocumentType},
        model::{
            document::{
                Document, DocumentConfig, DocumentDisplay, DocumentInsert, DocumentParameterUpdate,
            },
            List, PaginationSort,
        },
        provider::ProviderState,
        repo::{Atomic, Repository},
    },
    err,
    error::ChonkitError,
    map_err, transaction,
};
use dto::{ChunkForPreview, ChunkPreview, DocumentUpload, ParseOutputPreview, ParsePreview};
use std::time::Instant;
use uuid::Uuid;
use validify::{Validate, Validify};

/// High level operations for document management.
#[derive(Clone)]
pub struct DocumentService {
    repo: Repository,
    providers: ProviderState,
    tokenizer: Tokenizer,
}

impl DocumentService {
    pub fn new(repo: Repository, providers: ProviderState, tokenizer: Tokenizer) -> Self {
        Self {
            repo,
            providers,
            tokenizer,
        }
    }

    /// Get a paginated list of documents from the repository.
    ///
    /// * `p`: Pagination and sorting options.
    /// * `src`: Optional document source to filter by.
    /// * `ready`: If given and `true`, return only documents that are ready for processing.
    pub async fn list_documents(
        &self,
        p: PaginationSort<DocumentSearchColumn>,
        src: Option<&str>,
        ready: Option<bool>,
    ) -> Result<List<Document>, ChonkitError> {
        map_err!(p.validate());
        self.repo.list_documents(p, src, ready).await
    }

    /// Get a paginated list of documents from the repository with additional info for each.
    ///
    /// * `p`: Pagination.
    pub async fn list_documents_display(
        &self,
        p: PaginationSort<DocumentSearchColumn>,
        src: Option<&str>,
    ) -> Result<List<DocumentDisplay>, ChonkitError> {
        map_err!(p.validate());
        self.repo.list_documents_with_collections(p, src).await
    }

    /// Get a document from the repository.
    ///
    /// * `id`: Document ID.
    pub async fn get_document(&self, id: Uuid) -> Result<Document, ChonkitError> {
        match self.repo.get_document_by_id(id).await? {
            Some(doc) => Ok(doc),
            None => err!(DoesNotExist, "Document with ID {id}"),
        }
    }

    /// Get the full config for a document.
    ///
    /// * `id`: Document ID.
    pub async fn get_config(&self, id: Uuid) -> Result<DocumentConfig, ChonkitError> {
        let document = self.repo.get_document_config_by_id(id).await?;

        let Some(document) = document else {
            return err!(DoesNotExist, "Document with ID {id}");
        };

        Ok(document)
    }

    /// Get document chunks using its parsing and chunking configuration,
    /// or the default configurations if they have no configuration.
    ///
    /// * `document`: Document ID.
    /// * `content`: The document's content.
    pub async fn get_chunks<'content>(
        &self,
        document: &Document,
        content: &'content str,
    ) -> Result<ChunkedDocument<'content>, ChonkitError> {
        let Some(config) = self
            .repo
            .get_document_chunk_config(document.id)
            .await?
            .map(|config| config.config)
        else {
            return err!(
                DoesNotExist,
                "Chunking config for document with ID {}",
                document.id
            );
        };

        crate::core::chunk::chunk(&self.providers, config, content).await
    }

    /// Insert the document metadata to the repository and persist it
    /// in the underlying storage implementation.
    ///
    /// * `store`: The storage implementation.
    /// * `params`: Upload params.
    /// * `force`: If `true`, overwrite the document if it already exists. Hash
    ///            collisions always return errors.
    pub async fn upload(
        &self,
        mut params: DocumentUpload<'_>,
        force: bool,
    ) -> Result<Document, ChonkitError> {
        map_err!(params.validify());

        let store = self.providers.document.get_provider(FS_STORE_ID)?;

        let DocumentUpload { ref name, ty, file } = params;
        let path = store.absolute_path(name, ty);
        let hash = sha256(file);

        // Always return errors if there is a hash collision
        if let Some(existing) = self.repo.get_document_by_hash(&hash).await? {
            return err!(
                AlreadyExists,
                "New document '{name}' has same hash as existing '{}' ({})",
                existing.name,
                existing.id
            );
        };

        // The default parser parses the whole input so we use it
        // to check whether the document has any content. Reject if empty.
        // Also since we are sure that the document is new because of no hash collision,
        // we know there are no images in the system belonging to it, so we pass an empty slice.
        let output = parse(ParseConfig::default(), ty, file, &[])?;

        let images = match output {
            ParseOutput::String { text: _, images } => images,
            ParseOutput::Sections(sections) => sections
                .into_iter()
                .flat_map(|s| s.pages)
                .flat_map(|p| p.images)
                .collect(),
        };

        if let Some(existing) = self.repo.get_document_by_path(&path, store.id()).await? {
            if !force {
                return err!(
                    AlreadyExists,
                    "New document '{name}' has same path as existing '{}' ({})",
                    existing.name,
                    existing.id
                );
            }

            store.write(&path, file, true).await?;

            self.process_images(existing.id, images).await?;

            return self
                .repo
                .update_document_parameters(existing.id, DocumentParameterUpdate::new(&path, &hash))
                .await;
        };

        transaction!(self.repo, |tx| async move {
            let insert = DocumentInsert::new(name, &path, ty, &hash, store.id());

            let document = self
                .repo
                .insert_document_with_configs(
                    insert,
                    ParseConfig::default(),
                    ChunkConfig::snapping_default(),
                    tx,
                )
                .await?;

            self.process_images(document.id, images).await?;

            store.write(&path, file, false).await?;

            Ok(document)
        })
    }

    /// Remove the document from the repo and delete it from the storage.
    ///
    /// * `id`: Document ID.
    pub async fn delete(&self, id: Uuid) -> Result<(), ChonkitError> {
        let Some(document) = self.repo.get_document_by_id(id).await? else {
            return err!(DoesNotExist, "Document with ID {id}");
        };
        let collections = self
            .repo
            .get_document_assigned_collections(document.id)
            .await?;
        let store = self.providers.document.get_provider(&document.src)?;
        transaction! {self.repo, |tx| async move {
                self.repo.remove_document_by_id(document.id, Some(tx)).await?;
                for (_, name, provider) in collections {
                    let vector_db = self.providers.vector.get_provider(&provider)?;
                    vector_db
                        .delete_embeddings(&name, document.id)
                        .await?;
                }
                store.delete(&document.path).await
            }
        }
    }

    /// Sync storage contents with the repo.
    pub async fn sync(&self, provider: &str) -> Result<(), ChonkitError> {
        let __start = Instant::now();

        let store = self.providers.document.get_provider(provider)?;

        tracing::info!("Syncing documents with {}", store.id());

        store.sync(&self.repo).await?;

        tracing::info!(
            "Syncing finished for storage '{}', took {}ms",
            store.id(),
            __start.elapsed().as_millis()
        );

        Ok(())
    }

    /// Chunk the document without saving any embeddings. Useful for previewing.
    ///
    /// * `document_id`: ID of the document to chunk.
    /// * `config`: Chunking configuration.
    pub async fn chunk_preview(
        &self,
        document_id: Uuid,
        config: dto::ChunkPreviewPayload,
    ) -> Result<ChunkPreview, ChonkitError> {
        map_err!(config.validate());

        let parse_config = match config.parse_config {
            Some(cfg) => cfg,
            None => self
                .get_config(document_id)
                .await?
                .parse_config
                .unwrap_or_default(),
        };

        let ParsePreview {
            content: text,
            total_tokens,
        } = self.parse_preview(document_id, parse_config).await?;

        match text {
            ParseOutputPreview::String(ParsedDocumentContent { text, images }) => {
                let Some(chunker) = config.chunker else {
                    return err!(InvalidParameter, "Chunking configuration must be specified when previewing with generic parser");
                };

                let mut total_tokens_post = TokenCount::default();

                let chunks =
                    match crate::core::chunk::chunk(&self.providers, chunker, &text).await? {
                        ChunkedDocument::Ref(chunked) => chunked
                            .into_iter()
                            .map(|s| {
                                let token_count = self.tokenizer.count(s);
                                total_tokens_post += token_count;
                                ChunkForPreview {
                                    token_count,
                                    chunk: s.to_string(),
                                }
                            })
                            .collect(),
                        ChunkedDocument::Owned(chunked) => chunked
                            .into_iter()
                            .map(|s| {
                                let token_count = self.tokenizer.count(&s);
                                total_tokens_post += token_count;
                                ChunkForPreview {
                                    token_count,
                                    chunk: s,
                                }
                            })
                            .collect(),
                    };

                Ok(ChunkPreview {
                    chunks,
                    images,
                    total_tokens_pre: total_tokens,
                    total_tokens_post,
                })
            }
            ParseOutputPreview::Sections(sections) => {
                // TODO: configure section merge strategy
                // currently we are merging them, but we should also enable storing page by page
                let mut total_tokens = TokenCount::default();
                let mut chunks = vec![];
                let mut images = vec![];

                for section in sections.into_iter() {
                    let content = section.pages.iter().fold(String::new(), |mut acc, el| {
                        acc.push_str(&el.content);
                        acc.push('\n');
                        acc
                    });
                    let count = self.tokenizer.count(&content);
                    chunks.push(ChunkForPreview {
                        token_count: count,
                        chunk: content,
                    });
                    images.extend(section.pages.into_iter().flat_map(|p| p.images));
                    total_tokens += count;
                }

                Ok(ChunkPreview {
                    chunks,
                    images,
                    total_tokens_pre: total_tokens,
                    total_tokens_post: total_tokens,
                })
            }
        }
    }

    /// Preview the output of parsing a document. This uses the [parse] function internally,
    /// but remaps the output for display purposes.
    ///
    /// During preview, users can describe images found in documents and can adjust the parser
    /// to enhance the output.
    ///
    /// * `id`: Document ID.
    /// * `config`: Parsing configuration.
    pub async fn parse_preview(
        &self,
        id: Uuid,
        config: ParseConfig,
    ) -> Result<ParsePreview, ChonkitError> {
        let (document, content) = self.get_document_with_content(id).await?;

        // Load existing images to prevent unnecessary parsing

        let mut existing_images = self
            .repo
            .list_all_document_images(id, self.providers.image.id())
            .await?;

        // Remove the extension when doing lookup since we cannot know what it
        // is in advance

        let existing_image_paths = existing_images
            .iter()
            .filter_map(|i| Some(i.path.split_once('.')?.0.to_string()))
            .collect::<Vec<_>>();

        let ext = document.ext.as_str().try_into()?;

        let output = map_err!(
            tokio::task::spawn_blocking(move || {
                parse(config, ext, &content, &existing_image_paths)
            })
            .await
        )?;

        match output {
            ParseOutput::String { text, images } => {
                let parsed_images = self.process_images(id, images).await?;
                existing_images.extend(parsed_images);
                Ok(ParsePreview {
                    total_tokens: self.tokenizer.count(&text),
                    content: dto::ParseOutputPreview::String(ParsedDocumentContent {
                        text,
                        images: existing_images,
                    }),
                })
            }
            ParseOutput::Sections(document_sections) => {
                let mut total_tokens = TokenCount::default();
                let mut sections = vec![];

                for section in document_sections {
                    let mut pages = vec![];

                    for page in section.pages {
                        let mut page_images = existing_images
                            .iter()
                            .filter(|image| image.page_number == page.number as i32)
                            .cloned()
                            .collect::<Vec<_>>();

                        page_images.extend(self.process_images(id, page.images).await?);

                        total_tokens += self.tokenizer.count(&page.content);

                        pages.push(ParsedDocumentPage {
                            content: page.content,
                            number: page.number,
                            images: page_images,
                        });
                    }

                    sections.push(ParsedDocumentSection { pages })
                }

                Ok(ParsePreview {
                    total_tokens,
                    content: dto::ParseOutputPreview::Sections(sections),
                })
            }
        }
    }

    /// Update a document's parsing configuration.
    ///
    /// * `id`: Document ID.
    /// * `config`: Parsing configuration.
    pub async fn update_parser(&self, id: Uuid, config: ParseConfig) -> Result<(), ChonkitError> {
        map_err!(config.validate());

        if self.repo.get_document_by_id(id).await?.is_none() {
            return err!(DoesNotExist, "Document with ID {id}");
        }

        self.repo.upsert_document_parse_config(id, config).await?;

        Ok(())
    }

    /// Update a document's chunking configuration.
    ///
    /// * `id`: Document ID.
    /// * `config`: Chunking configuration.
    pub async fn update_chunker(&self, id: Uuid, config: ChunkConfig) -> Result<(), ChonkitError> {
        if self.repo.get_document_by_id(id).await?.is_none() {
            return err!(DoesNotExist, "Document with ID {id}");
        }

        self.repo.upsert_document_chunk_config(id, config).await?;

        Ok(())
    }

    /// Creates the default document if no other document exists.
    pub async fn create_default_document(&self) {
        let count = self.repo.get_document_count().await.unwrap_or(0);

        if count > 0 {
            tracing::info!("Found existing documents, skipping default document creation");
            return;
        }

        match self
            .upload(
                DocumentUpload::new(
                    String::from(DEFAULT_DOCUMENT_NAME),
                    DocumentType::Text(TextDocumentType::Txt),
                    DEFAULT_DOCUMENT_CONTENT.as_bytes(),
                ),
                false,
            )
            .await
        {
            Ok(_) => tracing::info!("Created default document '{DEFAULT_DOCUMENT_NAME}'"),
            Err(e) => {
                if let crate::error::ChonkitErr::AlreadyExists(_) = e.error {
                    tracing::info!("Default document '{DEFAULT_DOCUMENT_NAME}' already exists");
                }
            }
        }
    }

    pub async fn get_document_with_content(
        &self,
        id: Uuid,
    ) -> Result<(Document, Vec<u8>), ChonkitError> {
        let document = self.repo.get_document_by_id(id).await?;

        let Some(document) = document else {
            return err!(DoesNotExist, "Document with ID {id}");
        };

        let store = self.providers.document.get_provider(&document.src)?;

        let content = store.read(&document.path).await?;

        Ok((document, content))
    }

    pub async fn list_images(&self, document_id: Uuid) -> Result<List<ImageModel>, ChonkitError> {
        self.repo
            .list_document_images(document_id, self.providers.image.id())
            .await
    }

    pub async fn update_image_description(
        &self,
        document_id: Uuid,
        image_path: &str,
        description: Option<&str>,
    ) -> Result<(), ChonkitError> {
        self.repo
            .update_image_description(document_id, image_path, description)
            .await
    }

    async fn process_images(
        &self,
        document_id: Uuid,
        images: Vec<Image>,
    ) -> Result<Vec<ImageModel>, ChonkitError> {
        if images.is_empty() {
            return Ok(vec![]);
        }

        let total = images.len();
        tracing::debug!("Processing {} images in document {document_id}", total);

        let mut parsed_images = vec![];

        let existing_paths = self
            .repo
            .list_document_image_paths(document_id, self.providers.image.id())
            .await?;

        for (i, image) in images.into_iter().enumerate() {
            if existing_paths.contains(&image.path()) {
                parsed_images.push(ImageModel {
                    path: image.path(),
                    page_number: image.page_number as i32,
                    image_number: image.image_number as i32,
                    format: image.image.format.extensions_str()[0].to_string(),
                    hash: image.hash(),
                    document_id,
                    src: self.providers.image.id().to_string(),
                    description: image.description,
                    width: image.image.width as i32,
                    height: image.image.height as i32,
                });
                continue;
            }

            let img = match self.providers.image.store_image(document_id, &image).await {
                Ok(img) => img,
                Err(e) => {
                    tracing::error!(
                        "Unable to store image ({}) in document {document_id}: {e}",
                        image.path()
                    );
                    continue;
                }
            };

            parsed_images.push(img);

            tracing::debug!("Uploaded image {}/{}", i + 1, total);
        }

        Ok(parsed_images)
    }
}

/// Document service DTOs.
pub mod dto {
    use crate::core::{
        chunk::ChunkConfig,
        document::{parser::ParseConfig, DocumentType},
        model::image::ImageModel,
        token::TokenCount,
    };
    use serde::{Deserialize, Serialize};
    use validify::{Validate, Validify};

    #[derive(Debug, Clone, Validify)]
    pub struct DocumentUpload<'a> {
        /// Document name.
        #[modify(trim)]
        #[validate(length(min = 1, message = "Document name cannot be empty."))]
        pub name: String,

        /// Document extension.
        pub ty: DocumentType,

        /// Document file.
        pub file: &'a [u8],
    }

    impl<'a> DocumentUpload<'a> {
        pub fn new(name: String, ty: DocumentType, file: &'a [u8]) -> Self {
            Self { name, ty, file }
        }
    }

    /// DTO used for previewing chunks.
    #[derive(Debug, Deserialize, Validate, utoipa::ToSchema)]
    #[serde(rename_all = "camelCase")]
    pub struct ChunkPreviewPayload {
        /// Parsing configuration.
        #[serde(alias = "parser")]
        pub parse_config: Option<ParseConfig>,

        /// Chunking configuration.
        pub chunker: Option<ChunkConfig>,
    }

    #[derive(Debug, Serialize, utoipa::ToSchema)]
    #[serde(rename_all = "camelCase")]
    pub struct ChunkForPreview {
        pub chunk: String,
        pub token_count: TokenCount,
    }

    #[derive(Debug, Serialize, utoipa::ToSchema)]
    #[serde(rename_all = "camelCase")]
    pub struct ChunkPreview {
        pub chunks: Vec<ChunkForPreview>,
        pub images: Vec<ImageModel>,
        pub total_tokens_pre: TokenCount,
        pub total_tokens_post: TokenCount,
    }

    #[derive(Debug, Serialize, utoipa::ToSchema)]
    #[serde(rename_all = "camelCase")]
    pub struct ParsePreview {
        pub content: ParseOutputPreview,
        pub total_tokens: TokenCount,
    }

    #[derive(Debug, Serialize, utoipa::ToSchema)]
    #[serde(rename_all = "camelCase")]
    pub enum ParseOutputPreview {
        String(ParsedDocumentContent),
        Sections(Vec<ParsedDocumentSection>),
    }

    /// Service level DTO for representing parsed document content with images.
    #[derive(Debug, Serialize, utoipa::ToSchema)]
    #[serde(rename_all = "camelCase")]
    pub struct ParsedDocumentContent {
        pub text: String,
        pub images: Vec<ImageModel>,
    }

    /// Service level [DocumentSection][crate::core::document::parser::DocumentSection].
    #[derive(Debug, Default, PartialEq, Serialize, Deserialize, utoipa::ToSchema)]
    #[serde(rename_all = "camelCase")]
    pub struct ParsedDocumentSection {
        pub pages: Vec<ParsedDocumentPage>,
    }

    /// Service level [DocumentPage][crate::core::document::parser::DocumentPage].
    #[derive(Debug, Default, PartialEq, Serialize, Deserialize, utoipa::ToSchema)]
    #[serde(rename_all = "camelCase")]
    pub struct ParsedDocumentPage {
        /// The text contents of the page.
        pub content: String,

        /// The page number.
        pub number: usize,

        pub images: Vec<ImageModel>,
    }
}
