use super::api::ApiDoc;
use crate::{app::state::AppState, error::ChonkitError};
use axum::{
    extract::{DefaultBodyLimit, State},
    http::{HeaderValue, Method},
    response::IntoResponse,
    routing::{delete, get, post, put},
    Json, Router,
};
use std::time::Duration;
use tower_http::{classify::ServerErrorsFailureClass, cors::CorsLayer, trace::TraceLayer};
use tracing::Span;
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

pub(super) mod document;
pub(super) mod vector;

pub fn router(state: AppState, origins: Vec<String>) -> Router {
    let origins = origins
        .into_iter()
        .map(|origin| {
            tracing::debug!("Adding {origin} to allowed origins");
            HeaderValue::from_str(&origin)
        })
        .map(Result::unwrap);

    let cors = CorsLayer::new()
        .allow_origin(tower_http::cors::AllowOrigin::list(origins))
        .allow_headers(tower_http::cors::Any)
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::DELETE,
            Method::PUT,
            Method::PATCH,
        ]);

    let app_state_router = Router::new()
        .route("/_health", get(health_check))
        .route("/info", get(app_config))
        .with_state(state.clone());

    let sync = Router::new()
        .route("/documents/sync/:provider", get(document::sync))
        .with_state(state.clone());

    service_api(state)
        .merge(sync)
        .merge(app_state_router)
        .merge(SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", ApiDoc::openapi()))
        .layer(TraceLayer::new_for_http().on_failure(
            |error: ServerErrorsFailureClass, _latency: Duration, _span: &Span| {
                tracing::error!("{error}")
            },
        ))
        .layer(cors)
}

fn service_api(state: AppState) -> Router {
    use document::*;
    use vector::*;

    let batch_router = Router::new()
        .route("/embeddings/batch", post(batch_embed))
        .with_state(state.batch_embedder);

    Router::new()
        .route("/documents", get(list_documents))
        .route("/documents", post(upload_documents))
        .layer(DefaultBodyLimit::max(50_000_000))
        .route("/documents/:id", get(get_document))
        .route("/documents/:id", delete(delete_document))
        .route("/documents/:id/config", put(update_document_config))
        .route("/documents/:id/chunk/preview", post(chunk_preview))
        .route("/documents/:id/parse/preview", post(parse_preview))
        .route("/collections", get(list_collections))
        .route("/collections", post(create_collection))
        .route("/collections/:id", get(get_collection))
        .route("/collections/:id", delete(delete_collection))
        .route(
            "/collections/:collection_id/documents/:document_id",
            delete(delete_embeddings),
        )
        .route(
            "/collections/:collection_id/documents/:document_id/count",
            get(count_embeddings),
        )
        .route("/embeddings", get(list_embedded_documents))
        .route("/embeddings", post(embed))
        .route("/embeddings/:provider/models", get(list_embedding_models))
        .route("/search", post(search))
        .route("/display/documents", get(list_documents_display))
        .route("/display/collections", get(list_collections_display))
        .route("/display/collections/:id", get(collection_display))
        .with_state(state.services)
        .merge(batch_router)
}

async fn health_check() -> impl IntoResponse {
    "OK"
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
