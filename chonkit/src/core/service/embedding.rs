use crate::config::DEFAULT_IMAGE_PATCH_SIZE;
use crate::core::cache::embedding::{
    CachedImageEmbeddings, CachedTextEmbeddings, ImageEmbeddingCacheKey, TextEmbeddingCacheKey,
};
use crate::core::cache::{ImageEmbeddingCache, TextEmbeddingCache};
use crate::core::chunk::{ChunkConfig, ChunkedDocument};
use crate::core::document::parser::{parse_text, ParseOutput, TextParseConfig};
use crate::core::embeddings::Embeddings;
use crate::core::model::embedding::{
    EmbeddingAdditionReport, EmbeddingReport, EmbeddingReportBase, ImageEmbeddingAdditionReport,
    ImageEmbeddingInsert, ImageEmbeddingRemovalReport, TextEmbedding, TextEmbeddingAdditionReport,
    TextEmbeddingInsert, TextEmbeddingRemovalReport,
};
use crate::core::model::{List, Pagination};
use crate::core::provider::ProviderState;
use crate::core::repo::Repository;
use crate::core::vector::CollectionItemInsert;
use crate::error::ChonkitError;
use crate::{err, map_err};
use chonkit_embedders::EmbeddingModel;
use serde::Deserialize;
use uuid::Uuid;
use validify::Validate;

#[derive(Clone)]
pub struct EmbeddingService {
    repo: Repository,
    providers: ProviderState,
    text_cache: TextEmbeddingCache,
    image_cache: ImageEmbeddingCache,
}

impl EmbeddingService {
    pub fn new(
        repo: Repository,
        providers: ProviderState,
        text_cache: TextEmbeddingCache,
        image_cache: ImageEmbeddingCache,
    ) -> Self {
        Self {
            repo,
            providers,
            text_cache,
            image_cache,
        }
    }

    pub async fn get_embeddings(
        &self,
        document_id: Uuid,
        collection_id: Uuid,
    ) -> Result<Option<TextEmbedding>, ChonkitError> {
        self.repo
            .get_text_embeddings(document_id, collection_id)
            .await
    }

    pub async fn list_embeddings(
        &self,
        pagination: Pagination,
        collection_id: Option<Uuid>,
    ) -> Result<List<TextEmbedding>, ChonkitError> {
        map_err!(pagination.validate());
        self.repo.list_embeddings(pagination, collection_id).await
    }

    pub async fn list_outdated_embeddings(
        &self,
        collection_id: Uuid,
    ) -> Result<Vec<TextEmbedding>, ChonkitError> {
        self.repo.list_outdated_embeddings(collection_id).await
    }

    /// Return a list of models supported by the provided embedder and their respective sizes.
    ///
    /// * `embedder`: The embedder to use.
    pub async fn list_embedding_models(
        &self,
        embedder: &str,
    ) -> Result<Vec<EmbeddingModel>, ChonkitError> {
        let embedder = self.providers.embedding.get_provider(embedder)?;
        embedder.list_embedding_models().await
    }

    /// Add image embeddings using a multi-modal embedding model.
    pub async fn create_image_embeddings(
        &self,
        input: EmbedImageInput,
    ) -> Result<ImageEmbeddingAdditionReport, ChonkitError> {
        let start = chrono::Utc::now();

        // Make sure the collection and optional document exist.

        let EmbedImageInput {
            image: image_id,
            collection: collection_id,
        } = input;

        let Some(image_meta) = self.repo.get_image_by_id(image_id).await? else {
            return err!(DoesNotExist, "Image with ID {}", image_id);
        };

        let Some(collection) = self.repo.get_collection_by_id(collection_id).await? else {
            return err!(DoesNotExist, "Collection with ID '{}'", collection_id);
        };

        if self
            .repo
            .get_image_embeddings(image_id, collection_id)
            .await?
            .is_some()
        {
            return err!(
                AlreadyExists,
                "Image '{}' is already embedded in collection '{}'",
                image_id,
                collection_id
            );
        }

        let vector_db = self.providers.vector.get_provider(&collection.provider)?;

        let embedder = self
            .providers
            .embedding
            .get_provider(&collection.embedder)?;

        let Some(model_details) = embedder.model_details(&collection.model).await? else {
            return err!(
                InvalidEmbeddingModel,
                "Model '{}' is not supported by embedding provider '{}'"
                collection.model,
                collection.embedder
            );
        };

        if !model_details.multimodal {
            return err!(
                InvalidEmbeddingModel,
                "Model '{}' is not multimodal and cannot embed images",
                model_details.name
            );
        }

        let image = self.providers.image.get_image(&image_meta.path).await?;

        // The image description is part of its hash, if it was changed in the meantime
        // the cache will miss and we will get fresh embeddings.
        let hash = image.hash();

        let cached = self
            .image_cache
            .get(&ImageEmbeddingCacheKey::new(&hash, &collection.model))
            .await;

        if let Err(ref e) = cached {
            tracing::debug!("failed to get image embeddings from cache: {e}");
        }

        let mut cache = false;
        let mut tokens_used = None;

        match cached {
            Ok(Some(embeddings)) => {
                vector_db
                    .insert_embeddings(CollectionItemInsert::new_image(
                        image_meta.document_id,
                        &collection.name,
                        image_meta.id,
                        &image.image.to_b64_data_uri(),
                        &image.path(),
                        image.description.as_deref(),
                        embeddings.embeddings,
                    ))
                    .await?;
                cache = true;
            }
            Err(_) | Ok(None) => {
                if image.image.estimate_tokens(DEFAULT_IMAGE_PATCH_SIZE)
                    >= model_details.max_input_tokens as u32
                {
                    tracing::warn!(
                        "Skipping image due to too many tokens ({} > {})",
                        image.image.estimate_tokens(DEFAULT_IMAGE_PATCH_SIZE),
                        model_details.max_input_tokens
                    );

                    return err!(
                        InvalidParameter,
                        "Image has too many tokens ({} > {})",
                        image.image.estimate_tokens(DEFAULT_IMAGE_PATCH_SIZE),
                        model_details.max_input_tokens
                    );
                }

                tracing::debug!(
                    "cache miss (key: {}) for image embeddings, attempting re-embedding",
                    hash,
                );

                let b64 = image.image.to_b64_data_uri();
                let path = image.path();

                let mut embeddings = embedder
                    .embed_image(None, None, &b64, &model_details.name)
                    .await?;

                let vector = std::mem::take(&mut embeddings.embeddings[0]);
                tokens_used = embeddings.tokens_used.map(|t| t as i32);

                let collection_name = &collection.name;
                let collection_model = &collection.model;

                self.repo
                    .transaction(|tx| {
                        Box::pin(async move {
                            self.repo
                                .insert_image_embeddings(
                                    ImageEmbeddingInsert::new(image_id, collection_id),
                                    Some(tx),
                                )
                                .await?;

                            vector_db
                                .insert_embeddings(CollectionItemInsert::new_image(
                                    image_meta.document_id,
                                    collection_name,
                                    image_meta.id,
                                    &b64,
                                    &path,
                                    image.description.as_deref(),
                                    vector.clone(),
                                ))
                                .await?;

                            let key = ImageEmbeddingCacheKey::new(&hash, collection_model);

                            let embeddings =
                                CachedImageEmbeddings::new(vector, embeddings.tokens_used);

                            if let Err(e) = self.image_cache.set(&key, embeddings).await {
                                tracing::warn!("failed to cache image embeddings: {e}");
                            };

                            Ok(())
                        })
                    })
                    .await?;
            }
        };

        let report = ImageEmbeddingAdditionReport {
            image_id,
            report: EmbeddingAdditionReport {
                model_used: collection.model,
                tokens_used,
                embedding_provider: collection.embedder,
                total_vectors: 1,
                cache,
                base: EmbeddingReportBase {
                    collection_id: Some(collection.id),
                    collection_name: collection.name,
                    vector_db: collection.provider,
                    started_at: start,
                    finished_at: chrono::Utc::now(),
                },
            },
        };

        self.repo.insert_image_embedding_report(&report).await?;

        Ok(report)
    }

    /// Create and store embeddings in both the vector database
    /// and the repository.
    ///
    /// Errors if embeddings already exist in the collection
    /// for the document to prevent duplication in semantic search.
    pub async fn create_text_embeddings(
        &self,
        input: EmbedTextInput,
    ) -> Result<TextEmbeddingAdditionReport, ChonkitError> {
        // Make sure the collection and document exist.

        let Some(document) = self.repo.get_document_config_by_id(input.document).await? else {
            return err!(DoesNotExist, "Document with ID {}", input.document);
        };

        let Some(collection) = self.repo.get_collection_by_id(input.collection).await? else {
            return err!(DoesNotExist, "Collection with ID '{}'", input.collection);
        };

        let start = chrono::Utc::now();

        // Make sure we are not duplicating embeddings.

        let existing = self
            .repo
            .get_text_embeddings(document.id, collection.id)
            .await?;
        if existing.is_some() {
            return err!(
                AlreadyExists,
                "Embeddings for document '{}' in collection '{}'",
                document.id,
                collection.name
            );
        }

        // Load providers and check for state treachery

        let storage = self.providers.document.get_provider(&document.src)?;
        let vector_db = self.providers.vector.get_provider(&collection.provider)?;
        let embedder = self
            .providers
            .embedding
            .get_provider(&collection.embedder)?;

        let v_collection = vector_db.get_collection(&collection.name).await?;

        let Some(model_details) = embedder.model_details(&collection.model).await? else {
            return err!(
                InvalidEmbeddingModel,
                "Model '{}' is not supported by embedding provider '{}'"
                collection.model,
                embedder.id()
            );
        };

        if model_details.size != v_collection.size {
            return err!(
                InvalidEmbeddingModel,
                "Model size ({}) not compatible with collection size ({})"
                model_details.size,
                v_collection.size
            );
        }

        // Load parser and chunker

        let parse_cfg = document.parse_config.unwrap_or_default();

        let chunk_cfg = match parse_cfg {
            TextParseConfig::String(_) => document
                .chunk_config
                .or(Some(ChunkConfig::snapping_default())),
            // Sectioned parsers do not support chunking
            TextParseConfig::Section(_) => None,
        };

        tracing::debug!(
            "{} - embedding with parser-chunker: {}-{}",
            document.name,
            parse_cfg,
            chunk_cfg
                .as_ref()
                .map(|c| c.to_string())
                .unwrap_or("none".to_string())
        );

        // Check embedding cache

        let text_cache_key = TextEmbeddingCacheKey::new(
            &collection.model,
            &document.hash,
            chunk_cfg.as_ref(),
            &parse_cfg,
        )?;

        let cached = match self.text_cache.get(&text_cache_key).await {
            Ok(embeddings) => embeddings,
            Err(e) => {
                tracing::debug!(
                    "{} -  failed to get embeddings from cache: {e}",
                    document.name
                );
                None
            }
        };

        if let Some(embeddings) = cached {
            tracing::debug!("{} - using cached embeddings", document.id);
            return self
                .repo
                .transaction(|tx| {
                    Box::pin(async move {
                        self.repo
                            .insert_text_embeddings(
                                TextEmbeddingInsert::new(document.id, collection.id),
                                Some(tx),
                            )
                            .await?;

                        let report = TextEmbeddingAdditionReport {
                            document_id: document.id,
                            document_name: document.name,
                            report: EmbeddingAdditionReport {
                                model_used: collection.model,
                                tokens_used: Some(0),
                                embedding_provider: collection.embedder.clone(),
                                total_vectors: embeddings.embeddings.len() as i32,
                                cache: true,
                                base: EmbeddingReportBase {
                                    collection_id: Some(collection.id),
                                    collection_name: collection.name.clone(),
                                    vector_db: collection.provider.clone(),
                                    started_at: start,
                                    finished_at: chrono::Utc::now(),
                                },
                            },
                        };

                        self.repo.insert_text_embedding_report(&report).await?;

                        vector_db
                            .insert_embeddings(CollectionItemInsert::new_text(
                                document.id,
                                &collection.name,
                                &embeddings
                                    .chunks
                                    .iter()
                                    .map(|s| s.as_str())
                                    .collect::<Vec<&str>>(),
                                embeddings.embeddings,
                            ))
                            .await?;

                        Ok(report)
                    })
                })
                .await;
        }

        // From this point on we are certain nothing is cached

        // Read and parse
        let content_bytes = storage.read(&document.path).await?;

        let parse_output = parse_text(parse_cfg, document.ext.try_into()?, &content_bytes)?;

        // Chunk and embed
        let chunks: Vec<String>;
        let embeddings: Embeddings;

        match parse_output {
            ParseOutput::String(text) => match chunk_cfg {
                Some(cfg) => {
                    chunks = match crate::core::chunk::chunk(&self.providers, cfg, &text).await? {
                        ChunkedDocument::Ref(r) => {
                            r.iter().map(|s| s.to_string()).collect::<Vec<_>>()
                        }
                        ChunkedDocument::Owned(o) => o,
                    };

                    embeddings = embedder
                        .embed_text(
                            &chunks.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
                            &collection.model,
                        )
                        .await?;
                }
                None => {
                    embeddings = embedder.embed_text(&[&text], &collection.model).await?;
                    chunks = vec![text];
                }
            },
            ParseOutput::Sections(sections) => {
                let mut section_chunks = Vec::with_capacity(sections.len());

                for section in sections {
                    let mut content = String::new();

                    for page in section.pages {
                        content.push_str(&page.content);
                        content.push('\n');
                    }

                    section_chunks.push(content);
                }

                embeddings = embedder
                    .embed_text(
                        &section_chunks
                            .iter()
                            .map(|s| s.as_str())
                            .collect::<Vec<_>>(),
                        &collection.model,
                    )
                    .await?;
                // In case of sectioned parsers, we define the sections as chunks
                chunks = section_chunks
            }
        };

        // Chunking and embedding is done, store everything

        tracing::debug!(
            "{} - generating embeddings ({} total chunks)",
            document.name,
            chunks.len()
        );

        debug_assert_eq!(chunks.len(), embeddings.embeddings.len());

        self.repo
            .transaction(|tx| {
                Box::pin(async move {
                    // Repository operations go first since we can revert those with the tx

                    self.repo
                        .insert_text_embeddings(
                            TextEmbeddingInsert::new(document.id, collection.id),
                            Some(tx),
                        )
                        .await?;

                    let report = TextEmbeddingAdditionReport {
                        document_id: document.id,
                        document_name: document.name,
                        report: EmbeddingAdditionReport {
                            model_used: collection.model,
                            tokens_used: embeddings.tokens_used.map(|t| t as i32),
                            embedding_provider: collection.embedder.clone(),
                            total_vectors: embeddings.embeddings.len() as i32,
                            cache: false,
                            base: EmbeddingReportBase {
                                collection_id: Some(collection.id),
                                collection_name: collection.name.clone(),
                                vector_db: collection.provider.clone(),
                                started_at: start,
                                finished_at: chrono::Utc::now(),
                            },
                        },
                    };

                    self.repo.insert_text_embedding_report(&report).await?;

                    if let Err(e) = self
                        .text_cache
                        .set(
                            &text_cache_key,
                            CachedTextEmbeddings::new(
                                embeddings.embeddings.clone(),
                                embeddings.tokens_used,
                                chunks.iter().map(|s| s.to_string()).collect(),
                            ),
                        )
                        .await
                    {
                        tracing::warn!("failed to cache embeddings: {e}");
                    }

                    vector_db
                        .insert_embeddings(CollectionItemInsert::new_text(
                            document.id,
                            &collection.name,
                            &chunks.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
                            embeddings.embeddings,
                        ))
                        .await?;

                    Ok(report)
                })
            })
            .await
    }

    /// Returns the number of rows deleted from the db and the number of vectors deleted from the collection.
    pub async fn delete_text_embeddings(
        &self,
        collection_id: Uuid,
        document_id: Uuid,
    ) -> Result<TextEmbeddingRemovalReport, ChonkitError> {
        let Some(document) = self.repo.get_document_by_id(document_id).await? else {
            return err!(DoesNotExist, "Document with ID {document_id}");
        };

        let Some(collection) = self.repo.get_collection_by_id(collection_id).await? else {
            return err!(DoesNotExist, "Collection with ID '{collection_id}'");
        };

        if self
            .repo
            .get_text_embeddings(document.id, collection.id)
            .await?
            .is_none()
        {
            return err!(
                DoesNotExist,
                "Embeddings for document '{}' in collection '{}'",
                document.id,
                collection.name
            );
        };

        let start = chrono::Utc::now();

        let vector_db = self.providers.vector.get_provider(&collection.provider)?;

        self.repo
            .transaction(|tx| {
                Box::pin(async move {
                    let amount_deleted_db = self
                        .repo
                        .delete_text_embeddings(document_id, collection_id, Some(tx))
                        .await?;

                    let amount = vector_db
                        .count_vectors(&collection.name, document_id)
                        .await?;

                    let report = TextEmbeddingRemovalReport {
                        document_id,
                        document_name: document.name,
                        report: EmbeddingReportBase {
                            collection_id: Some(collection.id),
                            collection_name: collection.name.clone(),
                            vector_db: collection.provider.clone(),
                            started_at: start,
                            finished_at: chrono::Utc::now(),
                        },
                    };

                    self.repo
                        .insert_text_embedding_removal_report(&report)
                        .await?;

                    vector_db
                        .delete_text_embeddings(&collection.name, document_id)
                        .await?;

                    tracing::debug!(
                        "Deleted {amount} vectors in collection '{}' ({amount_deleted_db} from db)",
                        collection.name
                    );

                    Ok(report)
                })
            })
            .await
    }

    pub async fn delete_image_embeddings(
        &self,
        collection_id: Uuid,
        image_id: Uuid,
    ) -> Result<ImageEmbeddingRemovalReport, ChonkitError> {
        let Some(image) = self.repo.get_image_by_id(image_id).await? else {
            return err!(DoesNotExist, "Image with ID {image_id}");
        };

        let Some(collection) = self.repo.get_collection_by_id(collection_id).await? else {
            return err!(DoesNotExist, "Collection with ID '{collection_id}'");
        };

        if self
            .repo
            .get_image_embeddings(image.id, collection.id)
            .await?
            .is_none()
        {
            return err!(
                DoesNotExist,
                "Embeddings for image '{}' in collection '{}'",
                image.id,
                collection.name
            );
        };

        let start = chrono::Utc::now();

        let vector_db = self.providers.vector.get_provider(&collection.provider)?;

        self.repo
            .transaction(|tx| {
                Box::pin(async move {
                    let amount_deleted_db = self
                        .repo
                        .delete_text_embeddings(image_id, collection_id, Some(tx))
                        .await?;

                    let report = ImageEmbeddingRemovalReport {
                        image_id,
                        report: EmbeddingReportBase {
                            collection_id: Some(collection.id),
                            collection_name: collection.name.clone(),
                            vector_db: collection.provider.clone(),
                            started_at: start,
                            finished_at: chrono::Utc::now(),
                        },
                    };

                    self.repo
                        .insert_image_embedding_removal_report(&report)
                        .await?;

                    vector_db
                        .delete_image_embeddings(&collection.name, image_id)
                        .await?;

                    tracing::debug!(
                        "Deleted image vectors in collection '{}' ({amount_deleted_db} from db)",
                        collection.name
                    );

                    Ok(report)
                })
            })
            .await
    }

    /// Delete all text and image embeddings from the database and vector database.
    pub async fn delete_all_embeddings(&self, document_id: Uuid) -> Result<(), ChonkitError> {
        let collections = self
            .repo
            .get_document_assigned_collections(document_id)
            .await?;

        let images = self
            .repo
            .list_all_document_images(document_id, self.providers.image.id())
            .await?;

        for (collection_id, collection_name, provider) in collections {
            let images = &images[..];
            self.repo
                .transaction(|tx| {
                    Box::pin(async move {
                        let vector_db = self.providers.vector.get_provider(&provider)?;

                        self.repo
                            .delete_text_embeddings(document_id, collection_id, Some(tx))
                            .await?;

                        for image in images {
                            self.repo
                                .delete_image_embeddings(image.id, collection_id, Some(tx))
                                .await?;
                        }

                        vector_db
                            .count_vectors(&collection_name, document_id)
                            .await?;

                        vector_db
                            .delete_text_embeddings(&collection_name, document_id)
                            .await?;

                        tracing::debug!("Deleted embeddings from collection '{collection_name}'",);
                        Ok(())
                    })
                })
                .await?;
        }

        Ok(())
    }

    pub async fn count_embeddings(
        &self,
        collection_id: Uuid,
        document_id: Uuid,
    ) -> Result<usize, ChonkitError> {
        let Some(collection) = self.repo.get_collection_by_id(collection_id).await? else {
            return err!(DoesNotExist, "Collection with ID '{collection_id}'");
        };
        let vector_db = self.providers.vector.get_provider(&collection.provider)?;
        vector_db.count_vectors(&collection.name, document_id).await
    }

    pub async fn list_collection_embedding_reports(
        &self,
        params: ListEmbeddingReportsParams,
    ) -> Result<Vec<EmbeddingReport>, ChonkitError> {
        map_err!(params.validate());
        self.repo.list_collection_embedding_reports(params).await
    }
}

/// Used for embedding text from documents, one document at a time.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[cfg_attr(test, derive(Clone))]
#[serde(rename_all = "camelCase")]
pub struct EmbedTextInput {
    /// The ID of the document to embed.
    pub document: Uuid,

    /// The ID of the collection in which to store the embeddings to.
    pub collection: Uuid,
}

impl EmbedTextInput {
    pub fn new(document: Uuid, collection: Uuid) -> Self {
        Self {
            document,
            collection,
        }
    }
}

/// Used for embedding single images.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[cfg_attr(test, derive(Clone))]
#[serde(rename_all = "camelCase")]
pub struct EmbedImageInput {
    /// The ID of the document to embed.
    pub image: Uuid,

    /// The ID of the collection in which to store the embeddings to.
    pub collection: Uuid,
}

impl EmbedImageInput {
    pub fn new(image: Uuid, collection: Uuid) -> Self {
        Self { image, collection }
    }
}

#[derive(Debug, Default, Deserialize, Validate, utoipa::IntoParams, utoipa::ToSchema)]
pub struct ListEmbeddingReportsParams {
    pub collection: Option<Uuid>,
    pub document: Option<Uuid>,
    #[validate]
    #[serde(flatten)]
    #[param(inline)]
    pub options: Option<Pagination>,
}
