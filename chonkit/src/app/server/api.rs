use super::dto::{EmbedBatchInput, ListDocumentsPayload, ListEmbeddingsPayload, UploadResult};
use crate::{
    app::{server::{dto::UpdateImageDescription, router::collection::SyncParams}, state::AppConfig},
    core::{
        chunk::{ChunkConfig, SemanticWindowConfig, SlidingWindowConfig, SnappingWindowConfig, SplitlineConfig},
        document::parser::{PageRange, SectionParseConfig, StringParseConfig, ParseConfig},
        model::{
            collection::{Collection, CollectionDisplay, CollectionDisplayAggregate, CollectionSearchColumn, CollectionShort}, document::{
                Document, DocumentConfig, DocumentDisplay, DocumentSearchColumn, DocumentShort,
            }, embedding::{
                EmbeddingAdditionReport, EmbeddingReport, EmbeddingReportBase, EmbeddingReportSearchColumn, EmbeddingReportType, ImageEmbeddingAdditionReport, ImageEmbeddingRemovalReport, TextEmbedding, TextEmbeddingAdditionReport, TextEmbeddingRemovalReport
            }, image::ImageModel, List, Pagination, PaginationSort, Search, SortDirection
        },
        service::{
            collection::dto::{CollectionData, CollectionSearchResult, CreateCollectionPayload, SearchPayload, SyncIncompatibilityResolution}, document::dto::{ChunkForPreview, ChunkPreview, ChunkPreviewPayload, ListImagesParameters, ParseOutputPreview, ParsePreview, ParsedDocumentPage, ParsedDocumentSection}, embedding::{EmbedTextInput, ListEmbeddingReportsParams}
        },
        token::TokenCount,
        vector::{CollectionItemPayload, CollectionSearchItem, VectorCollection},
    },
};
use utoipa::OpenApi;

#[derive(OpenApi)]
#[openapi(
    paths(
        // App config
        super::router::app_config,

        // Documents
        super::router::document::list_documents,
        super::router::document::list_documents_display,
        super::router::document::get_document,
        super::router::document::delete_document,
        super::router::document::upload_documents,
        super::router::document::chunk_preview,
        super::router::document::parse_preview,
        super::router::document::update_document_config,
        super::router::document::sync,

        // Images
        super::router::document::list_images,
        super::router::document::delete_image,
        super::router::document::upload_images,
        super::router::document::update_image_description,
        super::router::document::process_document_images,

        // Collections
        super::router::collection::list_collections,
        super::router::collection::get_collection,
        super::router::collection::create_collection,
        super::router::collection::delete_collection,
        super::router::collection::list_collections_display,
        super::router::collection::collection_display,
        super::router::collection::search,
        super::router::collection::sync,
        super::router::collection::update_collection_groups,

        // Embeddings
        super::router::embedding::list_embedding_models,
        super::router::embedding::list_embedded_documents,
        super::router::embedding::list_embedding_reports,
        super::router::embedding::embed_text,
        super::router::embedding::batch_embed_text,
        super::router::embedding::embed_image,
        super::router::embedding::delete_embeddings,
        super::router::embedding::count_embeddings,
    ),
    components(schemas(
        List<Collection>,
        List<Document>,
        List<DocumentDisplay>,
        Pagination,

        // Search 

        DocumentSearchColumn,
        CollectionSearchColumn,
        EmbeddingReportSearchColumn,

        PaginationSort<DocumentSearchColumn>,
        PaginationSort<CollectionSearchColumn>,
        PaginationSort<EmbeddingReportSearchColumn>,

        Search<DocumentSearchColumn>,
        Search<CollectionSearchColumn>,
        Search<EmbeddingReportSearchColumn>,

        Document,
        DocumentConfig,
        UploadResult,

        // Chunk config
        ChunkConfig,
        SlidingWindowConfig,
        SnappingWindowConfig,
        SemanticWindowConfig,
        SemanticWindowConfig,
        SplitlineConfig,

        ChunkPreviewPayload,
        ParseConfig,
        SectionParseConfig,
        StringParseConfig,
        ParseConfig,
        ParsePreview,
        ParseOutputPreview,
        PageRange,
        ParsedDocumentSection,
        ParsedDocumentPage,

        ImageModel,
        ListImagesParameters,
        UpdateImageDescription,

        CreateCollectionPayload,
        CollectionSearchResult,
        CollectionSearchItem,
        CollectionItemPayload,
        CollectionData,
        SyncIncompatibilityResolution,
        SyncParams,
        SearchPayload,
        TextEmbedding,
        Collection,
        VectorCollection,
        AppConfig,
        EmbedBatchInput,
        EmbedTextInput,
        ListEmbeddingsPayload,
        ListDocumentsPayload,
        ChunkForPreview,
        ChunkPreview,
        EmbeddingReport,
        EmbeddingReportType,
        TextEmbeddingAdditionReport,
        ImageEmbeddingAdditionReport,
        TextEmbeddingRemovalReport,
        ImageEmbeddingRemovalReport,
        EmbeddingAdditionReport,
        EmbeddingReportBase,
        TokenCount,
        ListEmbeddingReportsParams,
        
        // Display
        DocumentDisplay,
        DocumentShort,
        CollectionDisplay,
        CollectionDisplayAggregate,
        CollectionShort,
        SortDirection,
    ))
)]
pub struct ApiDoc;
