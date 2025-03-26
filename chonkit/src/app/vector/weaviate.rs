use crate::config::WEAVIATE_ID;
use crate::core::provider::Identity;
use crate::core::vector::{
    CollectionSearchItem, CreateVectorCollection, VectorCollection, VectorDb,
    COLLECTION_EMBEDDING_MODEL_PROPERTY, COLLECTION_EMBEDDING_PROVIDER_PROPERTY,
    COLLECTION_GROUPS_PROPERTY, COLLECTION_ID_PROPERTY, COLLECTION_NAME_PROPERTY,
    COLLECTION_SIZE_PROPERTY, CONTENT_PROPERTY, DOCUMENT_ID_PROPERTY,
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
            match get_id_vector(self, &class.class).await {
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
        let props = create_collection_properties()?;
        let class = class.with_properties(props).build();

        if let Err(e) = self.schema.create_class(&class).await {
            tracing::error!("error creating class: {}", e);
            return err!(Weaviate, "{}", e);
        }

        if let Err(e) = upsert_id_vector(self, data).await {
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

        let collection = get_id_vector(self, collection).await?.with_groups(groups);

        if let Err(e) = upsert_id_vector(self, (&collection).into()).await {
            tracing::error!("error upserting identity vector: {}", e);
            return err!(Weaviate, "{}", e);
        };

        Ok(())
    }

    async fn get_collection(&self, name: &str) -> Result<VectorCollection, ChonkitError> {
        if let Err(e) = self.schema.get_class(name).await {
            return err!(Weaviate, "{e}");
        };
        get_id_vector(self, name).await
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
        let query = GetQuery::builder(collection, vec![CONTENT_PROPERTY])
            .with_near_vector(near_vector)
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
                let content =
                    match serde_json::from_value::<String>(obj.get(CONTENT_PROPERTY).cloned()?) {
                        Ok(content) => content,
                        Err(e) => {
                            tracing::error!("weaviate - failed to parse content: {e}");
                            return None;
                        }
                    };

                let Some(max_distance) = max_distance else {
                    return Some(CollectionSearchItem {
                        content,
                        distance: None,
                    });
                };

                let Some(distance) = obj.get("_additional") else {
                    return Some(CollectionSearchItem {
                        content,
                        distance: None,
                    });
                };

                // If the max_distance is specified, but there is no distance in the result,
                // return None

                let distance = distance.get("distance")?.as_f64()?;

                if distance <= max_distance {
                    Some(CollectionSearchItem {
                        content,
                        distance: Some(distance),
                    })
                } else {
                    tracing::debug!(
                        "weaviate - skipping chunk due to distance ({} > {})",
                        distance,
                        max_distance
                    );
                    None
                }
            })
            .collect())
    }

    async fn insert_embeddings(
        &self,
        document_id: Uuid,
        collection: &str,
        content: &[&str],
        vectors: Vec<Vec<f64>>,
    ) -> Result<(), ChonkitError> {
        debug_assert_eq!(content.len(), vectors.len());

        let objects = content
            .iter()
            .zip(vectors.into_iter())
            .map(|(content, vector)| {
                let properties = json!({
                    CONTENT_PROPERTY: content,
                    DOCUMENT_ID_PROPERTY: document_id
                });
                Object::builder(collection, properties)
                    .with_vector(vector)
                    .with_id(uuid::Uuid::new_v4())
                    .build()
            })
            .collect();

        let objects = MultiObjects::new(objects);

        if let Err(e) = self
            .batch
            .objects_batch_add(objects, Some(ConsistencyLevel::ONE), None)
            .await
        {
            tracing::error!("error inserting embeddings: {}", e);
            return err!(Weaviate, "{}", e);
        }

        Ok(())
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

async fn get_id_vector(
    weaviate: &WeaviateClient,
    collection: &str,
) -> Result<VectorCollection, ChonkitError> {
    let query = GetQuery::builder(
        collection,
        vec![
            COLLECTION_ID_PROPERTY,
            COLLECTION_NAME_PROPERTY,
            COLLECTION_SIZE_PROPERTY,
            COLLECTION_EMBEDDING_PROVIDER_PROPERTY,
            COLLECTION_EMBEDDING_MODEL_PROPERTY,
            COLLECTION_GROUPS_PROPERTY,
        ],
    )
    .with_where(&format!(
        "{{ path: [\"id\"], operator: Equal, valueText: \"{}\" }}",
        uuid::Uuid::nil()
    ))
    .with_limit(1)
    .build();

    let response = match weaviate.query.get(query).await {
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

async fn upsert_id_vector(
    weaviate: &WeaviateClient,
    collection: CreateVectorCollection<'_>,
) -> Result<(), Box<dyn std::error::Error>> {
    let properties = json!({
        COLLECTION_ID_PROPERTY: collection.collection_id,
        COLLECTION_NAME_PROPERTY: collection.name,
        COLLECTION_SIZE_PROPERTY: collection.size,
        COLLECTION_EMBEDDING_PROVIDER_PROPERTY: collection.embedding_provider,
        COLLECTION_EMBEDDING_MODEL_PROPERTY: collection.embedding_model,
        COLLECTION_GROUPS_PROPERTY: collection.groups
    });

    let _ = weaviate
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

    weaviate
        .objects
        .create(&object, Some(ConsistencyLevel::ONE))
        .await?;

    Ok(())
}

/// Create properties for a collection (weaviate class). We need classes
/// to also have defined properties because otherwise some foul weaviate
/// treachery with wrong types will occur.
fn create_collection_properties() -> Result<Properties, ChonkitError> {
    let id = PropertyBuilder::new(COLLECTION_ID_PROPERTY, vec!["uuid"]).build();

    let size = PropertyBuilder::new(COLLECTION_SIZE_PROPERTY, vec!["int"]).build();

    let name = PropertyBuilder::new(COLLECTION_NAME_PROPERTY, vec!["text"]).build();

    let embedding_provider =
        PropertyBuilder::new(COLLECTION_EMBEDDING_PROVIDER_PROPERTY, vec!["text"]).build();

    let embedding_model =
        PropertyBuilder::new(COLLECTION_EMBEDDING_MODEL_PROPERTY, vec!["text"]).build();

    let groups = PropertyBuilder::new(COLLECTION_GROUPS_PROPERTY, vec!["text[]"]).build();

    Ok(Properties::new(vec![
        id,
        size,
        name,
        embedding_provider,
        embedding_model,
        groups,
    ]))
}

// Attempt to parse Weaviate GraphQL data to a [dto::WeaviateError].
// fn parse_weaviate_error(s: &str) -> Option<WeaviateError> {
//     let json_err = s.rsplit_once("Response: ")?.1;
//     serde_json::from_str(json_err).ok()
// }

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
            vector::weaviate::WeaviateDb,
        },
        core::vector::{CreateVectorCollection, VectorDb},
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

        assert_eq!(id, collection.id);
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
}
