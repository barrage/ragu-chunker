use crate::{
    app::{
        external::google::{auth::GoogleAccessToken, drive::GoogleDriveApi},
        state::AppState,
    },
    core::{
        model::document::Document,
        service::external::file::{ImportFailure, ImportResult, OutdatedDocument},
    },
    error::ChonkitError,
};
use axum::{
    extract::{Path, State},
    Json,
};
use reqwest::StatusCode;
use serde::Deserialize;
use validify::Validate;

#[utoipa::path(
    post,
    path = "/external/google/documents/import",
    responses(
        (status = 201, description = "Import multiple files from Google Drive", body = ImportResult),
        (status = 500, description = "Internal server error")
    ),
    request_body = ImportPayload
)]
pub(super) async fn import_files(
    State(state): State<AppState>,
    access_token: axum::extract::Extension<GoogleAccessToken>,
    Json(payload): Json<ImportPayload>,
) -> Result<(StatusCode, Json<ImportResult>), ChonkitError> {
    let api = GoogleDriveApi::new(state.http_client.clone(), access_token.0);
    let service = state.services.external.storage(api);
    let result = service.import_documents(payload.files).await?;
    Ok((StatusCode::OK, Json(result)))
}

#[utoipa::path(
    post,
    path = "/external/google/documents/import/{file_id}",
    params(
        ("file_id" = Uuid, Path, description = "External file ID"),
    ),
    responses(
        (status = 201, description = "Import a single file from Google Drive"),
        (status = 500, description = "Internal server error")
    )
)]
pub(super) async fn import_file(
    State(state): State<AppState>,
    access_token: axum::extract::Extension<GoogleAccessToken>,
    Path(file_id): Path<String>,
) -> Result<(StatusCode, Json<Document>), ChonkitError> {
    let api = GoogleDriveApi::new(state.http_client.clone(), access_token.0);
    let service = state.services.external.storage(api);
    let document = service.import_document(&file_id).await?;
    Ok((StatusCode::CREATED, Json(document)))
}

#[utoipa::path(
    get,
    path = "/external/google/documents/outdated",
    responses(
        (status = 200, description = "List documents whose `updated_at` field is less than the external `modifiedTime` field", body = Vec<OutdatedDocument>),
        (status = 500, description = "Internal server error")
    )
)]
pub(super) async fn list_outdated_documents(
    access_token: axum::extract::Extension<GoogleAccessToken>,
    State(state): State<AppState>,
) -> Result<Json<Vec<OutdatedDocument>>, ChonkitError> {
    let api = GoogleDriveApi::new(state.http_client.clone(), access_token.0);
    let service = state.services.external.storage(api);
    let outdated = service.list_outdated_documents().await?;
    Ok(Json(outdated))
}

// DTOs

#[derive(Debug, Deserialize, utoipa::ToSchema, Validate)]
pub(super) struct ImportPayload {
    /// A list of file IDs from Drive.
    #[validate(length(min = 1))]
    #[validate(iter(length(min = 1)))]
    files: Vec<String>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema, Validate)]
pub(super) struct SingleImportPayload {
    #[validate(length(min = 1))]
    file: String,
}

// Open API.

#[derive(utoipa::OpenApi)]
#[openapi(
    paths(import_files, import_file, list_outdated_documents),
    components(schemas(ImportPayload, ImportResult, ImportFailure, OutdatedDocument))
)]
pub(super) struct GDriveApiDoc;
