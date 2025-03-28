#[cfg(test)]
#[suitest::suite(integration_tests)]
#[suitest::suite_cfg(sequential = true)]
mod document_service_integration_tests {
    use crate::{
        app::test::{TestState, TestStateConfig},
        core::{
            document::{
                parser::{Parse, ParseConfig, Parser},
                DocumentType, Docx, Pdf, Text, TextDocumentType,
            },
            service::{
                document::dto::DocumentUpload, embedding::EmbedSingleInput,
                vector::dto::CreateCollectionPayload,
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
            gdrive_download_path: TEST_GDRIVE_PATH.to_string(),
        })
        .await;

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

        let document = service.upload(upload, false).await.unwrap();

        let text_from_bytes = Parser::default().parse(Text(content)).unwrap();
        let text_from_store = service.get_content(document.id).await.unwrap();

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

        let document = service.upload(upload, false).await.unwrap();

        let text_from_bytes = Parser::default().parse(Pdf(content)).unwrap();
        let text_from_store = service.get_content(document.id).await.unwrap();

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

        let document = service.upload(upload, false).await.unwrap();

        let text_from_bytes = Parser::default().parse(Docx(content)).unwrap();
        let text_from_store = service.get_content(document.id).await.unwrap();

        assert_eq!(text_from_bytes, text_from_store);

        service.delete(document.id).await.unwrap();

        assert!(tokio::fs::metadata(document.path).await.is_err());
    }

    #[test]
    async fn upload_overwrite_works(state: TestState) {
        let service = state.app.services.document.clone();

        let content = &tokio::fs::read(format!("{TEST_DOCS_PATH}/test.md"))
            .await
            .unwrap();
        let upload = DocumentUpload {
            name: "UPLOAD_OVERWRITE_TEST".to_string(),
            ty: DocumentType::Text(TextDocumentType::Md),
            file: content,
        };

        let regular = service.upload(upload.clone(), false).await.unwrap();

        let upload = DocumentUpload {
            name: "UPLOAD_OVERWRITE_TEST".to_string(),
            ty: DocumentType::Text(TextDocumentType::Md),
            file: b"foo".as_slice(),
        };

        let forced = service.upload(upload, true).await.unwrap();

        assert_eq!(regular.id, forced.id);
        assert_eq!(regular.path, forced.path);

        let content = service.get_content(forced.id).await.unwrap();

        assert_eq!("foo", content);
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

        let document = file_service.upload(upload, false).await.unwrap();

        let config = ParseConfig::new(10, 20).use_range().with_filter("foo");

        service
            .update_parser(document.id, config.clone())
            .await
            .unwrap();

        let document = service.get_config(document.id).await.unwrap();
        let parse_config = document.parse_config.unwrap();

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

                let document = state
                    .app
                    .services
                    .document
                    .upload(upload, false)
                    .await
                    .unwrap();

                let vector_db = state.app.providers.vector.get_provider(vector).unwrap();
                let embedder = state
                    .app
                    .providers
                    .embedding
                    .get_provider(embedder)
                    .unwrap();

                let collection_1 = CreateCollectionPayload {
                    name: "DeleteDocumentTestCollection1".to_string(),
                    model: embedder.default_model().0,
                    vector_provider: vector_db.id().to_string(),
                    embedding_provider: embedder.id().to_string(),
                };

                let collection_2 = CreateCollectionPayload {
                    name: "DeleteDocumentTestCollection2".to_string(),
                    model: embedder.default_model().0,
                    vector_provider: vector_db.id().to_string(),
                    embedding_provider: embedder.id().to_string(),
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

                let embeddings_1 = EmbedSingleInput {
                    document: document.id,
                    collection: collection_1.id,
                };

                let embeddings_2 = EmbedSingleInput {
                    document: document.id,
                    collection: collection_2.id,
                };

                let report_1 = state
                    .app
                    .services
                    .embedding
                    .create_embeddings(embeddings_1)
                    .await
                    .unwrap();

                assert!(!report_1.cache);

                let report_2 = state
                    .app
                    .services
                    .embedding
                    .create_embeddings(embeddings_2)
                    .await
                    .unwrap();

                assert!(report_2.cache);

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
            }
        }
    }
}
