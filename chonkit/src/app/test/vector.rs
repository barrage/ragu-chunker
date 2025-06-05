// Tests vector service integration depending on the features used.
#[cfg(all(
    test,
    all(any(feature = "qdrant", feature = "weaviate"), feature = "fe-local")
))]
#[suitest::suite(integration_tests)]
mod vector_service_integration_tests {
    use crate::{
        app::test::{TestState, TestStateConfig, DEFAULT_MODELS},
        config::{DEFAULT_COLLECTION_NAME, FEMBED_EMBEDDER_ID},
        core::{
            document::{DocumentType, TextDocumentType},
            service::{
                collection::dto::{CreateCollectionPayload, SearchPayload},
                document::dto::DocumentUpload,
                embedding::EmbedSingleInput,
            },
        },
        error::ChonkitErr,
    };
    use suitest::{after_all, before_all, cleanup};

    const TEST_UPLOAD_PATH: &str = "__vector_service_test_upload__";
    const TEST_GDRIVE_PATH: &str = "__vector_service_test_gdrive_download__";

    #[before_all]
    async fn setup() -> TestState {
        let _ = tokio::fs::remove_dir_all(TEST_UPLOAD_PATH).await;
        let _ = tokio::fs::remove_dir_all(TEST_GDRIVE_PATH).await;

        tokio::fs::create_dir(TEST_UPLOAD_PATH).await.unwrap();
        tokio::fs::create_dir(TEST_GDRIVE_PATH).await.unwrap();

        let test_state = TestState::init(TestStateConfig {
            fs_store_path: TEST_UPLOAD_PATH.to_string(),
            _gdrive_download_path: TEST_GDRIVE_PATH.to_string(),
        })
        .await;

        // azure
        // fn default_model(&self) -> (String, usize) {
        //     (String::from("text-embedding-ada-002"), 1536)
        // }

        for provider in test_state.active_vector_providers.iter() {
            let embedder = test_state
                .app
                .providers
                .embedding
                .get_provider(FEMBED_EMBEDDER_ID)
                .unwrap();

            let create = CreateCollectionPayload {
                name: format!("{DEFAULT_COLLECTION_NAME}_{}_{}", provider, embedder.id()),
                model: DEFAULT_MODELS
                    .get()
                    .unwrap()
                    .get(embedder.id())
                    .unwrap()
                    .name
                    .clone(),
                vector_provider: provider.to_string(),
                embedding_provider: embedder.id().to_string(),
                groups: None,
            };

            test_state
                .app
                .services
                .collection
                .create_collection(create)
                .await
                .unwrap();
        }

        test_state
    }

    #[cleanup]
    async fn cleanup() {
        let _ = tokio::fs::remove_dir_all(TEST_UPLOAD_PATH).await;
        let _ = tokio::fs::remove_dir_all(TEST_GDRIVE_PATH).await;
    }

    #[after_all]
    async fn teardown() {
        let _ = tokio::fs::remove_dir_all(TEST_UPLOAD_PATH).await;
        let _ = tokio::fs::remove_dir_all(TEST_GDRIVE_PATH).await;
    }

    #[test]
    async fn default_collection_stored_successfully(state: TestState) {
        let embedder = state
            .app
            .providers
            .embedding
            .get_provider(FEMBED_EMBEDDER_ID)
            .unwrap()
            .clone();

        for provider in state.active_vector_providers.iter() {
            let vector_db = state.app.providers.vector.get_provider(provider).unwrap();
            let collection_name =
                format!("{DEFAULT_COLLECTION_NAME}_{}_{}", provider, embedder.id());

            let collection = state
                .app
                .services
                .collection
                .get_collection_by_name(&collection_name, provider)
                .await
                .unwrap();

            assert_eq!(collection.name, collection_name);
            assert_eq!(
                collection.model,
                DEFAULT_MODELS
                    .get()
                    .unwrap()
                    .get(embedder.id())
                    .unwrap()
                    .name
                    .clone(),
            );
            assert_eq!(collection.embedder, embedder.id());
            assert_eq!(collection.provider, *provider);

            let v_collection = vector_db.get_collection(&collection_name).await.unwrap();

            let model = embedder
                .model_details(&collection.model)
                .await
                .unwrap()
                .unwrap();

            assert_eq!(model.size, v_collection.size);
        }
    }

    #[test]
    async fn create_collection_works(state: TestState) {
        let service = &state.app.services.collection;
        let embedder = state
            .app
            .providers
            .embedding
            .get_provider(FEMBED_EMBEDDER_ID)
            .unwrap()
            .clone();

        for provider in state.active_vector_providers.iter() {
            let vector_db = state.app.providers.vector.get_provider(provider).unwrap();

            let name = "Test_collection_0";
            let model = embedder
                .list_embedding_models()
                .await
                .unwrap()
                .first()
                .cloned()
                .unwrap();

            let params = CreateCollectionPayload {
                model: model.name.clone(),
                name: name.to_string(),
                vector_provider: vector_db.id().to_string(),
                embedding_provider: embedder.id().to_string(),
                groups: None,
            };

            let collection = service.create_collection(params).await.unwrap();

            assert_eq!(collection.name, name);
            assert_eq!(collection.model, model.name);
            assert_eq!(collection.embedder, embedder.id());
            assert_eq!(collection.provider, vector_db.id());

            let v_collection = vector_db.get_collection(name).await.unwrap();

            let model = embedder
                .model_details(&collection.model)
                .await
                .unwrap()
                .unwrap();

            assert_eq!(model.size, v_collection.size);
        }
    }

    #[test]
    async fn create_collection_fails_with_invalid_model(state: TestState) {
        let service = &state.app.services.collection;
        let embedder = state
            .app
            .providers
            .embedding
            .get_provider(FEMBED_EMBEDDER_ID)
            .unwrap()
            .clone();

        for provider in state.active_vector_providers.iter() {
            let vector_db = state.app.providers.vector.get_provider(provider).unwrap();

            let name = "Test_collection_0";

            let params = CreateCollectionPayload {
                model: "invalid_model".to_string(),
                name: name.to_string(),
                vector_provider: vector_db.id().to_string(),
                embedding_provider: embedder.id().to_string(),
                groups: None,
            };

            let result = service.create_collection(params).await;

            assert!(result.is_err());
        }
    }

    #[test]
    async fn create_collection_fails_with_existing_collection(state: TestState) {
        let service = &state.app.services.collection;
        let embedder = state
            .app
            .providers
            .embedding
            .get_provider(FEMBED_EMBEDDER_ID)
            .unwrap()
            .clone();

        for provider in state.active_vector_providers.iter() {
            let vector_db = state.app.providers.vector.get_provider(provider).unwrap();
            let collection_name =
                format!("{DEFAULT_COLLECTION_NAME}_{}_{}", provider, embedder.id());

            let params = CreateCollectionPayload {
                model: DEFAULT_MODELS
                    .get()
                    .unwrap()
                    .get(embedder.id())
                    .unwrap()
                    .name
                    .clone(),
                name: collection_name,
                vector_provider: vector_db.id().to_string(),
                embedding_provider: embedder.id().to_string(),
                groups: None,
            };

            let result = service.create_collection(params).await;

            assert!(result.is_err());
        }
    }

    #[test]
    async fn inserting_and_searching_embeddings_works(state: TestState) {
        let services = &state.app.services;
        let postgres = &state.app.providers.database;
        let embedder = state
            .app
            .providers
            .embedding
            .get_provider(FEMBED_EMBEDDER_ID)
            .unwrap()
            .clone();

        for provider in state.active_vector_providers.iter() {
            let vector_db = state.app.providers.vector.get_provider(provider).unwrap();
            let collection_name =
                format!("{DEFAULT_COLLECTION_NAME}_{}_{}", provider, embedder.id());

            let default = services
                .collection
                .get_collection_by_name(&collection_name, vector_db.id())
                .await
                .unwrap();

            let content = "Hello World!";

            let document = services
                .document
                .upload(
                    DocumentUpload::new(
                        "test_document".to_string(),
                        DocumentType::Text(TextDocumentType::Txt),
                        content.as_bytes(),
                    ),
                    false,
                )
                .await
                .unwrap();

            let embeddings = EmbedSingleInput {
                document: document.id,
                collection: default.id,
            };

            let collection = services
                .collection
                .get_collection_by_name(&collection_name, vector_db.id())
                .await
                .unwrap();

            services
                .embedding
                .create_embeddings(embeddings)
                .await
                .unwrap();

            let search = SearchPayload {
                query: content.to_string(),
                collection_id: collection.id,
                limit: Some(1),
                max_distance: None,
            };

            let results = services.collection.search(search).await.unwrap();

            assert_eq!(1, results.items.len());
            assert_eq!(content, results.items[0].item.payload.as_content());

            let embeddings = postgres
                .get_embeddings_by_name(document.id, &collection_name, vector_db.id())
                .await
                .unwrap()
                .unwrap();

            let collection = postgres
                .get_collection_by_id(embeddings.collection_id)
                .await
                .unwrap()
                .unwrap();

            assert_eq!(collection.name, collection_name);
            assert_eq!(document.id, embeddings.document_id);

            services.document.delete(document.id).await.unwrap();
        }
    }

    #[test]
    async fn deleting_collection_removes_all_embeddings(state: TestState) {
        let services = &state.app.services;
        let postgres = &state.app.providers.database;
        let embedder = state
            .app
            .providers
            .embedding
            .get_provider(FEMBED_EMBEDDER_ID)
            .unwrap()
            .clone();

        for provider in state.active_vector_providers.iter() {
            let vector_db = state.app.providers.vector.get_provider(provider).unwrap();

            let collection_name = "Test_collection_delete_embeddings";

            let collection = services
                .collection
                .create_collection(CreateCollectionPayload {
                    name: collection_name.to_string(),
                    model: DEFAULT_MODELS
                        .get()
                        .unwrap()
                        .get(embedder.id())
                        .unwrap()
                        .name
                        .clone(),
                    vector_provider: vector_db.id().to_string(),
                    embedding_provider: embedder.id().to_string(),
                    groups: None,
                })
                .await
                .unwrap();

            let document = services
                .document
                .upload(
                    DocumentUpload::new(
                        "test_document_420".to_string(),
                        DocumentType::Text(TextDocumentType::Txt),
                        b"Hello, world! 420",
                    ),
                    false,
                )
                .await
                .unwrap();

            let embeddings = EmbedSingleInput {
                document: document.id,
                collection: collection.id,
            };

            services
                .embedding
                .create_embeddings(embeddings)
                .await
                .unwrap();

            services
                .collection
                .delete_collection(collection.id)
                .await
                .unwrap();

            let embeddings = postgres
                .get_embeddings(document.id, collection.id)
                .await
                .unwrap();

            assert!(embeddings.is_none());

            services.document.delete(document.id).await.unwrap();
        }
    }

    #[test]
    async fn prevents_duplicate_embeddings(state: TestState) {
        let services = &state.app.services;
        let embedder = state
            .app
            .providers
            .embedding
            .get_provider(FEMBED_EMBEDDER_ID)
            .unwrap()
            .clone();

        for provider in state.active_vector_providers.iter() {
            let vector_db = state.app.providers.vector.get_provider(provider).unwrap();
            let collection_name =
                format!("{DEFAULT_COLLECTION_NAME}_{}_{}", provider, embedder.id());

            let document = services
                .document
                .upload(
                    DocumentUpload::new(
                        "test_document_42069".to_string(),
                        DocumentType::Text(TextDocumentType::Txt),
                        b"Hello, world! 42069",
                    ),
                    false,
                )
                .await
                .unwrap();

            let default = services
                .collection
                .get_collection_by_name(&collection_name, vector_db.id())
                .await
                .unwrap();

            let create = EmbedSingleInput {
                document: document.id,
                collection: default.id,
            };

            services
                .embedding
                .create_embeddings(create.clone())
                .await
                .unwrap();

            let duplicate = services.embedding.create_embeddings(create).await;
            let error = duplicate.unwrap_err().error;

            assert!(matches!(error, ChonkitErr::AlreadyExists(_)));

            services.document.delete(document.id).await.unwrap();
        }
    }
}
