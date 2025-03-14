use super::{ChonkitErr, ChonkitError};
use axum::{http::StatusCode, response::IntoResponse, Json};
use serde::Serialize;

impl ChonkitError {
    pub fn status(&self) -> StatusCode {
        use ChonkitErr as E;
        use StatusCode as SC;
        match self.error {
            E::ParseInt(_) => SC::BAD_REQUEST,
            E::AlreadyExists(_) => SC::CONFLICT,
            E::DoesNotExist(_) => SC::NOT_FOUND,
            E::Validation(_)
            | E::Regex(_)
            | E::Chunker(_)
            | E::InvalidFile(_)
            | E::UnsupportedFileType(_)
            | E::InvalidProvider(_)
            | E::InvalidEmbeddingModel(_) => SC::UNPROCESSABLE_ENTITY,
            E::ParsePdf(_)
            | E::DocxRead(_)
            | E::Sqlx(_)
            | E::Http(_)
            | E::IO(_)
            | E::Fmt(_)
            | E::Embedding(_)
            | E::Utf8(_)
            | E::Batch
            | E::Calamine(_)
            | E::Xlsx(_)
            | E::SerdeJson(_) => SC::INTERNAL_SERVER_ERROR,
            E::Axum(_) => SC::INTERNAL_SERVER_ERROR,

            #[cfg(feature = "qdrant")]
            E::Qdrant(_) => SC::INTERNAL_SERVER_ERROR,

            #[cfg(feature = "weaviate")]
            E::Weaviate(_) => SC::BAD_REQUEST,
            E::Uuid(_) => SC::BAD_REQUEST,
            E::Chunks(_) => SC::UNPROCESSABLE_ENTITY,
            E::ParseConfig(_) => SC::UNPROCESSABLE_ENTITY,
            E::Unauthorized => SC::UNAUTHORIZED,
            E::Reqwest(ref e) => e.status().unwrap_or(SC::INTERNAL_SERVER_ERROR),
            E::InvalidParameter(_) => SC::BAD_REQUEST,
            #[cfg(feature = "gdrive")]
            E::GoogleApi(_) => SC::INTERNAL_SERVER_ERROR,
            E::OperationUnsupported(_) => SC::BAD_REQUEST,
            E::InvalidHeader(_) => SC::INTERNAL_SERVER_ERROR,
            E::Cache(_) => SC::BAD_REQUEST,
            E::CachePool(_) => SC::INTERNAL_SERVER_ERROR,
        }
    }
}

/// Error response wrapper.
#[derive(Debug, Serialize)]
struct ResponseError<T: Serialize> {
    error_type: ErrorType,
    body: T,
}

impl<T> ResponseError<T>
where
    T: Serialize,
{
    pub fn new(error_type: ErrorType, body: T) -> Self {
        Self { error_type, body }
    }
}

#[derive(Debug, Serialize)]
enum ErrorType {
    Internal,
    Api,
}

impl<T> IntoResponse for ResponseError<T>
where
    T: Serialize,
{
    fn into_response(self) -> axum::response::Response {
        <Json<ResponseError<T>> as IntoResponse>::into_response(Json(self))
    }
}

impl IntoResponse for ChonkitError {
    fn into_response(self) -> axum::response::Response {
        let status = self.status();

        self.print();

        use ChonkitErr as CE;
        use ErrorType as ET;

        match self.error {
            CE::InvalidProvider(e) => (status, ResponseError::new(ET::Api, e)).into_response(),
            CE::DoesNotExist(e) => (status, ResponseError::new(ET::Api, e)).into_response(),

            CE::SerdeJson(e) => {
                (status, ResponseError::new(ET::Api, e.to_string())).into_response()
            }

            CE::Validation(errors) => (status, ResponseError::new(ET::Api, errors)).into_response(),

            CE::InvalidEmbeddingModel(e) => {
                (status, ResponseError::new(ET::Api, e)).into_response()
            }

            CE::Batch => (
                status,
                ResponseError::new(ET::Internal, "Batch embedding error".to_string()),
            )
                .into_response(),

            // TODO
            CE::IO(_)
            | CE::Regex(_)
            | CE::Embedding(_)
            | CE::UnsupportedFileType(_)
            | CE::Fmt(_)
            | CE::ParseInt(_)
            | CE::Utf8(_)
            | CE::Sqlx(_)
            | CE::InvalidFile(_)
            | CE::InvalidHeader(_)
            | CE::Http(_) => (status, "Internal".to_string()).into_response(),
            CE::ParsePdf(e) => (status, e.to_string()).into_response(),
            CE::DocxRead(e) => (status, e.to_string()).into_response(),
            CE::AlreadyExists(e) => (status, ResponseError::new(ET::Api, e)).into_response(),

            #[cfg(feature = "weaviate")]
            CE::Weaviate(e) => (status, ResponseError::new(ET::Api, e)).into_response(),

            #[cfg(feature = "qdrant")]
            CE::Qdrant(qdrant_client::QdrantError::ResponseError { .. }) => (
                status,
                ResponseError::new(ET::Internal, "qdrant".to_string()),
            )
                .into_response(),

            #[cfg(feature = "qdrant")]
            CE::Qdrant(_) => (status, "qdrant".to_string()).into_response(),

            CE::Axum(_) => (status, "axum".to_string()).into_response(),
            CE::Uuid(_) => (status, "Invalid UUID format").into_response(),
            CE::Chunks(e) => (status, e.to_string()).into_response(),
            CE::Chunker(e) => (status, e.to_string()).into_response(),
            CE::ParseConfig(e) => (status, e.to_string()).into_response(),
            CE::Unauthorized => (status, "Unauthorized".to_string()).into_response(),
            CE::Reqwest(e) => (status, e.to_string()).into_response(),
            CE::InvalidParameter(e) => (status, e).into_response(),
            #[cfg(feature = "gdrive")]
            CE::GoogleApi(e) => (status, e.to_string()).into_response(),
            CE::OperationUnsupported(e) => (status, e).into_response(),
            CE::Calamine(e) => (status, e.to_string()).into_response(),
            CE::Xlsx(e) => (status, e.to_string()).into_response(),
            CE::Cache(e) => (status, e.to_string()).into_response(),
            CE::CachePool(e) => (status, e.to_string()).into_response(),
        }
    }
}
