use crate::{
    app::{
        server::dto::{ConfigUpdatePayload, ListDocumentsPayload, UploadResult},
        state::AppState,
    },
    core::{
        document::{parser::ParseConfig, DocumentType},
        model::{
            document::{Document, DocumentConfig, DocumentDisplay},
            List,
        },
        service::document::dto::{ChunkPreview, ChunkPreviewPayload, DocumentUpload},
    },
    error::ChonkitError,
};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use std::collections::HashMap;
use uuid::Uuid;

use super::Force;

#[utoipa::path(
    get,
    path = "/documents",
    responses(
        (status = 200, description = "List documents", body = inline(List<Document>)),
        (status = 400, description = "Invalid pagination parameters"),
        (status = 500, description = "Internal server error")
    ),
    params(
        ("pagination" = ListDocumentsPayload, Query, description = "Query parameters"),
    ),
)]
pub(super) async fn list_documents(
    State(state): State<AppState>,
    params: Option<Query<ListDocumentsPayload>>,
) -> Result<Json<List<Document>>, ChonkitError> {
    let Query(params) = params.unwrap_or_default();

    let documents = state
        .services
        .document
        .list_documents(params.pagination, params.src.as_deref(), params.ready)
        .await?;

    Ok(Json(documents))
}

#[utoipa::path(
    get,
    path = "/documents/display",
    responses(
        (status = 200, description = "List documents with additional info for display purposes.", body = inline(List<DocumentDisplay>)),
        (status = 400, description = "Invalid pagination parameters"),
        (status = 500, description = "Internal server error")
    ),
    params(
        ("pagination" = ListDocumentsPayload, Query, description = "Query parameters"),
    ),
)]
pub(super) async fn list_documents_display(
    State(state): State<AppState>,
    payload: Option<Query<ListDocumentsPayload>>,
) -> Result<Json<List<DocumentDisplay>>, ChonkitError> {
    let Query(payload) = payload.unwrap_or_default();

    let documents = state
        .services
        .document
        .list_documents_display(payload.pagination, payload.src.as_deref())
        .await?;

    Ok(Json(documents))
}

#[utoipa::path(
    get,
    path = "/documents/{id}",
    responses(
        (status = 200, description = "Get document by id", body = Document),
        (status = 404, description = "Document not found"),
        (status = 500, description = "Internal server error")
    ),
    params(
        ("id" = Uuid, Path, description = "Document ID")
    )
)]
pub(super) async fn get_document(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<DocumentConfig>, ChonkitError> {
    let document = state.services.document.get_config(id).await?;
    Ok(Json(document))
}

#[utoipa::path(
    delete,
    path = "/documents/{id}",
    responses(
        (status = 204, description = "Delete document by id"),
        (status = 404, description = "Document not found"),
        (status = 500, description = "Internal server error")
    ),
    params(
        ("id" = Uuid, Path, description = "Document ID")
    )
)]
pub(super) async fn delete_document(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ChonkitError> {
    state.services.document.delete(id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    post,
    path = "/documents",
    request_body(content = String, content_type = "multipart/form-data"),
    responses(
        (status = 200, description = "Upload documents", body = UploadResult),
        (status = 400, description = "Bad request"),
        (status = 500, description = "Internal server error")
    ),
)]
pub(super) async fn upload_documents(
    State(state): State<AppState>,
    force: Option<Query<Force>>,
    mut form: axum::extract::Multipart,
) -> Result<Json<UploadResult>, ChonkitError> {
    let mut documents = vec![];
    let force = force.map(|f| f.force).unwrap_or_default();
    let mut errors = HashMap::<String, Vec<String>>::new();

    while let Ok(Some(field)) = form.next_field().await {
        let Some(name) = field.file_name() else {
            continue;
        };

        let name = name.to_string();

        let file = match field.bytes().await {
            Ok(bytes) => bytes,
            Err(e) => {
                tracing::error!("error in form: {e}");
                errors
                    .entry(name)
                    .and_modify(|entry| entry.push(e.to_string()))
                    .or_insert_with(|| vec![e.to_string()]);
                continue;
            }
        };

        let typ = match DocumentType::try_from_file_name(&name) {
            Ok(ty) => ty,
            Err(e) => {
                tracing::error!("{e}");
                errors
                    .entry(name)
                    .and_modify(|entry| entry.push(e.to_string()))
                    .or_insert_with(|| vec![e.to_string()]);
                continue;
            }
        };

        let upload = DocumentUpload::new(name.to_string(), typ, &file);

        let document = match state.services.document.upload(upload, force).await {
            Ok(doc) => doc,
            Err(e) => {
                tracing::error!("{e}");
                errors
                    .entry(name)
                    .and_modify(|entry| entry.push(e.to_string()))
                    .or_insert_with(|| vec![e.to_string()]);
                continue;
            }
        };

        documents.push(document);
    }

    Ok(Json(UploadResult { documents, errors }))
}

#[utoipa::path(
    put,
    path = "/documents/{id}/config",
    responses(
        (status = 204, description = "Update parsing and chunking configuration", body = String),
        (status = 404, description = "Document not found"),
        (status = 500, description = "Internal server error")
    ),
    params(
        ("id" = Uuid, Path, description = "Document ID"),
    ),
    request_body = ConfigUpdatePayload
)]
pub(super) async fn update_document_config(
    State(state): State<AppState>,
    Path(document_id): Path<Uuid>,
    Json(config): Json<ConfigUpdatePayload>,
) -> Result<StatusCode, ChonkitError> {
    let ConfigUpdatePayload { parser, chunker } = config;

    if let Some(parser) = parser {
        state
            .services
            .document
            .update_parser(document_id, parser)
            .await?;
    }

    if let Some(chunker) = chunker {
        state
            .services
            .document
            .update_chunker(document_id, chunker)
            .await?;
    }

    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    post,
    path = "/documents/{id}/chunk/preview",
    responses(
        (status = 200, description = "Preview document chunks", body = inline(Vec<ChunkPreview>)),
        (status = 404, description = "Document not found"),
        (status = 500, description = "Internal server error")
    ),
    params(
        ("id" = Uuid, Path, description = "Document ID"),
    ),
    request_body = ChunkPreviewPayload
)]
pub(super) async fn chunk_preview(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(config): Json<ChunkPreviewPayload>,
) -> Result<Json<ChunkPreview>, ChonkitError> {
    let chunks = state.services.document.chunk_preview(id, config).await?;
    Ok(Json(chunks))
}

#[utoipa::path(
    post,
    path = "/documents/{id}/parse/preview",
    responses(
        (status = 200, description = "Preview parsed document", body = String),
        (status = 404, description = "Document not found"),
        (status = 500, description = "Internal server error")
    ),
    params(
        ("id" = Uuid, Path, description = "Document ID")
    ),
    request_body(content = ParseConfig, description = "Optional parse configuration for preview")
)]
pub(super) async fn parse_preview(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(config): Json<ParseConfig>,
) -> Result<Json<String>, ChonkitError> {
    let parsed = state.services.document.parse_document(id, config).await?;
    Ok(Json(parsed))
}

#[utoipa::path(
    get,
    path = "/documents/sync/{provider}", 
    responses(
        (status = 204, description = "Successfully synced", body = String),
        (status = 500, description = "Internal server error")
    ),
    params(
        ("id" = String, Path, description = "Storage provider")
    ),
)]
pub(super) async fn sync(
    State(state): State<AppState>,
    Path(provider): Path<String>,
) -> Result<StatusCode, ChonkitError> {
    state.services.document.sync(&provider).await?;
    Ok(StatusCode::NO_CONTENT)
}
