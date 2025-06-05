use crate::config::DEFAULT_IMAGE_PATCH_SIZE;
use crate::core::cache::embedding::{
    CachedImageEmbeddings, CachedTextEmbeddings, ImageEmbeddingCacheKey, TextEmbeddingCacheKey,
};
use crate::core::cache::{ImageEmbeddingCache, TextEmbeddingCache};
use crate::core::chunk::{ChunkConfig, ChunkedDocument};
use crate::core::document::parser::{parse, ParseMode, ParseOutput};
use crate::core::embeddings::Embeddings;
use crate::core::model::embedding::{
    Embedding, EmbeddingInsert, EmbeddingRemovalReportBuilder, EmbeddingReport,
    EmbeddingReportAddition, EmbeddingReportBuilder, EmbeddingReportRemoval,
};
use crate::core::model::image::Image;
use crate::core::model::{List, Pagination};
use crate::core::provider::ProviderState;
use crate::core::repo::{Atomic, Repository};
use crate::core::vector::CollectionItemInsert;
use crate::error::ChonkitError;
use crate::{err, map_err, transaction};
use chonkit_embedders::EmbeddingModel;
use chrono::Utc;
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
    ) -> Result<Option<Embedding>, ChonkitError> {
        self.repo.get_embeddings(document_id, collection_id).await
    }

    pub async fn list_embeddings(
        &self,
        pagination: Pagination,
        collection_id: Option<Uuid>,
    ) -> Result<List<Embedding>, ChonkitError> {
        map_err!(pagination.validate());
        self.repo.list_embeddings(pagination, collection_id).await
    }

    pub async fn list_outdated_embeddings(
        &self,
        collection_id: Uuid,
    ) -> Result<Vec<Embedding>, ChonkitError> {
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

    /// Create and store embeddings in both the vector database
    /// and the repository.
    ///
    /// Errors if embeddings already exist in the collection
    /// for the document to prevent duplication in semantic search.
    ///
    /// * `id`: Document ID.
    /// * `vector_db`: The vector DB implementation to use.
    /// * `embedder`: The embedder to use.
    pub async fn create_embeddings(
        &self,
        input: EmbedSingleInput,
    ) -> Result<EmbeddingReportAddition, ChonkitError> {
        // Make sure the collection and document exist.

        let Some(document) = self.repo.get_document_config_by_id(input.document).await? else {
            return err!(DoesNotExist, "Document with ID {}", input.document);
        };

        let Some(collection) = self.repo.get_collection_by_id(input.collection).await? else {
            return err!(DoesNotExist, "Collection with ID '{}'", input.collection);
        };

        // Start the report so we get a sense of how long all of this takes

        let report = EmbeddingReportBuilder::new(
            document.id,
            document.name.clone(),
            collection.id,
            collection.name.clone(),
        );

        // Make sure we are not duplicating embeddings.

        let existing = self.repo.get_embeddings(document.id, collection.id).await?;
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

        tracing::debug!("{} - starting embedding process", document.name);

        // Load parser and chunker

        let parse_cfg = document.parse_config.unwrap_or_default();

        let chunk_cfg = match parse_cfg.mode {
            ParseMode::String(_) => {
                tracing::debug!("{} - using generic parser", document.name);
                document
                    .chunk_config
                    .or(Some(ChunkConfig::snapping_default()))
            }
            // Sectioned parsers do not support chunking
            ParseMode::Section(_) => {
                tracing::debug!("{} - using section parser", document.name);
                None
            }
        };

        // Check embedding cache

        let text_cache_key = TextEmbeddingCacheKey::new(
            &collection.model,
            &document.hash,
            chunk_cfg.as_ref(),
            &parse_cfg.mode,
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

            // If there are cached text embeddings, we need to check for cached image embeddings
            // as well.
            let existing_images = self
                .repo
                .list_all_document_images(document.id, self.providers.image.id())
                .await?;

            let mut cached_img_embeddings = Vec::with_capacity(existing_images.len());
            let mut total_image_embeddings = 0;

            for image in existing_images {
                let image = self.providers.image.get_image(&image.path).await?;

                let img_cache_key =
                    &ImageEmbeddingCacheKey::new(image.hash(), collection.model.clone());

                let cached = match self.image_cache.get(img_cache_key).await {
                    Ok(embeddings) => embeddings,
                    Err(e) => {
                        tracing::debug!(
                            "{} - failed to get image embeddings from cache: {e}",
                            document.name
                        );
                        continue;
                    }
                };

                if let Some(cached) = cached {
                    cached_img_embeddings.push(cached);
                } else {
                    if image.image.estimate_tokens(DEFAULT_IMAGE_PATCH_SIZE)
                        >= model_details.max_input_tokens as u32
                    {
                        tracing::warn!(
                            "Skipping image due to too many tokens ({} > {})",
                            image.image.estimate_tokens(DEFAULT_IMAGE_PATCH_SIZE),
                            model_details.max_input_tokens
                        );
                        continue;
                    }

                    tracing::debug!(
                        "{} - cache miss (key: {}) for image embeddings, attempting re-embedding",
                        document.name,
                        img_cache_key.key()
                    );

                    let b64 = image.image.to_b64_data_uri();

                    let mut embeddings = embedder
                        .embed_image(None, None, &b64, &model_details.name)
                        .await?;

                    let tokens_used = embeddings.tokens_used;
                    let embeddings = std::mem::take(&mut embeddings.embeddings[0]);
                    let key = ImageEmbeddingCacheKey::new(image.hash(), collection.model.clone());

                    let embeddings = CachedImageEmbeddings::new(
                        embeddings,
                        tokens_used,
                        b64,
                        image.path(),
                        image.description,
                    );

                    self.image_cache.set(&key, embeddings.clone()).await?;

                    cached_img_embeddings.push(embeddings);
                }

                total_image_embeddings += 1;
            }

            debug_assert_eq!(total_image_embeddings, cached_img_embeddings.len());

            return transaction!(self.repo, |tx| async move {
                self.repo
                    .insert_embeddings(EmbeddingInsert::new(document.id, collection.id), Some(tx))
                    .await?;

                let report = report
                    .model_used(collection.model)
                    .embedding_provider(collection.embedder.clone())
                    .tokens_used(Some(0))
                    .image_vectors(total_image_embeddings)
                    .total_vectors(embeddings.embeddings.len() + total_image_embeddings)
                    .vector_db(collection.provider)
                    .from_cache()
                    .finished_at(Utc::now())
                    .build();

                self.store_embedding_report(&report).await?;

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

                for image in cached_img_embeddings {
                    vector_db
                        .insert_embeddings(CollectionItemInsert::new_image(
                            Some(document.id),
                            &collection.name,
                            &image.image_b64,
                            &image.image_path,
                            image.description.as_deref(),
                            image.embeddings,
                        ))
                        .await?;
                }

                Ok(report)
            });
        }

        // From this point on we are certain nothing is cached

        // Read and parse
        let content_bytes = storage.read(&document.path).await?;

        let existing_image_paths = self
            .repo
            .list_document_image_paths(document.id, self.providers.image.id())
            .await?;

        let parse_output = parse(
            parse_cfg,
            document.ext.try_into()?,
            &content_bytes,
            &existing_image_paths
                .iter()
                .filter_map(|p| Some(p.split_once('.')?.0.to_string()))
                .collect::<Vec<_>>(),
        )?;

        // Chunk and embed
        let chunks: Vec<String>;
        let embeddings: Embeddings;
        let mut image_embeddings: Vec<(Image, Embeddings)> = vec![];
        let mut total_image_embeddings = 0;

        match parse_output {
            ParseOutput::String { text, images } => {
                tracing::debug!("{} - parsed to string", document.name);
                image_embeddings.reserve(images.len() + existing_image_paths.len());

                match chunk_cfg {
                    Some(cfg) => {
                        chunks =
                            match crate::core::chunk::chunk(&self.providers, cfg, &text).await? {
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
                }

                if model_details.multimodal {
                    for path in existing_image_paths {
                        let image = self.providers.image.get_image(&path).await?;

                        if dbg!(image.image.estimate_tokens(DEFAULT_IMAGE_PATCH_SIZE))
                            >= model_details.max_input_tokens as u32
                        {
                            tracing::warn!(
                                "Skipping image due to too many tokens ({} > {})",
                                image.image.estimate_tokens(DEFAULT_IMAGE_PATCH_SIZE),
                                model_details.max_input_tokens
                            );
                            continue;
                        }

                        let embeddings = embedder
                            .embed_image(
                                None,
                                image.description.as_deref(),
                                &image.image.to_b64_data_uri(),
                                &model_details.name,
                            )
                            .await?;
                        image_embeddings.push((image, embeddings));
                        total_image_embeddings += 1;
                    }

                    for image in images {
                        // TODO: System message and context
                        if image.image.estimate_tokens(DEFAULT_IMAGE_PATCH_SIZE)
                            >= model_details.max_input_tokens as u32
                        {
                            tracing::warn!(
                                "Skipping image due to too many tokens ({} > {})",
                                image.image.estimate_tokens(DEFAULT_IMAGE_PATCH_SIZE),
                                model_details.max_input_tokens
                            );
                            continue;
                        }

                        let embeddings = embedder
                            .embed_image(
                                None,
                                image.description.as_deref(),
                                &image.image.to_b64_data_uri(),
                                &model_details.name,
                            )
                            .await?;
                        image_embeddings.push((image, embeddings));
                        total_image_embeddings += 1;
                    }
                }
            }
            ParseOutput::Sections(sections) => {
                tracing::debug!("{} - parsed sections", document.name);

                let mut section_chunks = Vec::with_capacity(sections.len());

                for path in existing_image_paths {
                    let image = self.providers.image.get_image(&path).await?;
                    if image.image.estimate_tokens(DEFAULT_IMAGE_PATCH_SIZE)
                        >= model_details.max_input_tokens as u32
                    {
                        tracing::warn!(
                            "Skipping image due to too many tokens ({} > {})",
                            image.image.estimate_tokens(DEFAULT_IMAGE_PATCH_SIZE),
                            model_details.max_input_tokens
                        );
                        continue;
                    }
                    let embeddings = embedder
                        .embed_image(
                            None,
                            image.description.as_deref(),
                            &image.image.to_b64_data_uri(),
                            &model_details.name,
                        )
                        .await?;
                    image_embeddings.push((image, embeddings));
                    total_image_embeddings += 1;
                }

                let sections_total = sections.len();
                for (i, section) in sections.into_iter().enumerate() {
                    let mut content = String::new();

                    let pages_total = section.pages.len();
                    for (j, page) in section.pages.into_iter().enumerate() {
                        content.push_str(&page.content);
                        content.push('\n');

                        if !model_details.multimodal {
                            continue;
                        }

                        let images_total = page.images.len();
                        for (k, image) in page.images.into_iter().enumerate() {
                            if image.image.estimate_tokens(DEFAULT_IMAGE_PATCH_SIZE)
                                >= model_details.max_input_tokens as u32
                            {
                                tracing::warn!(
                                    "Skipping image due to too many tokens ({} > {})",
                                    image.image.estimate_tokens(DEFAULT_IMAGE_PATCH_SIZE),
                                    model_details.max_input_tokens
                                );
                                continue;
                            }
                            let embeddings = embedder
                                .embed_image(
                                    None,
                                    image.description.as_deref(),
                                    &image.image.to_b64_data_uri(),
                                    &model_details.name,
                                )
                                .await?;
                            tracing::debug!("Image embedded (section {}/{sections_total} | page {}/{pages_total} | image {}/{images_total})", i+1, j+1, k+1);
                            image_embeddings.push((image, embeddings));
                            total_image_embeddings += 1;
                        }
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

        let mut total_tokens = embeddings.tokens_used.unwrap_or(0);
        for (_, embeddings) in image_embeddings.iter() {
            if let Some(tokens) = embeddings.tokens_used {
                total_tokens += tokens;
            }
        }

        debug_assert_eq!(chunks.len(), embeddings.embeddings.len());

        transaction!(self.repo, |tx| async move {
            // Repository operations go first since we can revert those with the tx

            self.repo
                .insert_embeddings(EmbeddingInsert::new(document.id, collection.id), Some(tx))
                .await?;

            let report = report
                .model_used(collection.model)
                .embedding_provider(collection.embedder.clone())
                .tokens_used(if total_tokens == 0 {
                    None
                } else {
                    Some(total_tokens)
                })
                .image_vectors(total_image_embeddings)
                .total_vectors(embeddings.embeddings.len() + total_image_embeddings)
                .vector_db(collection.provider)
                .finished_at(Utc::now())
                .build();

            self.store_embedding_report(&report).await?;

            tracing::debug!("{} - caching embeddings", document.name);

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
                tracing::warn!("{} - failed to cache embeddings: {}", document.name, e);
            }

            tracing::debug!("{} - inserting text embeddings", document.name);
            vector_db
                .insert_embeddings(CollectionItemInsert::new_text(
                    document.id,
                    &collection.name,
                    &chunks.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
                    embeddings.embeddings,
                ))
                .await?;

            tracing::debug!("{} - inserting image embeddings", document.name);
            for (img, mut embeddings) in image_embeddings {
                let embeddings = std::mem::take(&mut embeddings.embeddings[0]);
                vector_db
                    .insert_embeddings(CollectionItemInsert::new_image(
                        Some(document.id),
                        &collection.name,
                        &img.image.to_b64_data_uri(),
                        &img.path(),
                        img.description.as_deref(),
                        embeddings,
                    ))
                    .await?
            }

            tracing::debug!("{} - successfully processed", document.name);

            Ok(report)
        })
    }

    /// Returns the number of rows deleted from the db and the number of vectors deleted from the collection.
    pub async fn delete_embeddings(
        &self,
        collection_id: Uuid,
        document_id: Uuid,
    ) -> Result<EmbeddingReportRemoval, ChonkitError> {
        let Some(document) = self.repo.get_document_by_id(document_id).await? else {
            return err!(DoesNotExist, "Document with ID {document_id}");
        };

        let Some(collection) = self.repo.get_collection_by_id(collection_id).await? else {
            return err!(DoesNotExist, "Collection with ID '{collection_id}'");
        };

        if self
            .repo
            .get_embeddings(document.id, collection.id)
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

        let report = EmbeddingRemovalReportBuilder::new(
            document.id,
            document.name,
            collection.id,
            collection.name.clone(),
        );

        let vector_db = self.providers.vector.get_provider(&collection.provider)?;

        transaction!(self.repo, |tx| async move {
            let amount_deleted_db = self
                .repo
                .delete_embeddings(document_id, collection_id, Some(tx))
                .await?;

            let amount = vector_db
                .count_vectors(&collection.name, document_id)
                .await?;

            let report = report
                .total_vectors_removed(amount)
                .finished_at(Utc::now())
                .build();

            self.store_embedding_removal_report(&report).await?;

            vector_db
                .delete_embeddings(&collection.name, document_id)
                .await?;

            tracing::debug!(
                "Deleted {amount} vectors in collection '{}' ({amount_deleted_db} from db)",
                collection.name
            );

            Ok(report)
        })
    }

    pub async fn delete_all_embeddings(&self, document_id: Uuid) -> Result<usize, ChonkitError> {
        let collections = self
            .repo
            .get_document_assigned_collections(document_id)
            .await?;

        let mut total_deleted = 0;

        for (collection_id, collection_name, provider) in collections.iter() {
            let vector_db = self.providers.vector.get_provider(provider)?;

            let amount = transaction!(self.repo, |tx| async move {
                self.repo
                    .delete_embeddings(document_id, *collection_id, Some(tx))
                    .await?;
                let amount = vector_db
                    .count_vectors(collection_name, document_id)
                    .await?;
                vector_db
                    .delete_embeddings(collection_name, document_id)
                    .await?;

                Ok(amount)
            })?;

            total_deleted += amount;
        }

        tracing::debug!(
            "Deleted {total_deleted} embeddings from {} collections",
            collections.len()
        );

        Ok(total_deleted)
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

    async fn store_embedding_report(
        &self,
        report: &EmbeddingReportAddition,
    ) -> Result<(), ChonkitError> {
        tracing::debug!(
            "Storing embedding report for document '{}' in '{}'",
            report.document_name,
            report.collection_name
        );
        self.repo.insert_embedding_report(report).await?;
        Ok(())
    }

    async fn store_embedding_removal_report(
        &self,
        report: &EmbeddingReportRemoval,
    ) -> Result<(), ChonkitError> {
        tracing::debug!(
            "Storing embedding removal report for document '{}' in '{}'",
            report.document_name,
            report.collection_name
        );
        self.repo.insert_embedding_removal_report(report).await?;
        Ok(())
    }
}

/// Used for embedding documents one by one.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[cfg_attr(test, derive(Clone))]
#[serde(rename_all = "camelCase")]
pub struct EmbedSingleInput {
    /// The ID of the document to embed.
    pub document: Uuid,

    /// The ID of the collection in which to store the embeddings to.
    pub collection: Uuid,
}

impl EmbedSingleInput {
    pub fn new(document: Uuid, collection: Uuid) -> Self {
        Self {
            document,
            collection,
        }
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
