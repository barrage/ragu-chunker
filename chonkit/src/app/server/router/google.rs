use super::Force;
use crate::{
    app::{
        external::google::{auth::GoogleAccessToken, GOOGLE_ACCESS_TOKEN_COOKIE},
        server::router::HttpConfiguration,
        state::ServiceState,
    },
    core::{
        auth::{OAuthExchangeRequest, OAuthToken},
        model::document::Document,
        service::external::{ImportFailure, ImportResult, OutdatedDocument},
    },
    error::ChonkitError,
    map_err,
};
use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, HeaderValue},
    Form, Json,
};
use axum_macros::debug_handler;
use cookie::CookieBuilder;
use reqwest::StatusCode;
use serde::Deserialize;
use validify::Validate;

#[debug_handler]
#[utoipa::path(
    post,
    path = "/external/google/auth",
    request_body(content = OAuthExchangeRequest, content_type = "x-www-form-urlencoded"),
    responses(
        (status = 200, description = "Exchange code for access token", body = OAuthToken),
        (status = 400, description = "Bad request"),
        (status = 500, description = "Internal server error")
    ),
)]
pub(super) async fn authorize(
    State((services, http_config)): axum::extract::State<(ServiceState, HttpConfiguration)>,
    Form(request): axum::extract::Form<OAuthExchangeRequest>,
) -> Result<(HeaderMap, Json<OAuthToken>), ChonkitError> {
    let api = services.external.google_auth_api();
    let service = services.external.authorization(api);

    let access_token = service.exchange_code(request).await?;

    let cookie = CookieBuilder::new(GOOGLE_ACCESS_TOKEN_COOKIE, &access_token.access_token)
        .secure(true)
        .http_only(true)
        .domain(&*http_config.cookie_domain)
        .build();

    let mut headers = HeaderMap::new();
    headers.insert(
        axum::http::header::SET_COOKIE,
        map_err!(cookie.to_string().parse::<HeaderValue>()),
    );

    Ok((headers, Json(access_token)))
}

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
    access_token: axum::extract::Extension<GoogleAccessToken>,
    State(services): State<ServiceState>,
    force: Option<Query<Force>>,
    Json(payload): Json<ImportPayload>,
) -> Result<(StatusCode, Json<ImportResult>), ChonkitError> {
    let api = services.external.google_drive_api(access_token.0);
    let service = services.external.storage(api);
    let result = service
        .import_documents(payload.files, force.map(|f| f.force).unwrap_or_default())
        .await?;
    Ok((StatusCode::CREATED, Json(result)))
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
    access_token: axum::extract::Extension<GoogleAccessToken>,
    State(services): State<ServiceState>,
    Path(file_id): Path<String>,
    force: Option<Query<Force>>,
) -> Result<(StatusCode, Json<Document>), ChonkitError> {
    let api = services.external.google_drive_api(access_token.0);
    let service = services.external.storage(api);
    let document = service
        .import_document(&file_id, force.map(|f| f.force).unwrap_or_default())
        .await?;
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
    State(services): State<ServiceState>,
) -> Result<Json<Vec<OutdatedDocument>>, ChonkitError> {
    let api = services.external.google_drive_api(access_token.0);
    let service = services.external.storage(api);
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
    paths(authorize, import_files, import_file, list_outdated_documents),
    components(schemas(
        OAuthExchangeRequest,
        OAuthToken,
        ImportPayload,
        ImportResult,
        ImportFailure,
        Force,
        OutdatedDocument,
    ))
)]
pub(super) struct GDriveApiDoc;
