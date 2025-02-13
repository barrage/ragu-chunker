use crate::core::model::embedding::{
    Embedding, EmbeddingInsert, EmbeddingRemovalReportInsert, EmbeddingReport,
    EmbeddingReportInsert,
};
use crate::core::model::{List, Pagination};
use crate::core::provider::ProviderState;
use crate::core::repo::Repository;
use crate::error::ChonkitError;
use crate::{err, map_err};
use serde::Serialize;
use uuid::Uuid;
use validify::{Validate, Validify};

#[derive(Clone)]
pub struct EmbeddingService {
    repo: Repository,
    providers: ProviderState,
}

impl EmbeddingService {
    pub fn new(repo: Repository, providers: ProviderState) -> Self {
        Self { repo, providers }
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
        CreateEmbeddings {
            document_id,
            collection_id,
            chunks,
        }: CreateEmbeddings<'_>,
    ) -> Result<CreatedEmbeddings, ChonkitError> {
        // Make sure the collection exists.
        let Some(collection) = self.repo.get_collection(collection_id).await? else {
            return err!(DoesNotExist, "Collection with ID '{collection_id}'");
        };

        let existing = self.repo.get_embeddings(document_id, collection.id).await?;
        if existing.is_some() {
            let name = collection.name;
            return err!(
                AlreadyExists,
                "Embeddings for document '{document_id}' in collection '{name}'"
            );
        }

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

        let embeddings = embedder.embed(chunks, &collection.model).await?;

        let tokens_used = embeddings.tokens_used;

        debug_assert_eq!(chunks.len(), embeddings.embeddings.len());

        vector_db
            .insert_embeddings(document_id, &collection.name, chunks, embeddings.embeddings)
            .await?;

        let embeddings = self
            .repo
            .insert_embeddings(EmbeddingInsert::new(document_id, collection.id))
            .await?;

        Ok(CreatedEmbeddings {
            embeddings,
            tokens_used,
        })
    }

    /// Returns the number of rows deleted from the db and the number of vectors deleted from the collection.
    pub async fn delete_embeddings(
        &self,
        collection_id: Uuid,
        document_id: Uuid,
    ) -> Result<(u64, usize), ChonkitError> {
        let Some(collection) = self.repo.get_collection(collection_id).await? else {
            return err!(DoesNotExist, "Collection with ID '{collection_id}'");
        };

        let vector_db = self.providers.vector.get_provider(&collection.provider)?;

        let amount = vector_db
            .count_vectors(&collection.name, document_id)
            .await?;

        vector_db
            .delete_embeddings(&collection.name, document_id)
            .await?;

        let amount_deleted_db = self
            .repo
            .delete_embeddings(document_id, collection_id)
            .await?;

        tracing::info!(
            "Deleted {amount} vectors in collection '{}' ({amount_deleted_db} from db)",
            collection.name
        );

        Ok((amount_deleted_db, amount))
    }

    pub async fn delete_all_embeddings(&self, document_id: Uuid) -> Result<usize, ChonkitError> {
        let mut total_deleted = 0;

        let collections = self
            .repo
            .get_document_assigned_collection_names(document_id)
            .await?;

        for (collection, provider) in collections.iter() {
            let vector_db = self.providers.vector.get_provider(provider)?;
            let amount = vector_db.count_vectors(collection, document_id).await?;
            vector_db.delete_embeddings(collection, document_id).await?;
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
        let Some(collection) = self.repo.get_collection(collection_id).await? else {
            return err!(DoesNotExist, "Collection with ID '{collection_id}'");
        };
        let vector_db = self.providers.vector.get_provider(&collection.provider)?;
        vector_db.count_vectors(&collection.name, document_id).await
    }

    pub async fn list_collection_embedding_reports(
        &self,
        collection_id: Uuid,
    ) -> Result<Vec<EmbeddingReport>, ChonkitError> {
        self.repo
            .list_collection_embedding_reports(collection_id)
            .await
    }

    pub async fn store_embedding_report(
        &self,
        report: &EmbeddingReportInsert,
    ) -> Result<(), ChonkitError> {
        tracing::debug!(
            "Storing embedding report for document '{}' in '{}'",
            report.document_name,
            report.collection_name
        );
        self.repo.insert_embedding_report(report).await?;
        Ok(())
    }

    pub async fn store_embedding_removal_report(
        &self,
        report: &EmbeddingRemovalReportInsert,
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

#[derive(Debug, Clone, Validify)]
pub struct CreateEmbeddings<'a> {
    /// Document ID.
    pub document_id: Uuid,

    /// Which collection these embeddings are for.
    pub collection_id: Uuid,

    /// The chunked document.
    pub chunks: &'a [&'a str],
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct CreatedEmbeddings {
    pub embeddings: Embedding,
    pub tokens_used: Option<usize>,
}
