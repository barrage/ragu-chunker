use crate::core::cache::{CachedEmbeddings, EmbeddingCache, EmbeddingCacheKey};
use crate::core::chunk::{ChunkConfig, ChunkedDocument};
use crate::core::document::parser::{GenericParseConfig, ParseConfig, Parser};
use crate::core::model::embedding::{
    Embedding, EmbeddingInsert, EmbeddingRemovalReportBuilder, EmbeddingReport,
    EmbeddingReportAddition, EmbeddingReportBuilder, EmbeddingReportRemoval,
};
use crate::core::model::{List, Pagination};
use crate::core::provider::ProviderState;
use crate::core::repo::{Atomic, Repository};
use crate::error::ChonkitError;
use crate::{err, map_err, transaction};
use chrono::Utc;
use serde::Deserialize;
use uuid::Uuid;
use validify::Validate;

#[derive(Clone)]
pub struct EmbeddingService<Cache> {
    repo: Repository,
    providers: ProviderState,
    cache: Cache,
}

impl<Cache> EmbeddingService<Cache>
where
    Cache: EmbeddingCache,
{
    pub fn new(repo: Repository, providers: ProviderState, cache: Cache) -> Self {
        Self {
            repo,
            providers,
            cache,
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
    ) -> Result<Vec<(String, usize)>, ChonkitError> {
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

        let storage = self.providers.storage.get_provider(&document.src)?;
        let vector_db = self.providers.vector.get_provider(&collection.provider)?;
        let embedder = self
            .providers
            .embedding
            .get_provider(&collection.embedder)?;

        let v_collection = vector_db.get_collection(&collection.name).await?;
        let Some(size) = embedder.size(&collection.model).await? else {
            let (model, embedder) = (collection.model, embedder.id());
            return err!(
                InvalidEmbeddingModel,
                "Model '{model}' not supported for embedder {embedder}"
            );
        };

        if size != v_collection.size {
            let v_size = v_collection.size;
            return err!(
                InvalidEmbeddingModel,
                "Model size ({size}) not compatible with collection ({v_size})"
            );
        }

        tracing::info!("{} - starting embedding process", document.name);

        // Load parser and chunker

        let parse_cfg = match document.parse_config {
            Some(cfg) => cfg,
            // The default parser processes the whole document in cases it was not explicitly
            // set by users
            None => ParseConfig::Generic(GenericParseConfig::default()),
        };

        let chunk_cfg = match parse_cfg {
            ParseConfig::Generic(_) => {
                tracing::info!("{} - using generic parser", document.name);
                document
                    .chunk_config
                    .or(Some(ChunkConfig::snapping_default()))
            }
            // Sectioned parsers do not support chunking
            ParseConfig::Sectioned(_) => {
                tracing::info!("{} - using section parser", document.name);
                None
            }
        };

        // Check embedding cache

        let cache_key = EmbeddingCacheKey::new(&document.hash, chunk_cfg.as_ref(), &parse_cfg)?;

        // If the cache errors, we want to gracefully fail and continue as usual

        let cached = match self.cache.get(&cache_key).await {
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
            #[cfg(debug_assertions)]
            'debug_assertions: {
                // Assert we get the same chunks from the cache as we would when chunking regularly.
                let Some(chunk_cfg) = chunk_cfg.clone() else {
                    break 'debug_assertions;
                };

                let ParseConfig::Generic(parse_cfg) = parse_cfg.clone() else {
                    break 'debug_assertions;
                };

                let content_bytes = storage.read(&document.path).await?;

                let content = Parser::new(parse_cfg.to_owned())
                    .parse(document.ext.try_into()?, &content_bytes)?;

                let chunks =
                    crate::core::chunk::chunk(&self.providers, chunk_cfg.to_owned(), &content)
                        .await?;

                let chunks = match chunks {
                    ChunkedDocument::Ref(r) => r,
                    ChunkedDocument::Owned(ref o) => o.iter().map(|s| s.as_str()).collect(),
                };
                debug_assert_eq!(chunks, embeddings.chunks);
            }

            tracing::info!("{} - using cached embeddings", document.id);

            let collection_name = collection.name.clone();

            return transaction!(self.repo, |tx| async move {
                debug_assert_eq!(embeddings.chunks.len(), embeddings.embeddings.len());

                self.repo
                    .insert_embeddings(EmbeddingInsert::new(document.id, collection.id), Some(tx))
                    .await?;

                let report = report
                    .model_used(collection.model)
                    .embedding_provider(collection.embedder.clone())
                    .tokens_used(Some(0))
                    .total_vectors(embeddings.embeddings.len())
                    .vector_db(collection.provider)
                    .from_cache()
                    .finished_at(Utc::now())
                    .build();

                self.store_embedding_report(&report).await?;

                vector_db
                    .insert_embeddings(
                        document.id,
                        &collection_name,
                        &embeddings
                            .chunks
                            .iter()
                            .map(|s| s.as_str())
                            .collect::<Vec<&str>>(),
                        embeddings.embeddings,
                    )
                    .await?;

                Ok(report)
            });
        }

        // Read, parse, chunk, embed

        let content_bytes = storage.read(&document.path).await?;

        // FIXME: We do not have to assign here, and we can avoid remapping to/from owned multiple times
        let (chunks, embeddings) = match parse_cfg {
            ParseConfig::Generic(config) => {
                // In case of generic parsers, there will always be an associated chunker.
                let Some(chunk_cfg) = chunk_cfg else {
                    unreachable!("generic parsers always have a default chunker")
                };

                tracing::info!("{} - parsing generic", document.name);

                let content =
                    Parser::new(config).parse(document.ext.try_into()?, &content_bytes)?;

                let chunks =
                    crate::core::chunk::chunk(&self.providers, chunk_cfg, &content).await?;

                let chunks = match chunks {
                    ChunkedDocument::Ref(r) => r,
                    ChunkedDocument::Owned(ref o) => o.iter().map(|s| s.as_str()).collect(),
                };

                tracing::info!(
                    "{} - generating embeddings ({} total chunks)",
                    document.name,
                    chunks.len()
                );

                let embeddings = embedder.embed(&chunks, &collection.model).await?;

                (
                    chunks
                        .iter()
                        .map(|s| s.to_string())
                        .collect::<Vec<String>>(),
                    embeddings,
                )
            }
            ParseConfig::Sectioned(config) => {
                tracing::info!("{} - parsing sections", document.name);

                // In case of sectioned parsers, we define the sections as chunks
                let content =
                    Parser::new(config).parse(document.ext.try_into()?, &content_bytes)?;

                let sections = content
                    .into_iter()
                    .map(|s| {
                        s.pages.into_iter().fold(String::new(), |mut acc, el| {
                            acc.push_str(&el.content);
                            acc.push('\n');
                            acc
                        })
                    })
                    .collect::<Vec<String>>();

                let chunks = sections.iter().map(|s| s.as_str()).collect::<Vec<&str>>();

                tracing::info!(
                    "{} - generating embeddings ({} total chunks)",
                    document.name,
                    chunks.len()
                );

                let embeddings = embedder.embed(&chunks, &collection.model).await?;

                (
                    chunks
                        .iter()
                        .map(|s| s.to_string())
                        .collect::<Vec<String>>(),
                    embeddings,
                )
            }
        };

        tracing::info!("{} - storing embeddings", document.name);

        debug_assert_eq!(chunks.len(), embeddings.embeddings.len());

        transaction!(self.repo, |tx| async move {
            // Repository operations go first since we can revert those with the tx

            self.repo
                .insert_embeddings(EmbeddingInsert::new(document.id, collection.id), Some(tx))
                .await?;

            let report = report
                .model_used(collection.model)
                .embedding_provider(collection.embedder.clone())
                .tokens_used(embeddings.tokens_used)
                .total_vectors(chunks.len())
                .vector_db(collection.provider)
                .finished_at(Utc::now())
                .build();

            self.store_embedding_report(&report).await?;

            tracing::info!("{} - caching embeddings", document.name);

            if let Err(e) = self
                .cache
                .set(
                    &cache_key,
                    CachedEmbeddings::new(
                        embeddings.embeddings.clone(),
                        embeddings.tokens_used,
                        chunks.iter().map(|s| s.to_string()).collect(),
                    ),
                )
                .await
            {
                tracing::warn!("{} - failed to cache embeddings: {}", document.name, e);
            }

            vector_db
                .insert_embeddings(
                    document.id,
                    &collection.name,
                    &chunks.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
                    embeddings.embeddings,
                )
                .await?;

            tracing::info!("{} - successfully processed", document.name);

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

            tracing::info!(
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

        tracing::info!(
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
