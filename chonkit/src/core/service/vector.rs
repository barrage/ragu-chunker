use crate::core::model::collection::{Collection, CollectionDisplay, CollectionInsert};
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
        p: PaginationSort,
    ) -> Result<List<Collection>, ChonkitError> {
        map_err!(p.validate());
        self.repo.list_collections(p).await
    }

    pub async fn list_collections_display(
        &self,
        p: PaginationSort,
    ) -> Result<List<CollectionDisplay>, ChonkitError> {
        map_err!(p.validate());
        self.repo.list_collections_display(p).await
    }

    /// Get the collection for the given ID.
    ///
    /// * `id`: Collection ID.
    pub async fn get_collection(&self, id: Uuid) -> Result<Collection, ChonkitError> {
        let collection = self.repo.get_collection(id).await?;
        match collection {
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
        let Some(collection) = self.repo.get_collection(id).await? else {
            return err!(DoesNotExist, "Collection with ID '{id}'");
        };
        let vector_db = self.providers.vector.get_provider(&collection.provider)?;
        vector_db.delete_vector_collection(&collection.name).await?;
        let count = self.repo.delete_collection(id).await?;
        Ok(count)
    }

    /// Query the vector database (semantic search).
    /// Limit defaults to 5.
    ///
    /// * `input`: Search params.
    pub async fn search(&self, mut search: SearchPayload) -> Result<Vec<String>, ChonkitError> {
        map_err!(search.validify());

        let collection = if let Some(collection_id) = search.collection_id {
            self.get_collection(collection_id).await?
        } else {
            let (Some(name), Some(provider)) = (&search.collection_name, &search.provider) else {
                // Cannot happen because of above validify
                unreachable!("not properly validated");
            };

            self.get_collection_by_name(name, provider).await?
        };

        let vector_db = self.providers.vector.get_provider(&collection.provider)?;
        let embedder = self
            .providers
            .embedding
            .get_provider(&collection.embedder)?;

        let mut embeddings = embedder.embed(&[&search.query], &collection.model).await?;

        debug_assert_eq!(1, embeddings.embeddings.len());

        vector_db
            .query(
                std::mem::take(&mut embeddings.embeddings[0]),
                &collection.name,
                search.limit.unwrap_or(5),
            )
            .await
    }
}

pub mod dto {
    use serde::Deserialize;
    use utoipa::ToSchema;
    use uuid::Uuid;
    use validify::{
        field_err, schema_err, schema_validation, ValidationError, ValidationErrors, Validify,
    };

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
    }

    /// Params for semantic search.
    #[derive(Debug, Deserialize, Validify, ToSchema)]
    #[serde(rename_all = "camelCase")]
    #[validate(Self::validate_schema)]
    pub struct SearchPayload {
        /// The text to search by.
        #[modify(trim)]
        pub query: String,

        /// The collection to search in. Has priority over
        /// everything else.
        pub collection_id: Option<Uuid>,

        /// If given search via the name and provider combo.
        #[validate(length(min = 1))]
        #[modify(trim)]
        pub collection_name: Option<String>,

        /// Vector provider.
        pub provider: Option<String>,

        /// Amount of results to return.
        #[validate(range(min = 1.))]
        pub limit: Option<u32>,
    }

    impl SearchPayload {
        #[schema_validation]
        fn validate_schema(&self) -> Result<(), ValidationErrors> {
            let SearchPayload {
                collection_id,
                collection_name,
                provider,
                ..
            } = self;
            match (collection_id, collection_name, provider) {
                (None, None, None) => {
                    schema_err!(
                        "either_id_or_name_and_provider",
                        "one of either `collection_id`, or `provider` and `collection_name` combination must be set"
                    );
                }
                (None, Some(_), None) | (None, None, Some(_)) => {
                    schema_err!(
                    "name_and_provider",
                    "both 'collection_name'and 'provider' must be set if `collection_id` is not set"
                );
                }
                _ => {}
            }
        }
    }
}
