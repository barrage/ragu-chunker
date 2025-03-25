use crate::core::model::collection::{
    Collection, CollectionDisplay, CollectionInsert, CollectionSearchColumn,
};
use crate::core::model::{List, PaginationSort};
use crate::core::provider::ProviderState;
use crate::core::repo::{Atomic, Repository};
use crate::core::vector::CreateVectorCollection;
use crate::error::ChonkitError;
use crate::{err, map_err, transaction};
use dto::{CreateCollectionPayload, SearchPayload};
use tracing::info;
use uuid::Uuid;
use validify::{Validate, Validify};

/// High level operations related to collections.
#[derive(Clone)]
pub struct CollectionService {
    repo: Repository,
    providers: ProviderState,
}

impl CollectionService {
    pub fn new(repo: Repository, providers: ProviderState) -> Self {
        Self { repo, providers }
    }
}

impl CollectionService {
    /// List vector collections.
    ///
    /// * `p`: Pagination params.
    pub async fn list_collections(
        &self,
        p: PaginationSort<CollectionSearchColumn>,
    ) -> Result<List<Collection>, ChonkitError> {
        map_err!(p.validate());
        self.repo.list_collections(p).await
    }

    pub async fn list_collections_display(
        &self,
        p: PaginationSort<CollectionSearchColumn>,
    ) -> Result<List<CollectionDisplay>, ChonkitError> {
        map_err!(p.validate());
        self.repo.list_collections_display(p).await
    }

    /// Get the collection for the given ID.
    ///
    /// * `id`: Collection ID.
    pub async fn get_collection(&self, id: Uuid) -> Result<Collection, ChonkitError> {
        match self.repo.get_collection_by_id(id).await? {
            Some(collection) => Ok(collection),
            None => err!(DoesNotExist, "Collection with ID '{id}'"),
        }
    }

    /// Get the collection for the given ID with additional info for display purposes.
    ///
    /// * `id`: Collection ID.
    pub async fn get_collection_display(
        &self,
        id: Uuid,
    ) -> Result<CollectionDisplay, ChonkitError> {
        let collection = self.repo.get_collection_display(id).await?;
        match collection {
            Some(collection) => Ok(collection),
            None => err!(DoesNotExist, "Collection with ID '{id}'"),
        }
    }

    /// Get the collection for the given name and provider unique combo.
    ///
    /// * `name`: Collection name.
    /// * `provider`: Vector provider.
    pub async fn get_collection_by_name(
        &self,
        name: &str,
        provider: &str,
    ) -> Result<Collection, ChonkitError> {
        let collection = self.repo.get_collection_by_name(name, provider).await?;
        match collection {
            Some(collection) => Ok(collection),
            None => err!(DoesNotExist, "Collection '{name}'"),
        }
    }

    /// Create a collection in the vector DB and store its info in the repository.
    ///
    /// * `data`: Creation data.
    pub async fn create_collection(
        &self,
        mut data: CreateCollectionPayload,
    ) -> Result<Collection, ChonkitError> {
        map_err!(data.validify());

        let CreateCollectionPayload {
            name,
            model,
            vector_provider,
            embedding_provider,
            groups,
        } = data;

        let vector_db = self.providers.vector.get_provider(&vector_provider)?;
        let embedder = self.providers.embedding.get_provider(&embedding_provider)?;

        let Some(size) = embedder.size(&model).await? else {
            let embedder_id = embedder.id();
            return err!(
                InvalidEmbeddingModel,
                "Model {model} not supported by embedder '{embedder_id}'"
            );
        };

        info!("Creating collection '{name}' of size '{size}'",);

        transaction!(self.repo, |tx| async move {
            let insert = CollectionInsert::new(&name, &model, embedder.id(), vector_db.id());
            let collection = self.repo.insert_collection(insert, Some(tx)).await?;

            let data = CreateVectorCollection::new(
                collection.id,
                &name,
                size,
                &embedding_provider,
                &model,
                groups,
            );

            vector_db.create_vector_collection(data).await?;

            Ok(collection)
        })
    }

    /// Delete a vector collection and all its corresponding embedding entries.
    /// It is assumed the vector provider has a collection with the name
    /// equal to the one found in the collection with the given ID.
    ///
    /// * `id`: Collection ID.
    pub async fn delete_collection(&self, id: Uuid) -> Result<u64, ChonkitError> {
        let Some(collection) = self.repo.get_collection_by_id(id).await? else {
            return err!(DoesNotExist, "Collection with ID '{id}'");
        };
        let vector_db = self.providers.vector.get_provider(&collection.provider)?;
        vector_db.delete_vector_collection(&collection.name).await?;
        let count = self.repo.delete_collection(id).await?;
        Ok(count)
    }

    /// Sync the collections in the repository with the ones in the vector DB.
    pub async fn sync(&self) -> Result<(), ChonkitError> {
        tracing::info!("Starting collection sync");

        for provider in self.providers.vector.list_provider_ids() {
            let v_provider = self.providers.vector.get_provider(provider)?;

            let collections = self
                .repo
                .list_collections(PaginationSort::default())
                .await?;

            for collection in collections {
                if let Err(e) = v_provider.get_collection(&collection.name).await {
                    tracing::error!("Error getting collection: {e}");
                    tracing::debug!("Deleting collection '{}' from database", collection.name);
                    self.repo.delete_collection(collection.id).await?;
                };
            }

            let v_collections = v_provider.list_vector_collections().await;

            for v_collection in v_collections {
                match v_collection {
                    Ok(v_collection) => {
                        let collection = self
                            .repo
                            .get_collection_by_name(&v_collection.name, provider)
                            .await?;

                        if collection.is_none() {
                            tracing::info!(
                                "Inserting collection '{}' to database",
                                v_collection.name
                            );
                            let collection = CollectionInsert::new(
                                &v_collection.name,
                                &v_collection.embedding_model,
                                &v_collection.embedding_provider,
                                v_provider.id(),
                            );
                            self.repo.insert_collection(collection, None).await?;
                        }
                    }
                    Err(e) => {
                        tracing::warn!("Skipping collection: {e}");
                    }
                }
            }
        }

        Ok(())
    }

    /// Query the vector database (semantic search).
    /// Limit defaults to 5.
    ///
    /// * `input`: Search params.
    pub async fn search(
        &self,
        search: SearchPayload,
    ) -> Result<dto::CollectionSearchResult, ChonkitError> {
        map_err!(search.validate());

        let collection = self.get_collection(search.collection_id).await?;

        let vector_db = self.providers.vector.get_provider(&collection.provider)?;
        let embedder = self
            .providers
            .embedding
            .get_provider(&collection.embedder)?;

        let mut embeddings = embedder.embed(&[&search.query], &collection.model).await?;

        debug_assert_eq!(1, embeddings.embeddings.len());

        let chunks = vector_db
            .query(
                std::mem::take(&mut embeddings.embeddings[0]),
                &collection.name,
                search.limit.unwrap_or(5),
                search.max_distance,
            )
            .await?;

        tracing::debug!("search - successful query ({} results)", chunks.len());

        Ok(dto::CollectionSearchResult {
            query: search.query,
            items: chunks,
        })
    }
}

pub mod dto {
    use serde::{Deserialize, Serialize};
    use utoipa::ToSchema;
    use uuid::Uuid;
    use validify::{field_err, Validate, ValidationError, Validify};

    use crate::core::vector::CollectionSearchItem;

    fn ascii_alphanumeric_underscored(s: &str) -> Result<(), ValidationError> {
        if !s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            return Err(field_err!(
                "collection_name",
                "collection name must be alphanumeric with underscores [a-z A-Z 0-9 _]"
            ));
        }
        Ok(())
    }

    fn begins_with_capital_ascii_letter(s: &str) -> Result<(), ValidationError> {
        if s.starts_with('_')
            || s.chars()
                .next()
                .is_some_and(|c| !c.is_ascii_alphabetic() || c.is_lowercase())
        {
            return Err(field_err!(
                "collection_name",
                "collection name must start with a capital characer [A-Z]"
            ));
        }
        Ok(())
    }

    #[derive(Debug, Deserialize, Validify, ToSchema)]
    #[serde(rename_all = "camelCase")]
    pub struct CreateCollectionPayload {
        /// Collection name. Cannot contain special characters.
        #[validate(custom(ascii_alphanumeric_underscored))]
        #[validate(custom(begins_with_capital_ascii_letter))]
        #[validate(length(min = 1))]
        #[modify(trim)]
        pub name: String,

        /// Collection embedding model.
        pub model: String,

        /// Vector database provider.
        pub vector_provider: String,

        /// Embeddings provider.
        pub embedding_provider: String,

        /// Optional collection groups that indicate which user groups can use it.
        #[validate(length(min = 1))]
        pub groups: Option<Vec<String>>,
    }

    /// Params for semantic search.
    #[derive(Debug, Deserialize, Validate, ToSchema)]
    #[serde(rename_all = "camelCase")]
    pub struct SearchPayload {
        /// The text to search by.
        #[validate(length(min = 1))]
        pub query: String,

        /// The collection to search in. Has priority over
        /// everything else.
        pub collection_id: Uuid,

        /// Amount of results to return.
        #[validate(range(min = 1.))]
        pub limit: Option<u32>,

        /// The similarity threshold for vector retrieval, between 0 and 2. Any similiarity
        /// below this value will be excluded.
        /// A similarity of 0 means the vectors are identical, a similarity of 2 means they are
        /// opposite.
        #[validate(range(min = 0., max = 2.))]
        pub max_distance: Option<f64>,
    }

    #[derive(Debug, Serialize, ToSchema)]
    #[serde(rename_all = "camelCase")]
    pub struct CollectionSearchResult {
        pub query: String,
        pub items: Vec<CollectionSearchItem>,
    }
}
