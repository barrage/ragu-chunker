use crate::{
    app::{
        batch::{BatchJob, BatchJobResult, JobResult},
        server::dto::{EmbedBatchInput, ListEmbeddingsPayload},
        state::AppState,
    },
    core::{
        model::{
            embedding::{
                Embedding, EmbeddingReport, EmbeddingReportAddition, EmbeddingReportRemoval,
            },
            List,
        },
        service::embedding::{EmbedSingleInput, ListEmbeddingReportsParams},
    },
    err,
    error::ChonkitError,
    map_err,
};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{sse::Event, Sse},
    Json,
};
use chonkit_embedders::EmbeddingModel;
use futures_util::Stream;
use std::time::Duration;
use tokio_stream::StreamExt;
use uuid::Uuid;
use validify::Validate;

#[utoipa::path(
    get,
    path = "/embeddings/{provider}/models", 
    responses(
        (status = 200, description = "List available embedding models", body = inline(HashMap<String, usize>)),
        (status = 500, description = "Internal server error")
    ),
    params(
        ("provider" = String, Path, description = "Vector database provider"),
    ),
)]
pub(super) async fn list_embedding_models(
    State(state): State<AppState>,
    Path(provider): Path<String>,
) -> Result<Json<Vec<EmbeddingModel>>, ChonkitError> {
    let models = state
        .services
        .embedding
        .list_embedding_models(&provider)
        .await?;
    Ok(Json(models))
}

#[utoipa::path(
    post,
    path = "/embeddings", 
    responses(
        (status = 200, description = "Embeddings created successfully", body = EmbeddingReportAddition),
        (status = 404, description = "Collection or document not found"),
        (status = 500, description = "Internal server error")
    ),
    request_body = EmbedSingleInput
)]
pub(super) async fn embed(
    State(state): State<AppState>,
    Json(input): Json<EmbedSingleInput>,
) -> Result<(StatusCode, Json<EmbeddingReportAddition>), ChonkitError> {
    let report = state.services.embedding.create_embeddings(input).await?;
    Ok((StatusCode::OK, Json(report)))
}

#[utoipa::path(
    post,
    path = "/embeddings/batch", 
    responses(
        (status = 200, description = "Embeddings created successfully"),
        (status = 500, description = "Internal server error")
    ),
    request_body = EmbedBatchInput
)]
pub(super) async fn batch_embed(
    State(state): State<AppState>,
    Json(input): Json<EmbedBatchInput>,
) -> Result<Sse<impl Stream<Item = Result<Event, ChonkitError>>>, ChonkitError> {
    map_err!(input.validate());

    let EmbedBatchInput {
        collection,
        add,
        remove,
    } = input;

    let (tx, rx) = tokio::sync::mpsc::channel::<BatchJobResult>(add.len() + remove.len() + 1);

    let job = BatchJob::new(collection, add, remove, tx);

    if let Err(e) = state.batch_embedder.send(job).await {
        tracing::error!("Error sending embedding job: {:?}", e.0);
        return err!(Batch);
    };

    let stream = tokio_stream::wrappers::ReceiverStream::new(rx)
        .take_while(|result| matches!(result, BatchJobResult::Event(_)))
        .map(|result| {
            let BatchJobResult::Event(result) = result else {
                unreachable!()
            };
            tracing::debug!("sse received event");
            let event = match result.result {
                JobResult::Ok(report) => match Event::default().json_data(report) {
                    Ok(event) => event,
                    Err(err) => {
                        tracing::error!("Error serializing embedding report: {err}");
                        let err = format!("error: {err}").replace('\n', " ");
                        Event::default().data(err)
                    }
                },
                JobResult::Err(err) => {
                    tracing::error!("Received error in batch embedder: {err}");
                    let err = format!("error: {err}").replace('\n', " ");
                    Event::default().data(err)
                }
            };
            Ok(event)
        });

    Ok(Sse::new(stream).keep_alive(
        axum::response::sse::KeepAlive::new()
            .interval(Duration::from_secs(1))
            .text("keep-alive"),
    ))
}

#[utoipa::path(
    get,
    path = "/embeddings", 
    responses(
        (status = 200, description = "List of embedded documents, optionally filtered by collection ID", body = inline(List<Embedding>)),
        (status = 500, description = "Internal server error")
    ),
    params(
        ("payload" = ListEmbeddingsPayload, Query, description = "List parameters"),
    ),
)]
pub(super) async fn list_embedded_documents(
    State(state): State<AppState>,
    Query(payload): Query<ListEmbeddingsPayload>,
) -> Result<Json<List<Embedding>>, ChonkitError> {
    let ListEmbeddingsPayload {
        collection: collection_id,
        pagination,
    } = payload;

    let embeddings = state
        .services
        .embedding
        .list_embeddings(pagination.unwrap_or_default(), collection_id)
        .await?;
    Ok(Json(embeddings))
}

#[utoipa::path(
    get,
    path = "/collections/{collection_id}/outdated", 
    responses(
        (status = 200, description = "List of all embeddings whose `created_at` field is less than their respective document's `updated_at` field", body = inline(Vec<Embedding>)),
        (status = 500, description = "Internal server error")
    ),
)]
pub(super) async fn list_outdated_embeddings(
    State(state): State<AppState>,
    Path(collection_id): Path<Uuid>,
) -> Result<Json<Vec<Embedding>>, ChonkitError> {
    let embeddings = state
        .services
        .embedding
        .list_outdated_embeddings(collection_id)
        .await?;
    Ok(Json(embeddings))
}

#[utoipa::path(
    get,
    path = "/collections/{collection_id}/documents/{document_id}/count",
    responses(
        (status = 200, description = "Count of embeddings for a given document in a given collection.", body = Json<usize>),
        (status = 500, description = "Internal server error")
    ),
    params(
        ("collection_id" = Uuid, Path, description = "Collection ID"),
        ("document_id" = Uuid, Path, description = "Document ID"),
    ),
)]
pub(super) async fn count_embeddings(
    State(state): State<AppState>,
    Path((collection_id, document_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<usize>, ChonkitError> {
    let amount = state
        .services
        .embedding
        .count_embeddings(collection_id, document_id)
        .await?;
    Ok(Json(amount))
}

#[utoipa::path(
    delete,
    path = "/collections/{collection_id}/documents/{document_id}",
    responses(
        (status = 200, description = "Delete embeddings for a given document in a given collection.", body = EmbeddingReportRemoval),
        (status = 500, description = "Internal server error")
    ),
    params(
        ("collection_id" = Uuid, Path, description = "Collection ID"),
        ("document_id" = Uuid, Path, description = "Document ID"),
    ),
)]
pub(super) async fn delete_embeddings(
    State(state): State<AppState>,
    Path((collection_id, document_id)): Path<(Uuid, Uuid)>,
) -> Result<(StatusCode, Json<EmbeddingReportRemoval>), ChonkitError> {
    let report = state
        .services
        .embedding
        .delete_embeddings(collection_id, document_id)
        .await?;

    Ok((StatusCode::OK, Json(report)))
}

#[utoipa::path(
    get,
    path = "/embeddings/reports",
    responses(
        (status = 200, description = "List of embedding reports", body = inline(Vec<EmbeddingReport>)),
        (status = 500, description = "Internal server error")
    ),
    params(
        ("parameters" = Option<ListEmbeddingReportsParams>, Query, description = "Parameters to filter and limit results by"),
    ),
)]
pub(super) async fn list_embedding_reports(
    State(state): State<AppState>,
    params: Option<Query<ListEmbeddingReportsParams>>,
) -> Result<Json<Vec<EmbeddingReport>>, ChonkitError> {
    let params = params.map(|params| params.0).unwrap_or_default();
    let embeddings = state
        .services
        .embedding
        .list_collection_embedding_reports(params)
        .await?;
    Ok(Json(embeddings))
}
