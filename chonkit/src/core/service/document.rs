use crate::{
    config::{DEFAULT_DOCUMENT_CONTENT, DEFAULT_DOCUMENT_NAME, FS_STORE_ID},
    core::{
        chunk::{
            ChunkConfig, ChunkedDocument, SemanticEmbedder, SemanticWindowConfig,
            SnappingWindowConfig,
        },
        document::{
            parser::{ParseConfig, Parser},
            sha256, DocumentType, TextDocumentType,
        },
        model::{
            document::{
                Document, DocumentConfig, DocumentDisplay, DocumentInsert, DocumentParameterUpdate,
            },
            List, PaginationSort,
        },
        provider::ProviderState,
        repo::{Atomic, Repository},
        service::token::TokenCount,
    },
    err,
    error::ChonkitError,
    map_err, transaction,
};
use dto::{ChunkForPreview, ChunkPreview, DocumentUpload};
use std::time::Instant;
use uuid::Uuid;
use validify::{Validate, Validify};

use super::token::Tokenizer;

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
        p: PaginationSort,
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
        p: PaginationSort,
        src: Option<&str>,
        document_id: Option<Uuid>,
    ) -> Result<List<DocumentDisplay>, ChonkitError> {
        map_err!(p.validate());
        self.repo
            .list_documents_with_collections(p, src, document_id)
            .await
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
        let file = self.repo.get_document_config_by_id(id).await?;

        let Some(file) = file else {
            return err!(DoesNotExist, "Document with ID {id}");
        };

        Ok(file)
    }

    /// Get document text content using its parsing configuration,
    /// or the default parser if it has no configuration.
    ///
    /// * `id`: Document ID.
    pub async fn get_content(&self, id: Uuid) -> Result<String, ChonkitError> {
        let Some(document) = self.repo.get_document_by_id(id).await? else {
            return err!(DoesNotExist, "Document with ID {id}");
        };

        let store = self.providers.storage.get_provider(&document.src)?;

        let ext = document.ext.as_str().try_into()?;
        let parser = self.get_parsing_config(id).await?;

        let content = store.read(&document.path).await?;

        parser.parse_bytes(ext, &content)
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

        self.chunk(config, content).await
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

        let store = self.providers.storage.get_provider(FS_STORE_ID)?;

        let DocumentUpload { ref name, ty, file } = params;
        let path = store.absolute_path(name, ty);
        let hash = sha256(file);

        // The default parser parses the whole input so we use it
        // to check whether the document has any content. Reject if empty.
        Parser::default().parse_bytes(ty, file)?;

        // Always return errors if there is a hash collision
        if let Some(existing) = self.repo.get_document_by_hash(&hash).await? {
            return err!(
                AlreadyExists,
                "New document '{name}' has same hash as existing '{}' ({})",
                existing.name,
                existing.id
            );
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
            let update = DocumentParameterUpdate::new(&path, &hash);
            return self
                .repo
                .update_document_parameters(existing.id, update)
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
            .get_document_assigned_collection_names(document.id)
            .await?;
        let store = self.providers.storage.get_provider(&document.src)?;
        transaction! {self.repo, |tx| async move {
                self.repo.remove_document_by_id(document.id, Some(tx)).await?;
                for (collection, provider) in collections {
                    let vector_db = self.providers.vector.get_provider(&provider)?;
                    vector_db
                        .delete_embeddings(&collection, document.id)
                        .await?;
                }
                store.delete(&document.path).await
            }
        }
    }

    /// Sync storage contents with the repo.
    pub async fn sync(&self, provider: &str) -> Result<(), ChonkitError> {
        let __start = Instant::now();

        let store = self.providers.storage.get_provider(provider)?;

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

        let parser = if let Some(parser) = config.parser {
            parser
        } else {
            let config = self.get_config(document_id).await?;
            match config.parse_config {
                Some(config) => config,
                None => return err!(DoesNotExist, "Parsing configuration for {document_id}"),
            }
        };

        let content = self.parse_document(document_id, parser).await?;

        let total_tokens_pre = self.tokenizer.count(&content);
        let mut total_tokens_post = TokenCount::default();

        let chunks = match self.chunk(config.chunker, &content).await? {
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
            total_tokens_pre,
            total_tokens_post,
        })
    }

    async fn chunk<'i>(
        &self,
        config: ChunkConfig,
        input: &'i str,
    ) -> Result<ChunkedDocument<'i>, ChonkitError> {
        let chunks = match config {
            ChunkConfig::Sliding(config) => {
                let chunker = map_err!(chunx::SlidingWindow::new(config.size, config.overlap));
                let chunked = map_err!(chunker.chunk(input));

                ChunkedDocument::Ref(chunked)
            }
            ChunkConfig::Snapping(config) => {
                let SnappingWindowConfig {
                    size,
                    overlap,
                    delimiter,
                    skip_f,
                    skip_b,
                } = config;

                let chunker = map_err!(chunx::Snapping::new(
                    size, overlap, delimiter, skip_f, skip_b
                ));

                let chunked = map_err!(chunker.chunk(input));

                ChunkedDocument::Owned(chunked)
            }
            ChunkConfig::Semantic(config) => {
                let SemanticWindowConfig {
                    size,
                    threshold,
                    distance_fn,
                    delimiter,
                    embedding_provider,
                    embedding_model,
                    skip_f,
                    skip_b,
                } = config;

                let chunker =
                    chunx::Semantic::new(size, threshold, distance_fn, delimiter, skip_f, skip_b);

                let embedder = self.providers.embedding.get_provider(&embedding_provider)?;

                if embedder.size(&embedding_model).await?.is_none() {
                    return err!(
                        InvalidEmbeddingModel,
                        "Model '{embedding_model}' not supported by '{embedding_provider}'"
                    );
                };

                let semantic_embedder = SemanticEmbedder(embedder.clone());

                let chunked = chunker
                    .chunk(input, &semantic_embedder, &embedding_model)
                    .await?;

                ChunkedDocument::Owned(chunked)
            }
        };

        if chunks.is_empty() {
            return err!(Chunks, "chunks cannot be empty");
        }

        Ok(chunks)
    }

    /// Parse the document and return its content.
    ///
    /// * `id`: Document ID.
    /// * `config`: If given, uses the parsing config, otherwise use the default parser for the
    ///             file type.
    pub async fn parse_document(
        &self,
        id: Uuid,
        config: ParseConfig,
    ) -> Result<String, ChonkitError> {
        map_err!(config.validate());

        let document = self.repo.get_document_by_id(id).await?;

        let Some(document) = document else {
            return err!(DoesNotExist, "Document with ID {id}");
        };

        let store = self.providers.storage.get_provider(&document.src)?;

        let ext: DocumentType = document.ext.as_str().try_into()?;
        let parser = Parser::new(config);

        tracing::info!("Using parser ({ext}) for '{id}'");

        let content = store.read(&document.path).await?;

        parser.parse_bytes(ext, &content)
    }

    /// Update a document's parsing configuration.
    ///
    /// * `id`: Document ID.
    /// * `config`: Parsing configuration.
    pub async fn update_parser(&self, id: Uuid, config: ParseConfig) -> Result<(), ChonkitError> {
        map_err!(config.validate());

        let document = self.repo.get_document_by_id(id).await?;

        if document.is_none() {
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
        let document = self.repo.get_document_by_id(id).await?;

        if document.is_none() {
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

    /// Get a parser for a document, or a default parser if the document has no configuration.
    ///
    /// * `id`: Document ID.
    /// * `ext`: Document extension.
    async fn get_parsing_config(&self, id: Uuid) -> Result<Parser, ChonkitError> {
        let config = self.repo.get_document_parse_config(id).await?;
        match config {
            Some(cfg) => Ok(Parser::new(cfg.config)),
            None => Ok(Parser::default()),
        }
    }
}

/// Document service DTOs.
pub mod dto {
    use crate::core::{
        chunk::ChunkConfig,
        document::{parser::ParseConfig, DocumentType},
        service::token::TokenCount,
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
        pub parser: Option<ParseConfig>,

        /// Chunking configuration.
        pub chunker: ChunkConfig,
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
}
