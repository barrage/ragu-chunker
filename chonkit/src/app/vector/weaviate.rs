use crate::config::WEAVIATE_ID;
use crate::core::provider::Identity;
use crate::core::vector::{
    CollectionItem, CollectionItemImage, CollectionItemInsert, CollectionItemInsertPayload,
    CollectionItemText, CollectionSearchItem, CreateVectorCollection, VectorCollection, VectorDb,
    COLLECTION_EMBEDDING_MODEL_PROPERTY, COLLECTION_EMBEDDING_PROVIDER_PROPERTY,
    COLLECTION_GROUPS_PROPERTY, COLLECTION_ID_PROPERTY, COLLECTION_NAME_PROPERTY,
    COLLECTION_SIZE_PROPERTY, DOCUMENT_ID_PROPERTY,
};
use crate::{err, error::ChonkitError, map_err};
use dto::{QueryResult, WeaviateError};
use serde_json::json;
use std::sync::Arc;
use uuid::Uuid;
use weaviate_community::collections::schema::{Properties, PropertyBuilder};
use weaviate_community::{
    collections::{
        batch::{BatchDeleteRequest, MatchConfig},
        objects::{ConsistencyLevel, MultiObjects, Object},
        query::GetQuery,
        schema::Class,
    },
    WeaviateClient,
};

/// Alias for an arced Weaviate instance.
pub type WeaviateDb = Arc<WeaviateClient>;

pub fn init(url: &str) -> WeaviateDb {
    tracing::info!("Connecting to weaviate at {url}");
    Arc::new(WeaviateClient::new(url, None, None).expect("error initialising weaviate"))
}

impl Identity for WeaviateClient {
    fn id(&self) -> &'static str {
        WEAVIATE_ID
    }
}

#[async_trait::async_trait]
impl VectorDb for WeaviateClient {
    async fn list_vector_collections(&self) -> Vec<Result<VectorCollection, ChonkitError>> {
        let mut results = vec![];

        let classes = match self.schema.get().await {
            Ok(classes) => classes,
            Err(e) => {
                tracing::error!("error getting classes: {}", e);
                return results;
            }
        };

        for class in classes.classes {
            match WeaviateInner::new(self).get_id_vector(&class.class).await {
                Ok(c) => results.push(Ok(c)),
                Err(e) => {
                    tracing::error!("error getting identity vector: {}", e);
                    results.push(err!(Weaviate, "{}", class.class));
                }
            };
        }

        results
    }

    async fn create_vector_collection(
        &self,
        data: CreateVectorCollection<'_>,
    ) -> Result<(), ChonkitError> {
        let class = Class::builder(data.name);

        // Create properties for a collection (weaviate class). We need classes
        // to also have defined properties because otherwise some foul weaviate
        // treachery with wrong types will occur.
        let id = PropertyBuilder::new(COLLECTION_ID_PROPERTY, vec!["uuid"]).build();
        let size = PropertyBuilder::new(COLLECTION_SIZE_PROPERTY, vec!["int"]).build();
        let name = PropertyBuilder::new(COLLECTION_NAME_PROPERTY, vec!["text"]).build();
        let embedding_provider =
            PropertyBuilder::new(COLLECTION_EMBEDDING_PROVIDER_PROPERTY, vec!["text"]).build();
        let embedding_model =
            PropertyBuilder::new(COLLECTION_EMBEDDING_MODEL_PROPERTY, vec!["text"]).build();
        let groups = PropertyBuilder::new(COLLECTION_GROUPS_PROPERTY, vec!["text[]"]).build();

        let props = Properties::new(vec![
            id,
            size,
            name,
            embedding_provider,
            embedding_model,
            groups,
        ]);

        let class = class.with_properties(props).build();

        if let Err(e) = self.schema.create_class(&class).await {
            tracing::error!("error creating class: {}", e);
            return err!(Weaviate, "{}", e);
        }

        if let Err(e) = WeaviateInner::new(self).upsert_id_vector(data).await {
            tracing::error!("error creating identity vector: {}", e);
            return err!(Weaviate, "{}", e);
        };

        Ok(())
    }

    async fn update_collection_groups(
        &self,
        collection: &str,
        groups: Option<Vec<String>>,
    ) -> Result<(), ChonkitError> {
        // Sanity check
        if let Err(e) = self.schema.get_class(collection).await {
            return err!(Weaviate, "{e}");
        };

        let inner = WeaviateInner::new(self);

        let collection = inner.get_id_vector(collection).await?.with_groups(groups);

        if let Err(e) = inner.upsert_id_vector((&collection).into()).await {
            tracing::error!("error upserting identity vector: {}", e);
            return err!(Weaviate, "{}", e);
        };

        Ok(())
    }

    async fn get_collection(&self, name: &str) -> Result<VectorCollection, ChonkitError> {
        if let Err(e) = self.schema.get_class(name).await {
            return err!(Weaviate, "{e}");
        };
        WeaviateInner::new(self).get_id_vector(name).await
    }

    async fn delete_vector_collection(&self, name: &str) -> Result<(), ChonkitError> {
        if let Err(e) = self.schema.delete(name).await {
            return err!(Weaviate, "{}", e);
        }
        Ok(())
    }

    async fn query(
        &self,
        search: Vec<f64>,
        collection: &str,
        limit: u32,
        max_distance: Option<f64>,
    ) -> Result<Vec<CollectionSearchItem>, ChonkitError> {
        tracing::debug!("weaviate - querying collection '{collection}' (limit: {limit}, max_distance: {max_distance:?})");
        let near_vector = &format!("{{ vector: {search:?} }}");
        let query = GetQuery::builder(collection, CollectionItem::query_properties().to_vec())
            .with_near_vector(near_vector)
            .with_where(&format!(
                "{{ 
                    path: [\"id\"],
                    operator: NotEqual,
                    valueText: \"{}\" 
                }}",
                Uuid::nil()
            ))
            .with_limit(limit)
            .with_additional(vec!["distance"])
            .build();

        let response = match self.query.get(query).await {
            Ok(res) => res,
            Err(e) => return err!(Weaviate, "{}", e),
        };

        if response["data"].is_null() {
            tracing::warn!("weaviate - query is missing 'data' field; response: {response:?}");
            let error = map_err!(serde_json::from_value::<WeaviateError>(response));
            return err!(
                Weaviate,
                "{}",
                error
                    .errors
                    .into_iter()
                    .map(|e| e.message)
                    .collect::<Vec<_>>()
                    .join(";")
            );
        }

        let result: QueryResult = map_err!(serde_json::from_value(response));

        let Some(results) = result.data.get.get(collection) else {
            return err!(
                Weaviate,
                "Response error - cannot index into '{collection}' in {}",
                result.data.get
            );
        };

        let results = map_err!(serde_json::from_value::<Vec<serde_json::Value>>(
            results.clone()
        ));

        tracing::debug!("weaviate - successful query ({} results)", results.len());

        Ok(results
            .into_iter()
            .filter_map(|obj| {
                let try_get_distance = |obj: &serde_json::Value| -> Option<f64> {
                    obj.get("_additional")?.get("distance")?.as_f64()
                };

                let distance = try_get_distance(&obj);

                let is_greater = match (distance, max_distance) {
                    (Some(distance), Some(max)) if distance > max => {
                        tracing::debug!(
                            "weaviate - skipping object distance > max ({} > {})",
                            distance,
                            max
                        );
                        true
                    }
                    _ => false,
                };

                if is_greater {
                    return None;
                }

                match serde_json::from_value::<CollectionItem>(obj) {
                    Ok(item) => Some(CollectionSearchItem::new(item, distance)),
                    Err(e) => {
                        tracing::error!("weaviate - failed to parse item: {e}");
                        None
                    }
                }
            })
            .collect())
    }

    async fn insert_embeddings(
        &self,
        insert: CollectionItemInsert<'_>,
    ) -> Result<(), ChonkitError> {
        let client = WeaviateInner::new(self);
        match insert.payload {
            CollectionItemInsertPayload::Text { items, vectors } => {
                client
                    .insert_text_embeddings(insert.collection, items, vectors)
                    .await
            }
            CollectionItemInsertPayload::Image { item, vector } => {
                client
                    .insert_image_embeddings(insert.collection, item, vector)
                    .await
            }
        }
    }

    async fn delete_embeddings(
        &self,
        collection: &str,
        document_id: Uuid,
    ) -> Result<(), ChonkitError> {
        let delete = BatchDeleteRequest::builder(MatchConfig::new(
            collection,
            json!({
                "path": [DOCUMENT_ID_PROPERTY],
                "operator": "Equal",
                "valueText": document_id.to_string()
            }),
        ))
        .build();

        if let Err(e) = self
            .batch
            .objects_batch_delete(delete, Some(ConsistencyLevel::ALL), None)
            .await
        {
            tracing::error!("error deleting vectors: {}", e);
            return err!(Weaviate, "{}", e);
        }

        Ok(())
    }

    async fn count_vectors(
        &self,
        collection: &str,
        document_id: Uuid,
    ) -> Result<usize, ChonkitError> {
        let query = GetQuery::builder(collection, vec![DOCUMENT_ID_PROPERTY])
            .with_where(&format!(
                "{{ 
                    path: [\"{DOCUMENT_ID_PROPERTY}\"],
                    operator: Equal,
                    valueText: \"{document_id}\" 
                }}"
            ))
            .build();

        let response = match self.query.get(query).await {
            Ok(res) => res,
            Err(e) => return err!(Weaviate, "{}", e),
        };

        if response["data"].is_null() {
            tracing::warn!("Weaviate query is missing 'data' field; response: {response:?}");
            let error = map_err!(serde_json::from_value::<WeaviateError>(response));
            return err!(
                Weaviate,
                "{}",
                error
                    .errors
                    .into_iter()
                    .map(|e| e.message)
                    .collect::<Vec<_>>()
                    .join(";")
            );
        }

        let result: QueryResult = map_err!(serde_json::from_value(response));

        let Some(results) = result.data.get.get(collection) else {
            return err!(
                Weaviate,
                "Response error - cannot index into '{collection}' in {}",
                result.data.get
            );
        };

        let amount = map_err!(serde_json::from_value::<Vec<serde_json::Value>>(
            results.clone()
        ))
        .len();

        Ok(amount)
    }
}

struct WeaviateInner<'a> {
    client: &'a WeaviateClient,
}

impl<'a> WeaviateInner<'a> {
    fn new(client: &'a WeaviateClient) -> Self {
        Self { client }
    }

    async fn upsert_id_vector(
        &self,
        collection: CreateVectorCollection<'_>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let properties = map_err!(serde_json::to_value(&collection));

        let _ = self
            .client
            .objects
            .delete(
                collection.name,
                &uuid::Uuid::nil(),
                Some(ConsistencyLevel::ONE),
                None,
            )
            .await;

        let object = Object::builder(collection.name, properties)
            .with_vector(vec![0.0; collection.size])
            .with_id(uuid::Uuid::nil())
            .build();

        self.client
            .objects
            .create(&object, Some(ConsistencyLevel::ONE))
            .await?;

        Ok(())
    }

    async fn get_id_vector(&self, collection: &str) -> Result<VectorCollection, ChonkitError> {
        let query = GetQuery::builder(collection, VectorCollection::query_properties().to_vec())
            .with_where(&format!(
                "{{ path: [\"id\"], operator: Equal, valueText: \"{}\" }}",
                uuid::Uuid::nil()
            ))
            .with_limit(1)
            .build();

        let response = match self.client.query.get(query).await {
            Ok(res) => res,
            Err(e) => return err!(Weaviate, "{}", e),
        };

        if response["data"].is_null() {
            tracing::warn!("weaviate - query is missing 'data' field; response: {response:?}");
            let error = map_err!(serde_json::from_value::<WeaviateError>(response));
            return err!(
                Weaviate,
                "{}",
                error
                    .errors
                    .into_iter()
                    .map(|e| e.message)
                    .collect::<Vec<_>>()
                    .join(";")
            );
        }

        let result: QueryResult = map_err!(serde_json::from_value(response));

        let Some(results) = result.data.get.get(collection) else {
            return err!(
                Weaviate,
                "Response error - cannot index into '{collection}' in {}",
                result.data.get
            );
        };

        let mut results = map_err!(serde_json::from_value::<Vec<serde_json::Value>>(
            results.clone()
        ));

        if results.is_empty() {
            return err!(Weaviate, "No result found for ID vector query");
        }

        let result = results.remove(0);

        let collection_info: VectorCollection = map_err!(serde_json::from_value(result));

        Ok(collection_info)
    }

    async fn insert_text_embeddings(
        &self,
        collection: &str,
        content: Vec<CollectionItemText<'_>>,
        vectors: Vec<Vec<f64>>,
    ) -> Result<(), ChonkitError> {
        debug_assert_eq!(content.len(), vectors.len());

        let objects = content
            .iter()
            .zip(vectors.into_iter())
            .filter_map(|(content, vector)| {
                let properties = serde_json::to_value(content).ok()?;
                Some(
                    Object::builder(collection, properties)
                        .with_vector(vector)
                        .with_id(uuid::Uuid::new_v4())
                        .build(),
                )
            })
            .collect();

        let objects = MultiObjects::new(objects);

        if let Err(e) = self
            .client
            .batch
            .objects_batch_add(objects, Some(ConsistencyLevel::ONE), None)
            .await
        {
            tracing::error!("error inserting text embeddings: {}", e);
            return err!(Weaviate, "{}", e);
        }

        Ok(())
    }

    async fn insert_image_embeddings(
        &self,
        collection: &str,
        payload: CollectionItemImage<'_>,
        vector: Vec<f64>,
    ) -> Result<(), ChonkitError> {
        let properties = map_err!(serde_json::to_value(payload));

        let object = Object::builder(collection, properties)
            .with_vector(vector)
            .with_id(uuid::Uuid::new_v4())
            .build();

        if let Err(e) = self
            .client
            .objects
            .create(&object, Some(ConsistencyLevel::ONE))
            .await
        {
            tracing::error!("error inserting image embeddings: {}", e);
            return err!(Weaviate, "{}", e);
        }

        Ok(())
    }
}

mod dto {
    use serde::Deserialize;

    #[derive(Debug, Deserialize)]
    pub struct WeaviateError {
        pub errors: Vec<ErrorMessage>,
    }

    #[derive(Debug, Deserialize)]
    pub struct ErrorMessage {
        pub message: String,
    }

    #[derive(Debug, Deserialize)]
    pub struct QueryResult {
        pub data: GetResult,
    }

    #[derive(Debug, Deserialize)]
    pub struct GetResult {
        #[serde(rename = "Get")]
        pub get: serde_json::Value,
    }
}

#[cfg(test)]
#[suitest::suite(weaviate_tests)]
#[suitest::suite_cfg(sequential = true)]
mod weaviate_tests {
    use crate::{
        app::{
            test::{init_weaviate, AsyncContainer},
            vector::weaviate::{WeaviateDb, WeaviateInner},
        },
        core::vector::{CollectionItemText, CreateVectorCollection, VectorDb},
    };
    use suitest::before_all;
    use uuid::Uuid;

    #[before_all]
    async fn setup() -> (WeaviateDb, AsyncContainer) {
        let (weaver, img) = init_weaviate().await;
        (weaver, img)
    }

    #[test]
    async fn creates_collection(weaver: WeaviateDb) {
        let name = "My_collection_0";
        let id = Uuid::new_v4();
        let groups = vec!["admin".to_string(), "user".to_string()];

        let data = CreateVectorCollection::new(
            id,
            name,
            420,
            "openai",
            "text-embedding-ada-002",
            Some(groups.clone()),
        );

        weaver.create_vector_collection(data).await.unwrap();

        let collection = weaver.get_collection(name).await.unwrap();

        assert_eq!(id, collection.collection_id);
        assert_eq!(name, collection.name);
        assert_eq!(420, collection.size);
        assert_eq!(groups, collection.groups.unwrap());
        assert_eq!("openai", collection.embedding_provider);
        assert_eq!("text-embedding-ada-002", collection.embedding_model);

        weaver.delete_vector_collection(name).await.unwrap();
    }

    #[test]
    async fn updates_collection_groups(weaver: WeaviateDb) {
        let name = "My_collection_with_groups";
        let id = Uuid::new_v4();
        let groups = vec!["admin".to_string(), "user".to_string()];

        let collection =
            CreateVectorCollection::new(id, name, 420, "openai", "text-embedding-ada-002", None);

        weaver.create_vector_collection(collection).await.unwrap();

        let collection = weaver.get_collection(name).await.unwrap();

        assert!(collection.groups.is_none());

        weaver
            .update_collection_groups(name, Some(groups.clone()))
            .await
            .unwrap();

        let collection = weaver.get_collection(name).await.unwrap();

        assert_eq!(groups, collection.groups.unwrap());
    }

    #[test]
    async fn queries_collection_skipping_id_vector(weaviate: WeaviateDb) {
        let name = "My_collection_for_query";
        let id = Uuid::new_v4();
        let document_id = Uuid::new_v4();

        let collection =
            CreateVectorCollection::new(id, name, 420, "openai", "text-embedding-ada-002", None);

        weaviate.create_vector_collection(collection).await.unwrap();

        WeaviateInner::new(weaviate)
            .insert_text_embeddings(
                name,
                vec![CollectionItemText {
                    content: "foo",
                    document_id,
                }],
                vec![vec![0.420f64; 420]],
            )
            .await
            .unwrap();

        let results = weaviate
            .query(vec![0.420f64; 420], name, 420, None)
            .await
            .unwrap();

        assert_eq!(1, results.len());
        assert_eq!("foo", results[0].item.payload.as_content());

        weaviate.delete_vector_collection(name).await.unwrap();
    }
}
