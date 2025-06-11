use crate::core::document::parser::{parse_text, ParseConfig, ParseOutput};
use crate::core::document::{get_image, process_document_images, store_document, store_images};
use crate::core::model::document::DocumentSearchColumn;
use crate::core::model::image::{Image, ImageData, ImageModel};
use crate::core::service::document::dto::{
    ListImagesParameters, ParsedDocumentPage, ParsedDocumentSection,
};
use crate::core::token::{TokenCount, Tokenizer};
use crate::{
    config::{DEFAULT_DOCUMENT_CONTENT, DEFAULT_DOCUMENT_NAME, FS_STORE_ID},
    core::{
        chunk::{ChunkConfig, ChunkedDocument},
        document::{DocumentType, TextDocumentType},
        model::{
            document::{Document, DocumentConfig, DocumentDisplay},
            List, PaginationSort,
        },
        provider::ProviderState,
        repo::Repository,
    },
    err,
    error::ChonkitError,
    map_err,
};
use dto::{ChunkForPreview, ChunkPreview, DocumentUpload, ParseOutputPreview, ParsePreview};
use std::time::Instant;
use uuid::Uuid;
use validify::{Validate, Validify};

/// High level operations for document management.
///
/// Documents include textual documents, as well as images
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

    pub async fn process_document_images(&self, id: Uuid) -> Result<(), ChonkitError> {
        let Some(document) = self.repo.get_document_by_id(id).await? else {
            return err!(DoesNotExist, "Document with ID {id}");
        };

        let file = self
            .providers
            .document
            .get_provider(&document.src)?
            .read(&document.path)
            .await?;

        process_document_images(
            self.repo.clone(),
            self.providers.image.clone(),
            id,
            DocumentType::try_from(document.ext.as_str())?,
            file,
        )
        .await?;

        Ok(())
    }

    /// Insert the document metadata to the repository and persist it
    /// in the underlying storage implementation.
    ///
    /// * `store`: The storage implementation.
    /// * `params`: Upload params.
    /// * `force`: If `true`, overwrite the document if it already exists. Hash
    ///            collisions always return errors.
    pub async fn upload(&self, mut params: DocumentUpload<'_>) -> Result<Document, ChonkitError> {
        map_err!(params.validify());

        let DocumentUpload { ref name, ty, file } = params;

        let img_store = self.providers.image.clone();
        let doc_store = self.providers.document.get_provider(FS_STORE_ID)?;

        let document = store_document(&self.repo, &*doc_store, name, ty, file).await?;

        // Process images in the background as it can take a while

        let file = file.to_vec();

        process_document_images(self.repo.clone(), img_store, document.id, ty, file).await?;

        Ok(document)
    }

    /// Remove the document from the repo, delete it from storage, delete all of its images,
    /// and remove all of its text embeddings and image embeddings from all vector databases.
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

        let images = self
            .repo
            .list_all_document_images(document.id, self.providers.image.id())
            .await?;

        let store = self.providers.document.get_provider(&document.src)?;
        let image_store = &self.providers.image;

        for (_, name, provider) in collections {
            let result: Result<(), ChonkitError> = async {
                let vector_db = self.providers.vector.get_provider(&provider)?;

                // Remove text embeddings from all found collections
                vector_db.delete_text_embeddings(&name, document.id).await?;

                // Remove image BLOBs and image embeddings from all found collections
                for image in images.iter() {
                    vector_db.delete_image_embeddings(&name, image.id).await?;
                    image_store.delete_image(&image.path).await?;
                }

                Ok(())
            }
            .await;

            if let Err(e) = result {
                tracing::error!("Failed to delete document part: {e}");
            }
        }

        // Remove the document only when we are certain it is cleaned up

        self.repo
            .transaction(|tx| {
                Box::pin(async {
                    self.repo
                        .remove_document_by_id(document.id, Some(tx))
                        .await?;
                    store.delete(&document.path).await
                })
            })
            .await?;

        Ok(())
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
            ParseOutputPreview::String(text) => {
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
                    total_tokens_pre: total_tokens,
                    total_tokens_post,
                })
            }
            ParseOutputPreview::Sections(sections) => {
                // TODO: configure section merge strategy
                // currently we are merging them, but we should also enable storing page by page
                let mut total_tokens = TokenCount::default();
                let mut chunks = vec![];

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

                    total_tokens += count;
                }

                Ok(ChunkPreview {
                    chunks,
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

        let ext = document.ext.as_str().try_into()?;

        let output = map_err!(
            tokio::task::spawn_blocking(move || { parse_text(config, ext, &content) }).await
        )?;

        if output.is_empty() {
            return err!(InvalidFile, "Parsing resulted in empty output");
        }

        match output {
            ParseOutput::String(text) => Ok(ParsePreview {
                total_tokens: self.tokenizer.count(&text),
                content: dto::ParseOutputPreview::String(text),
            }),
            ParseOutput::Sections(document_sections) => {
                let mut total_tokens = TokenCount::default();
                let mut sections = vec![];

                for section in document_sections {
                    let mut pages = vec![];

                    for page in section.pages {
                        total_tokens += self.tokenizer.count(&page.content);
                        pages.push(ParsedDocumentPage {
                            content: page.content,
                            number: page.number,
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
    pub async fn update_parser(
        &self,
        id: Uuid,
        collection_id: Option<Uuid>,
        config: ParseConfig,
    ) -> Result<(), ChonkitError> {
        map_err!(config.validate());

        if self.repo.get_document_by_id(id).await?.is_none() {
            return err!(DoesNotExist, "Document with ID {id}");
        }

        self.repo
            .upsert_document_parse_config(id, collection_id, config)
            .await?;

        Ok(())
    }

    /// Update a document's chunking configuration.
    ///
    /// * `id`: Document ID.
    /// * `config`: Chunking configuration.
    pub async fn update_chunker(
        &self,
        id: Uuid,
        collection_id: Option<Uuid>,
        config: ChunkConfig,
    ) -> Result<(), ChonkitError> {
        if self.repo.get_document_by_id(id).await?.is_none() {
            return err!(DoesNotExist, "Document with ID {id}");
        }

        self.repo
            .upsert_document_chunk_config(id, collection_id, config)
            .await?;

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
            .upload(DocumentUpload::new(
                String::from(DEFAULT_DOCUMENT_NAME),
                DocumentType::Text(TextDocumentType::Txt),
                DEFAULT_DOCUMENT_CONTENT.as_bytes(),
            ))
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

    /// Get a document with its content bytes.
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

    // IMAGES

    pub async fn upload_images(
        &self,
        images: Vec<ImageData>,
    ) -> Result<Vec<ImageModel>, ChonkitError> {
        let images = images
            .into_iter()
            .map(|image| {
                Image::new(
                    None,
                    None,
                    image.bytes,
                    image.format,
                    image.width,
                    image.height,
                )
            })
            .collect();

        store_images(
            self.repo.clone(),
            self.providers.image.clone(),
            None,
            images,
        )
        .await
    }

    /// List image metadata from the repository.
    pub async fn list_images(
        &self,
        parameters: ListImagesParameters,
    ) -> Result<List<ImageModel>, ChonkitError> {
        self.repo
            .list_document_images(self.providers.image.id(), parameters)
            .await
    }

    /// Update an image's description which is used to enrich its embeddings.
    pub async fn update_image_description(
        &self,
        image_id: Uuid,
        description: Option<&str>,
    ) -> Result<(), ChonkitError> {
        self.repo
            .update_image_description(image_id, description)
            .await
    }

    /// Delete an
    pub async fn delete_image(&self, image_id: Uuid) -> Result<(), ChonkitError> {
        let collections = self.repo.get_image_assigned_collections(image_id).await?;

        for (_, name, provider) in collections {
            let vector_db = self.providers.vector.get_provider(&provider)?;
            vector_db.delete_image_embeddings(&name, image_id).await?;
        }

        self.repo.delete_image_by_id(image_id).await
    }

    pub async fn get_image(&self, id: Uuid) -> Result<(Image, ImageModel), ChonkitError> {
        get_image(self.repo.clone(), &*self.providers.image, id).await
    }
}

/// Document service DTOs.
pub mod dto {
    use crate::core::{
        chunk::ChunkConfig,
        document::{parser::ParseConfig, DocumentType},
        model::Pagination,
        token::TokenCount,
    };
    use serde::{Deserialize, Serialize};
    use uuid::Uuid;
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
        String(String),
        Sections(Vec<ParsedDocumentSection>),
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
    }

    #[derive(Debug, Deserialize, utoipa::ToSchema)]
    #[serde(rename_all = "camelCase")]
    pub struct ListImagesParameters {
        /// Limit and offset
        #[serde(flatten)]
        pub pagination: Option<Pagination>,
        pub document_id: Option<Uuid>,
    }
}
