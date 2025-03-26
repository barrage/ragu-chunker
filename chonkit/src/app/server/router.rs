use super::api::ApiDoc;
use crate::{app::state::AppState, error::ChonkitError};
use axum::{
    extract::{DefaultBodyLimit, State},
    http::{HeaderName, HeaderValue, Method},
    response::IntoResponse,
    routing::{delete, get, post, put},
    Json, Router,
};
use reqwest::StatusCode;
use serde::Deserialize;
use std::{str::FromStr, time::Duration};
use tower_http::{
    classify::ServerErrorsFailureClass,
    cors::{AllowCredentials, CorsLayer},
    trace::TraceLayer,
};
use tracing::Span;
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

pub(super) mod collection;
pub(super) mod document;
pub(super) mod embedding;

#[cfg(feature = "gdrive")]
pub(super) mod google;

pub fn router(state: AppState) -> Router {
    let origins = state
        .http_config
        .cors_origins
        .iter()
        .map(|origin| {
            tracing::info!("CORS - Adding {origin} to allowed origins");
            HeaderValue::from_str(origin)
        })
        .map(Result::unwrap);

    let headers = state
        .http_config
        .cors_headers
        .iter()
        .map(|header| {
            tracing::info!("CORS - Adding {header} to allowed headers");
            HeaderName::from_str(header)
        })
        .map(Result::unwrap);

    let cors = CorsLayer::new()
        .allow_origin(tower_http::cors::AllowOrigin::list(origins))
        .allow_headers(tower_http::cors::AllowHeaders::list(headers))
        .allow_credentials(AllowCredentials::yes())
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::DELETE,
            Method::PUT,
            Method::PATCH,
        ]);

    let router = Router::new()
        .route("/info", get(app_config))
        .route("/documents", get(document::list_documents))
        .route("/documents", post(document::upload_documents))
        .route_layer(DefaultBodyLimit::max(100_000_000))
        .route("/documents/:id", get(document::get_document))
        .route("/documents/:id", delete(document::delete_document))
        .route(
            "/documents/:id/config",
            put(document::update_document_config),
        )
        .route(
            "/documents/:id/chunk/preview",
            post(document::chunk_preview),
        )
        .route(
            "/documents/:id/parse/preview",
            post(document::parse_preview),
        )
        .route("/documents/sync/:provider", get(document::sync))
        .route("/collections", get(collection::list_collections))
        .route("/collections", post(collection::create_collection))
        .route("/collections/:id", get(collection::get_collection))
        .route(
            "/collections/:id/groups",
            put(collection::update_collection_groups),
        )
        .route("/collections/:id", delete(collection::delete_collection))
        .route(
            "/collections/:collection_id/documents/:document_id",
            delete(embedding::delete_embeddings),
        )
        .route(
            "/collections/:collection_id/documents/:document_id/count",
            get(embedding::count_embeddings),
        )
        .route("/collections/sync", post(collection::sync))
        .route(
            "/embeddings/reports",
            get(embedding::list_embedding_reports),
        )
        .route("/embeddings", get(embedding::list_embedded_documents))
        .route(
            "/embeddings/:collection_id/outdated",
            get(embedding::list_outdated_embeddings),
        )
        .route("/embeddings", post(embedding::embed))
        .route(
            "/embeddings/:provider/models",
            get(embedding::list_embedding_models),
        )
        .route("/embeddings/batch", post(embedding::batch_embed))
        .route("/search", post(collection::search))
        .route("/documents/display", get(document::list_documents_display))
        .route(
            "/collections/display",
            get(collection::list_collections_display),
        )
        .route(
            "/display/collections/:id",
            get(collection::collection_display),
        )
        .with_state(state.clone());

    #[cfg(feature = "gdrive")]
    let router = {
        let gdrive_router = Router::new()
            .route("/google/documents/import", post(google::import_files))
            .route(
                "/google/documents/import/:file_id",
                post(google::import_file),
            )
            .route(
                "/google/documents/outdated",
                get(google::list_outdated_documents),
            )
            .layer(axum::middleware::from_fn(
                crate::app::server::middleware::extract_google_access_token,
            ))
            .with_state(state.clone());

        router.merge(Router::new().nest("/external", gdrive_router))
    };

    #[cfg(feature = "auth-jwt")]
    let router = router.layer(axum::middleware::from_fn_with_state(
        state.jwt_verifier.clone(),
        crate::app::server::middleware::verify_jwt,
    ));

    router
        .layer(
            TraceLayer::new_for_http()
                .on_request(|req: &axum::http::Request<_>, _span: &Span| {
                    let ctype = req
                        .headers()
                        .get("content-type")
                        .map(|v| v.to_str().unwrap_or("none"))
                        .unwrap_or_else(|| "none");

                    tracing::info!("Processing request | content-type: {ctype}");
                })
                .on_response(
                    |res: &axum::http::Response<_>, latency: Duration, _span: &Span| {
                        let status = res.status();
                        let ctype = res
                            .headers()
                            .get("content-type")
                            .map(|v| v.to_str().unwrap_or("none"))
                            .unwrap_or_else(|| "none");

                        tracing::info!(
                            "Sending response | {status} | {}ms | {ctype}",
                            latency.as_millis()
                        );
                    },
                )
                .on_failure(
                    |error: ServerErrorsFailureClass, _latency: Duration, _span: &Span| {
                        tracing::error!("Error in request: {error}")
                    },
                ),
        )
        .layer(cors)
        // Unprotected at all times
        .merge(
            SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", {
                #[allow(unused_mut)]
                let mut api = ApiDoc::openapi();

                #[cfg(feature = "gdrive")]
                api.merge(google::GDriveApiDoc::openapi());

                api
            }),
        )
        // Has to go last to exclude all the tracing/cors layers
        .route("/_health", get(health_check))
}

async fn health_check() -> impl IntoResponse {
    StatusCode::OK
}

#[derive(Deserialize, utoipa::ToSchema)]
struct Force {
    force: bool,
}

#[utoipa::path(
    get,
    path = "/info",
    responses(
        (status = 200, description = "Get app configuration and available providers", body = AppConfig),
        (status = 500, description = "Internal server error")
    )
)]
async fn app_config(state: State<AppState>) -> Result<impl IntoResponse, ChonkitError> {
    Ok(Json(state.get_configuration().await?))
}
