use crate::{
    app::{batch::{BatchJob, JobResult}, server::dto::{ EmbeddingBatchPayload, EmbeddingSinglePayload, ListEmbeddingsPayload, }, state::AppState }, core::{
        chunk::ChunkedDocument, model::{
            collection::{Collection, CollectionDisplay, Embedding},  List, PaginationSort
        }, service::vector::dto::{CreateCollectionPayload, CreateEmbeddings, SearchPayload }
    }, err, error::ChonkitError, map_err
};
use axum::{
    extract::{Path, Query, State}, http::StatusCode, response::{sse::Event, Sse}, Json
};
use futures_util::Stream;
use tokio_stream::StreamExt;
use validify::Validate;
use std::{collections::HashMap, time::Duration};
use uuid::Uuid;

#[utoipa::path(
    get,
    path = "/collections", 
    responses(
        (status = 200, description = "List collections", body = inline(List<Collection>)),
        (status = 500, description = "Internal server error")
    ),
    params(
        ("pagination" = PaginationSort, Query, description = "Pagination parameters")
    )
)]
pub(super) async fn list_collections(
    State(state): State<AppState>,
    payload: Option<Query<PaginationSort>>,
) -> Result<Json<List<Collection>>, ChonkitError> {
    let Query(pagination) = payload.unwrap_or_default();
    let collections = state.services.vector.list_collections(pagination).await?;
    Ok(Json(collections))
}

#[utoipa::path(
    get,
    path = "/display/collections",
    responses(
        (status = 200, description = "List collections with additional info for display purposes.", body = inline(List<CollectionDisplay>)),
        (status = 400, description = "Invalid pagination parameters"),
        (status = 500, description = "Internal server error")
    ),
    params(
        ("pagination" = PaginationSort, Query, description = "Query parameters"),
    ),
)]
pub(super) async fn list_collections_display(
    State(state): State<AppState>,
    payload: Option<Query<PaginationSort>>,
) -> Result<Json<List<CollectionDisplay>>, ChonkitError> {
    let Query(pagination) = payload.unwrap_or_default();
    let collections = state.services.vector.list_collections_display(pagination).await?;
    Ok(Json(collections))
}

#[utoipa::path(
    get,
    path = "/display/collections/{id}",
    responses(
        (status = 200, description = "Get collection by id", body = CollectionDisplay),
        (status = 404, description = "Collection not found"),
        (status = 500, description = "Internal server error")
    ),
    params(
        ("id" = Uuid, Path, description = "Collection ID")        
    ) 
)]
pub(super) async fn collection_display(
    State(state):State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<CollectionDisplay>, ChonkitError> {
    let collection = state.services.vector.get_collection_display(id).await?;
    Ok(Json(collection))
}

#[utoipa::path(
    post,
    path = "/collections", 
    responses(
        (status = 201, description = "Collection created successfully", body = Collection),
        (status = 409, description = "Collection already exists"),
        (status = 500, description = "Internal server error")
    ),
    request_body = CreateCollectionPayload
)]
pub(super) async fn create_collection(
    State(state):State<AppState>,
    Json(payload): Json<CreateCollectionPayload>,
) -> Result<(StatusCode, Json<Collection>), ChonkitError> {
    let collection = state.services.vector
        .create_collection( payload)
        .await?;
    Ok((StatusCode::CREATED, Json(collection)))
}

#[utoipa::path(
    get,
    path = "/collections/{id}", 
    responses(
        (status = 200, description = "Collection retrieved successfully", body = Collection),
        (status = 400, description = "Invalid collection ID format"),
        (status = 404, description = "Collection not found"),
        (status = 500, description = "Internal server error")
    ),
    params(
        ("id" = Uuid, Path, description = "Collection ID")
    )
)]
pub(super) async fn get_collection(
    State(state): State<AppState>,
    Path(id_str): Path<String>,
) -> Result<Json<Collection>, ChonkitError> {
    let collection_id = map_err!(Uuid::parse_str(&id_str));
    let collection = state.services.vector.get_collection(collection_id).await?;
    Ok(Json(collection))
}

#[utoipa::path(
    delete,
    path = "/collections/{id}", 
    responses(
        (status = 204, description = "Collection deleted successfully"),
        (status = 400, description = "Invalid collection ID format"),
        (status = 500, description = "Internal server error")
    ),
    params(
        ("id" = Uuid, Path, description = "Collection ID")
    )
)]
pub(super) async fn delete_collection(
    State(state): State<AppState>,
    Path(id_str): Path<String>,
) -> Result<StatusCode, ChonkitError> {
    let collection_id = map_err!(Uuid::parse_str(&id_str));

    state.services.vector
        .delete_collection(collection_id)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

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
    State(state):State<AppState>,
    Path(provider): Path<String>,
) -> Result<Json<HashMap<String, usize>>, ChonkitError> {
    let models = state.services.vector
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
    State(state):State<AppState>,
    Json(payload): Json<EmbeddingSinglePayload>,
) -> Result<(StatusCode, Json<Embedding>), ChonkitError> {
    let EmbeddingSinglePayload {
        document: document_id,
        collection,
    } = payload;

    let document = state.services.document.get_document(document_id).await?;
    let collection = state.services.vector.get_collection(collection).await?;
    let content = state.services.document.get_content(document_id).await?;

    let chunks = state.services.document
        .get_chunks(&document, &content)
        .await?;

    let chunks = match chunks {
        ChunkedDocument::Ref(r) => r,
        ChunkedDocument::Owned(ref o) => {
            o.iter().map(|s| s.as_str()).collect()
        }
    };

    let create = CreateEmbeddings {
        document_id: document.id,
        collection_id: collection.id,
        chunks: &chunks,
    };

    let embedding = state.services.vector
        .create_embeddings(create)
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
        remove
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
    State(state):State<AppState>,
    Query(payload): Query<ListEmbeddingsPayload>,
) -> Result<Json<List<Embedding>>, ChonkitError> {
    let ListEmbeddingsPayload {
        collection: collection_id,
        pagination,
    } = payload;

    let embeddings = state.services.vector.list_embeddings(pagination.unwrap_or_default(), collection_id).await?;
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
    let embeddings = state.services.vector.list_outdated_embeddings(collection_id).await?;
    Ok(Json(embeddings))
}

#[utoipa::path(
    post,
    path = "/search", 
    responses(
        (status = 200, description = "Search results returned", body = inline(Vec<String>)),
        (status = 500, description = "Internal server error")
    ),
    request_body = SearchPayload
)]
pub(super) async fn search(
    State(state):State<AppState>,
    Json(search): Json<SearchPayload>,
) -> Result<Json<Vec<String>>, ChonkitError> {
    let chunks = state.services.vector.search(search).await?;
    Ok(Json(chunks))
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
    State(state):State<AppState>,
    Path((collection_id, document_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<usize>, ChonkitError> {
    let amount = state.services.vector
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
    state.services.vector
        .delete_embeddings(collection_id, document_id)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
