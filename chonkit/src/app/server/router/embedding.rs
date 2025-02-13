use crate::{
    app::{
        batch::{BatchJob, JobResult},
        server::dto::{EmbeddingBatchPayload, EmbeddingSinglePayload, ListEmbeddingsPayload},
        state::AppState,
    },
    core::{
        chunk::ChunkedDocument,
        model::{
            embedding::{
                Embedding, EmbeddingRemovalReportBuilder, EmbeddingReport, EmbeddingReportBuilder,
            },
            List,
        },
        service::embedding::{CreateEmbeddings, CreatedEmbeddings},
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
use chrono::Utc;
use futures_util::Stream;
use std::{collections::HashMap, time::Duration};
use tokio_stream::StreamExt;
use uuid::Uuid;
use validify::Validate;

#[utoipa::path(
    get,
    path = "/embeddings/{provider}/models", 
    responses(
        (status = 200, description = "List available embedding models", body = HashMap<String, usize>),
        (status = 500, description = "Internal server error")
    ),
    params(
        ("provider" = String, Path, description = "Vector database provider"),
    ),
)]
pub(super) async fn list_embedding_models(
    State(state): State<AppState>,
    Path(provider): Path<String>,
) -> Result<Json<HashMap<String, usize>>, ChonkitError> {
    let models = state
        .services
        .embedding
        .list_embedding_models(&provider)
        .await?
        .into_iter()
        .collect::<HashMap<String, usize>>();
    Ok(Json(models))
}

#[utoipa::path(
    post,
    path = "/embeddings", 
    responses(
        (status = 204, description = "Embeddings created successfully"),
        (status = 404, description = "Collection or document not found"),
        (status = 500, description = "Internal server error")
    ),
    request_body = EmbeddingSinglePayload
)]
pub(super) async fn embed(
    State(state): State<AppState>,
    Json(payload): Json<EmbeddingSinglePayload>,
) -> Result<(StatusCode, Json<CreatedEmbeddings>), ChonkitError> {
    let EmbeddingSinglePayload {
        document: document_id,
        collection,
    } = payload;

    let document = state.services.document.get_document(document_id).await?;
    let collection = state.services.collection.get_collection(collection).await?;

    let report = EmbeddingReportBuilder::new(
        document.id,
        document.name.clone(),
        collection.id,
        collection.name.clone(),
    );

    let content = state.services.document.get_content(document_id).await?;

    let chunks = state
        .services
        .document
        .get_chunks(&document, &content)
        .await?;

    let chunks = match chunks {
        ChunkedDocument::Ref(r) => r,
        ChunkedDocument::Owned(ref o) => o.iter().map(|s| s.as_str()).collect(),
    };

    let create = CreateEmbeddings {
        document_id: document.id,
        collection_id: collection.id,
        chunks: &chunks,
    };

    let embedding = state.services.embedding.create_embeddings(create).await?;

    let report = report
        .model_used(collection.model)
        .embedding_provider(collection.embedder.clone())
        .tokens_used(embedding.tokens_used)
        .total_vectors(chunks.len())
        .vector_db(collection.provider)
        .finished_at(Utc::now())
        .build();

    state
        .services
        .embedding
        .store_embedding_report(&report)
        .await?;

    Ok((StatusCode::CREATED, Json(embedding)))
}

#[utoipa::path(
    post,
    path = "/embeddings/batch", 
    responses(
        (status = 200, description = "Embeddings created successfully"),
        (status = 500, description = "Internal server error")
    ),
    request_body = EmbeddingBatchPayload
)]
pub(super) async fn batch_embed(
    State(state): State<AppState>,
    Json(job): Json<EmbeddingBatchPayload>,
) -> Result<Sse<impl Stream<Item = Result<Event, ChonkitError>>>, ChonkitError> {
    map_err!(job.validate());

    let EmbeddingBatchPayload {
        collection,
        add,
        remove,
    } = job;

    let (tx, rx) = tokio::sync::mpsc::channel::<JobResult>(add.len() + remove.len());

    let job = BatchJob::new(collection, add, remove, tx);

    if let Err(e) = state.batch_embedder.send(job).await {
        tracing::error!("Error sending embedding job: {:?}", e.0);
        return err!(Batch);
    };

    let stream = tokio_stream::wrappers::ReceiverStream::new(rx).map(|result| {
        let event = match result {
            JobResult::Ok(report) => match Event::default().json_data(report) {
                Ok(event) => event,
                Err(err) => {
                    tracing::error!("Error serializing embedding report: {err}");
                    let err = format!("error: {err}");
                    Event::default().data(err)
                }
            },
            JobResult::Err(err) => {
                tracing::error!("Received error in batch embedder: {err}");
                let err = format!("error: {err}");
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
        (status = 204, description = "Delete embeddings for a given document in a given collection."),
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
) -> Result<StatusCode, ChonkitError> {
    let collection = state
        .services
        .collection
        .get_collection(collection_id)
        .await?;
    let document = state.services.document.get_document(document_id).await?;

    let report = EmbeddingRemovalReportBuilder::new(
        document.id,
        document.name,
        collection.id,
        collection.name,
    );
    let (_, total_deleted) = state
        .services
        .embedding
        .delete_embeddings(collection_id, document_id)
        .await?;

    let report = report
        .total_vectors_removed(total_deleted)
        .finished_at(Utc::now())
        .build();

    state
        .services
        .embedding
        .store_embedding_removal_report(&report)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    get,
    path = "/collections/{collection_id}/embeddings/reports",
    responses(
        (status = 200, description = "List of embedding reports for a given collection.", body = inline(Vec<EmbeddingReport>)),
        (status = 500, description = "Internal server error")
    ),
    params(
        ("collection_id" = Uuid, Path, description = "Collection ID"),
    ),
)]
pub(super) async fn list_embedding_reports(
    State(state): State<AppState>,
    Path(collection_id): Path<Uuid>,
) -> Result<Json<Vec<EmbeddingReport>>, ChonkitError> {
    let embeddings = state
        .services
        .embedding
        .list_collection_embedding_reports(collection_id)
        .await?;
    Ok(Json(embeddings))
}
