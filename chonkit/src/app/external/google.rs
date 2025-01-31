use chrono::{DateTime, Utc};
use serde::Deserialize;

pub mod auth;
pub mod drive;
pub mod store;

/// Cookie name for the Google access token
pub const GOOGLE_ACCESS_TOKEN_COOKIE: &str = "google_drive_access_token";
/// Header name for the Google access token
pub const GOOGLE_ACCESS_HEADER: &str = "x-google-drive-access-token";

#[derive(Debug, thiserror::Error)]
pub enum GoogleError {
    #[error("{0}")]
    Auth(GoogleAuthError),
    #[error("{0}")]
    Api(GoogleApiError),
    #[error("{0}")]
    Operation(Status),
    #[error("{0}")]
    App(String),
}

#[derive(Debug, Deserialize, thiserror::Error)]
#[error("{error_description}")]
pub struct GoogleAuthError {
    error: String,
    error_description: String,
}

/// Encountered when using the Google Drive API.
#[derive(Debug, Deserialize, thiserror::Error)]
#[error("{error}")]
pub struct GoogleApiError {
    pub error: GoogleApiErrorInner,
}

#[derive(Debug, Deserialize, thiserror::Error)]
#[error("code: {code}, message: {message}, errors: {errors:?}, status: {status:?}")]
pub struct GoogleApiErrorInner {
    pub code: u16,
    pub message: String,
    pub errors: Vec<GoogleErrorDetail>,
    pub status: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct GoogleErrorDetail {
    pub message: String,
    pub domain: String,
    pub reason: String,
}

// DTOs

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ListFilesResponse {
    files: Vec<DriveFile>,
    next_page_token: Option<String>,
    incomplete_search: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DriveFile {
    id: String,
    name: String,
    mime_type: Option<String>,
    capabilities: Option<DriveFileCapabilities>,
    file_extension: Option<String>,
    modified_time: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DriveFileCapabilities {
    can_download: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Operation {
    /// Necessary in case operations come unfinished.
    name: String,
    done: bool,
    #[serde(alias = "error")]
    response: Option<OperationResult>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum OperationResult {
    Error(Status),
    Response(OperationResponse),
}

#[derive(Debug, Deserialize, thiserror::Error)]
#[serde(rename_all = "camelCase")]
#[error("code: {code}, message: {message}, details: {details:?}")]
pub struct Status {
    code: u16,
    message: String,
    details: Vec<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OperationResponse {
    download_uri: String,
}
