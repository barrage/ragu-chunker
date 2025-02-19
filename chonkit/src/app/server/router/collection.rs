use crate::{
    app::  state::AppState , core::{
         model::{
            collection::{Collection, CollectionDisplay, CollectionSearchColumn},  List, PaginationSort
        }, service:: vector::dto::{CreateCollectionPayload, SearchPayload }
    },  error::ChonkitError, map_err
};
use axum::{
    extract::{Path, Query, State}, http::StatusCode,  Json
};
use uuid::Uuid;

#[utoipa::path(
    get,
    path = "/collections", 
    responses(
        (status = 200, description = "List collections", body = inline(List<Collection>)),
        (status = 500, description = "Internal server error")
    ),
    params(
        ("pagination" = Option<PaginationSort>, Query, description = "Pagination parameters")
    )
)]
pub(super) async fn list_collections(
    State(state): State<AppState>,
    params: Option<Query<PaginationSort<CollectionSearchColumn>>>,
) -> Result<Json<List<Collection>>, ChonkitError> {
    let Query(params) = params.unwrap_or_default();
    let collections = state.services.collection.list_collections(params).await?;
    Ok(Json(collections))
}

#[utoipa::path(
    get,
    path = "/collections/display",
    responses(
        (status = 200, description = "List collections with additional info for display purposes.", body = inline(List<CollectionDisplay>)),
        (status = 400, description = "Invalid pagination parameters"),
        (status = 500, description = "Internal server error")
    ),
    params(
        ("pagination" = Option<PaginationSort>, Query, description = "Query parameters"),
    ),
)]
pub(super) async fn list_collections_display(
    State(state): State<AppState>,
    payload: Option<Query<PaginationSort<CollectionSearchColumn>>>,
) -> Result<Json<List<CollectionDisplay>>, ChonkitError> {
    let Query(pagination) = payload.unwrap_or_default();
    let collections = state.services.collection.list_collections_display(pagination).await?;
    Ok(Json(collections))
}

#[utoipa::path(
    get,
    path = "/collections/{id}/display",
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
    let collection = state.services.collection.get_collection_display(id).await?;
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
    let collection = state.services.collection
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
    let collection = state.services.collection.get_collection(collection_id).await?;
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

    state.services.collection
        .delete_collection(collection_id)
        .await?;
    Ok(StatusCode::NO_CONTENT)
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
    State(state): State<AppState>,
    Json(search): Json<SearchPayload>,
) -> Result<Json<Vec<String>>, ChonkitError> {
    let chunks = state.services.collection.search(search).await?;
    Ok(Json(chunks))
}
