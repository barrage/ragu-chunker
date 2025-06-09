#[rustfmt::skip]
use super::router::{
    // App config
    __path_app_config,

    // Documents
    document::{
        __path_list_documents,
        __path_list_documents_display,
        __path_get_document,
        __path_delete_document,
        __path_upload_documents,
        __path_chunk_preview,
        __path_parse_preview,
        __path_update_document_config,
        __path_sync,
    },

    // Vectors
    collection::{
        __path_list_collections,
        __path_get_collection,
        __path_create_collection,
        __path_delete_collection,
        __path_search, 
        __path_list_collections_display,
        __path_collection_display,
        __path_sync as __path_collection_sync,
        __path_update_collection_groups
    },

    // Embeddings
    embedding::{
        __path_count_embeddings,
        __path_delete_embeddings,
        __path_list_embedded_documents,
        __path_embed_text,
        __path_batch_embed_text,
        __path_embed_image,
        __path_list_embedding_models,
        __path_list_embedding_reports,
    }
};

use super::dto::{EmbedBatchInput, ListDocumentsPayload, ListEmbeddingsPayload, UploadResult};
use crate::{
    app::{server::{dto::UpdateImageDescription, router::collection::SyncParams}, state::AppConfig},
    core::{
        chunk::{ChunkConfig, SemanticWindowConfig, SlidingWindowConfig, SnappingWindowConfig, SplitlineConfig},
        document::parser::{PageRange, SectionParseConfig, StringParseConfig, TextParseConfig},
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
        app_config,
        // Documents
        list_documents,
        list_documents_display,
        get_document,
        delete_document,
        upload_documents,
        chunk_preview,
        parse_preview,
        update_document_config,
        sync,

        // Collections
        list_collections,
        get_collection,
        create_collection,
        delete_collection,
        list_collections_display,
        collection_display,
        search,
        collection_sync,
        update_collection_groups,

        // Embeddings
        list_embedding_models,
        list_embedded_documents,
        list_embedding_reports,
        embed_text,
        batch_embed_text,
        embed_image,
        delete_embeddings,
        count_embeddings,
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
        TextParseConfig,
        SectionParseConfig,
        StringParseConfig,
        TextParseConfig,
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
