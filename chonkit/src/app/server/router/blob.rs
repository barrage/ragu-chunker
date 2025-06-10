use crate::{app::state::AppState, error::ChonkitError};
use axum::extract::{Path, State};
use axum::http::header;
use axum::http::{HeaderMap, HeaderValue};
use uuid::Uuid;

#[utoipa::path(
    get,
    path = "/blobs/images/{path}",
    responses(
        (status = 200, description = "Success", body = inline(Vec<u8>)),
        (status = 500, description = "Internal server error")
    ),
    params(
        ("path" = Uuid, Path, description = "Image path")
    )
)]
pub(super) async fn get_image(
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<(HeaderMap, Vec<u8>), ChonkitError> {
    let (image, _) = state.services.document.get_image(id).await?;

    let mut header_map = HeaderMap::new();

    header_map.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(image.image.format.to_mime_type())
            .expect("invalid content-type header"),
    );
    header_map.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&format!("inline; filename={:?}", image.path()))
            .expect("invalid content-disposition header"),
    );

    // TODO: Make streaming
    Ok((header_map, image.image.bytes))
}

#[utoipa::path(
    get,
    path = "/blobs/documents/{document_id}",
    responses(
        (status = 200, description = "Success", body = inline(Vec<u8>)),
        (status = 500, description = "Internal server error")
    ),
    params(
        ("id" = Uuid, Path, description = "Document ID")
    )
)]
pub(super) async fn get_document_bytes(
    Path(document_id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<(HeaderMap, Vec<u8>), ChonkitError> {
    let (document, bytes) = state
        .services
        .document
        .get_document_with_content(document_id)
        .await?;

    let mut header_map = HeaderMap::new();

    header_map.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(document.mime_type()).expect("invalid content-type header"),
    );

    header_map.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&format!("inline; filename={:?}", document.name))
            .expect("invalid content-disposition header"),
    );

    // TODO: Make streaming
    Ok((header_map, bytes))
}
