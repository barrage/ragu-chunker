#[cfg(test)]
#[suitest::suite(integration_tests)]
#[suitest::suite_cfg(sequential = true)]
mod document_service_integration_tests {
    use crate::{
        app::test::{TestState, TestStateConfig, DEFAULT_MODELS},
        core::{
            document::{
                parser::{parse_text, ParseConfig, StringParseConfig},
                DocumentType, TextDocumentType,
            },
            service::{
                collection::dto::CreateCollectionPayload, document::dto::DocumentUpload,
                embedding::EmbedTextInput,
            },
        },
    };

    const TEST_UPLOAD_PATH: &str = "__document_service_test_upload__";
    const TEST_GDRIVE_PATH: &str = "__document_service_test_gdrive_download__";
    const TEST_DOCS_PATH: &str = "test/docs";
    use suitest::{after_all, before_all, cleanup};

    #[before_all]
    async fn setup() -> TestState {
        let _ = tokio::fs::remove_dir_all(TEST_UPLOAD_PATH).await;
        tokio::fs::create_dir(TEST_UPLOAD_PATH).await.unwrap();

        let test_state = TestState::init(TestStateConfig {
            fs_store_path: TEST_UPLOAD_PATH.to_string(),
            _gdrive_download_path: TEST_GDRIVE_PATH.to_string(),
        })
        .await;

        // azure
        // fn default_model(&self) -> (String, usize) {
        //     (String::from("text-embedding-ada-002"), 1536)
        // }

        test_state
    }

    #[cleanup]
    async fn cleanup() {
        let _ = tokio::fs::remove_dir_all(TEST_UPLOAD_PATH).await;
    }

    #[after_all]
    async fn teardown() {
        let _ = tokio::fs::remove_dir_all(TEST_UPLOAD_PATH).await;
    }

    #[test]
    async fn upload_text_happy(state: TestState) {
        let service = state.app.services.document.clone();

        let content = b"Hello world";
        let upload = DocumentUpload {
            name: "UPLOAD_TEST_TXT".to_string(),
            ty: DocumentType::Text(TextDocumentType::Txt),
            file: content,
        };

        let document = service.upload(upload).await.unwrap();

        let config = ParseConfig::default();

        let text_from_bytes = parse_text(
            config.clone(),
            document.ext.as_str().try_into().unwrap(),
            content,
        )
        .unwrap();

        let text_from_store = parse_text(
            config,
            document.ext.try_into().unwrap(),
            &state
                .app
                .providers
                .document
                .get_provider(&document.src)
                .unwrap()
                .read(&document.path)
                .await
                .unwrap(),
        )
        .unwrap();

        assert_eq!(text_from_bytes, text_from_store);

        service.delete(document.id).await.unwrap();

        assert!(tokio::fs::metadata(document.path).await.is_err());
    }

    #[test]
    async fn upload_pdf_happy(state: TestState) {
        let service = state.app.services.document.clone();

        let content = &tokio::fs::read(format!("{TEST_DOCS_PATH}/test.pdf"))
            .await
            .unwrap();
        let upload = DocumentUpload {
            name: "UPLOAD_TEST_PDF".to_string(),
            ty: DocumentType::Pdf,
            file: content,
        };

        let document = service.upload(upload).await.unwrap();

        let config = ParseConfig::default();

        let text_from_bytes = parse_text(
            config.clone(),
            document.ext.as_str().try_into().unwrap(),
            content,
        )
        .unwrap();

        let text_from_store = parse_text(
            config,
            document.ext.try_into().unwrap(),
            &state
                .app
                .providers
                .document
                .get_provider(&document.src)
                .unwrap()
                .read(&document.path)
                .await
                .unwrap(),
        )
        .unwrap();

        assert_eq!(text_from_bytes, text_from_store);

        service.delete(document.id).await.unwrap();

        assert!(tokio::fs::metadata(document.path).await.is_err());
    }

    #[test]
    async fn upload_docx_happy(state: TestState) {
        let service = state.app.services.document.clone();

        let content = &tokio::fs::read(format!("{TEST_DOCS_PATH}/test.docx"))
            .await
            .unwrap();
        let upload = DocumentUpload {
            name: "UPLOAD_TEST_DOCX".to_string(),
            ty: DocumentType::Docx,
            file: content,
        };

        let document = service.upload(upload).await.unwrap();

        let text_from_bytes = parse_text(
            ParseConfig::default(),
            document.ext.as_str().try_into().unwrap(),
            content,
        )
        .unwrap();

        let text_from_store = parse_text(
            ParseConfig::default(),
            document.ext.try_into().unwrap(),
            &state
                .app
                .providers
                .document
                .get_provider(&document.src)
                .unwrap()
                .read(&document.path)
                .await
                .unwrap(),
        )
        .unwrap();

        assert_eq!(text_from_bytes, text_from_store);

        service.delete(document.id).await.unwrap();

        assert!(tokio::fs::metadata(document.path).await.is_err());
    }

    #[test]
    async fn update_parser(state: TestState) {
        let file_service = state.app.services.document.clone();
        let service = state.app.services.document.clone();

        let content = &tokio::fs::read(format!("{TEST_DOCS_PATH}/test.pdf"))
            .await
            .unwrap();

        let upload = DocumentUpload {
            name: "UPLOAD_TEST_PARSER".to_string(),
            ty: DocumentType::Pdf,
            file: content,
        };

        let document = file_service.upload(upload).await.unwrap();

        let config = ParseConfig::String(
            StringParseConfig::new(10, 20)
                .use_range()
                .with_filter("foo"),
        );

        service
            .update_parser(document.id, None, config.clone())
            .await
            .unwrap();

        let parse_config = service
            .get_config(document.id)
            .await
            .unwrap()
            .parse_config
            .unwrap();

        let ParseConfig::String(config) = config else {
            unreachable!();
        };

        let ParseConfig::String(parse_config) = parse_config else {
            panic!("unexpected parse mode")
        };

        assert_eq!(config.start, parse_config.start);
        assert_eq!(config.end, parse_config.end);
        assert_eq!(
            config.filters[0].to_string(),
            parse_config.filters[0].to_string()
        );
        assert_eq!(config.range, parse_config.range);

        file_service.delete(document.id).await.unwrap();

        assert!(tokio::fs::metadata(document.path).await.is_err());
    }

    #[test]
    async fn deleting_document_removes_all_embeddings(state: TestState) {
        let content = &tokio::fs::read(format!("{TEST_DOCS_PATH}/test.pdf"))
            .await
            .unwrap();

        for vector in state.active_vector_providers.iter() {
            for embedder in state.active_embedding_providers.iter() {
                let upload = DocumentUpload {
                    name: "UPLOAD_TEST_PARSER".to_string(),
                    ty: DocumentType::Pdf,
                    file: content,
                };

                let document = state.app.services.document.upload(upload).await.unwrap();

                let vector_db = state.app.providers.vector.get_provider(vector).unwrap();
                let embedder = state
                    .app
                    .providers
                    .embedding
                    .get_provider(embedder)
                    .unwrap();

                let collection_1 = CreateCollectionPayload {
                    name: "DeleteDocumentTestCollection1".to_string(),
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
                };

                let collection_2 = CreateCollectionPayload {
                    name: "DeleteDocumentTestCollection2".to_string(),
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
                };

                let collection_1 = state
                    .app
                    .services
                    .collection
                    .create_collection(collection_1)
                    .await
                    .unwrap();

                let collection_2 = state
                    .app
                    .services
                    .collection
                    .create_collection(collection_2)
                    .await
                    .unwrap();

                let embeddings_1 = EmbedTextInput {
                    document: document.id,
                    collection: collection_1.id,
                };

                let embeddings_2 = EmbedTextInput {
                    document: document.id,
                    collection: collection_2.id,
                };

                let report_1 = state
                    .app
                    .services
                    .embedding
                    .create_text_embeddings(embeddings_1)
                    .await
                    .unwrap();

                assert!(!report_1.report.cache);

                let report_2 = state
                    .app
                    .services
                    .embedding
                    .create_text_embeddings(embeddings_2)
                    .await
                    .unwrap();

                assert!(report_2.report.cache);

                let count = state
                    .app
                    .services
                    .embedding
                    .count_embeddings(collection_1.id, document.id)
                    .await
                    .unwrap();

                assert!(count >= 1);

                let count = state
                    .app
                    .services
                    .embedding
                    .count_embeddings(collection_2.id, document.id)
                    .await
                    .unwrap();

                assert!(count >= 1);

                state
                    .app
                    .services
                    .document
                    .delete(document.id)
                    .await
                    .unwrap();

                let count = state
                    .app
                    .services
                    .embedding
                    .count_embeddings(collection_1.id, document.id)
                    .await
                    .unwrap();

                assert_eq!(0, count);

                let count = state
                    .app
                    .services
                    .embedding
                    .count_embeddings(collection_2.id, document.id)
                    .await
                    .unwrap();

                assert_eq!(0, count);

                let emb_1 = state
                    .app
                    .services
                    .embedding
                    .get_embeddings(document.id, collection_1.id)
                    .await
                    .unwrap();
                assert!(emb_1.is_none());

                let emb_2 = state
                    .app
                    .services
                    .embedding
                    .get_embeddings(document.id, collection_2.id)
                    .await
                    .unwrap();
                assert!(emb_2.is_none());

                // We have to clear the cache here to keep test state fresh per provider
                state.embedding_cache.clear().await.unwrap();
            }
        }
    }
}
