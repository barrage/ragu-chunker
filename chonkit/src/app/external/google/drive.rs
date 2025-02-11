use super::auth::GoogleAccessToken;
use crate::{
    app::external::google::{
        DriveFile, GoogleApiError, GoogleError, ListFilesResponse, Operation, OperationResult,
    },
    config::GOOGLE_STORE_ID,
    core::{
        document::store::{external::ExternalDocumentStorage, DocumentFile, ExternalPath},
        document::DocumentType,
        provider::Identity,
    },
    err,
    error::{ChonkitErr, ChonkitError},
    map_err,
};

const FILES_EP: &str = "https://www.googleapis.com/drive/v3/files";
const OPERATIONS_EP: &str = "https://www.googleapis.com/drive/v3/operations";

const LIST_FILES_FIELDS: &str = "incompleteSearch,nextPageToken,files(id,name,mimeType,capabilities(canDownload),fileExtension,modifiedTime,sha256Checksum,md5Checksum,sha1Checksum)";
const GET_FILE_FIELDS: &str =
    "id,name,mimeType,capabilities(canDownload),fileExtension,modifiedTime,sha256Checksum,md5Checksum,sha1Checksum";

/// Google Drive API client.
/// [Identity] implementation corresponds to [GoogleDriveStore][super::store::GoogleDriveStore].
#[derive(Debug, Clone)]
pub struct GoogleDriveApi {
    /// Must already be in the form of `Bearer <JWT>`.
    token: String,
    client: reqwest::Client,
}

impl GoogleDriveApi {
    pub fn new(client: reqwest::Client, token: GoogleAccessToken) -> Self {
        Self {
            token: token.0,
            client,
        }
    }

    async fn list_all_drive_files(&self) -> Result<Vec<DriveFile>, ChonkitError> {
        let mut files = vec![];
        let mut next_page_token: Option<String> = None;
        let mut requests = 0;

        tracing::debug!("Google Drive listing all files");

        loop {
            let query: &[(&str, &str)] = if let Some(ref npt) = next_page_token {
                &[
                    ("fields", LIST_FILES_FIELDS),
                    ("pageSize", "1000"),
                    ("pageToken", npt),
                ]
            } else {
                &[("fields", LIST_FILES_FIELDS), ("pageSize", "1000")]
            };

            let response = map_err!(
                self.client
                    .get(FILES_EP)
                    .header("Authorization", &self.token)
                    .query(query)
                    .send()
                    .await
            );

            requests += 1;

            if !response.status().is_success() {
                let response: GoogleApiError = map_err!(response.json().await);
                tracing::error!("{response}");
                return err!(ChonkitErr::GoogleApi(GoogleError::Api(response)));
            }

            let response: ListFilesResponse = map_err!(response.json().await);

            files.extend(response.files);

            if let Some(npt) = response.next_page_token {
                tracing::debug!("Continuing listing, issued requests: {requests}");
                next_page_token = Some(npt);
                continue;
            }

            if !response.incomplete_search {
                break;
            }

            tracing::debug!("Continuing listing, issued requests: {requests}");
        }

        Ok(files)
    }

    async fn get_drive_file(&self, file_id: &str) -> Result<DriveFile, ChonkitError> {
        let response = map_err!(
            self.client
                .get(format!("{FILES_EP}/{file_id}"))
                .header("Authorization", &self.token)
                .query(&[("fields", GET_FILE_FIELDS)])
                .send()
                .await
        );

        if !response.status().is_success() {
            let response = map_err!(response.text().await);
            return err!(ChonkitErr::GoogleApi(GoogleError::App(response)));
        }

        let file = map_err!(response.json().await);

        Ok(file)
    }

    /// Reads a file from Google Drive using the provided bearer token.
    /// The token must already be in the form `Bearer <token>`.
    async fn download_drive_file(&self, drive_file_id: &str) -> Result<Vec<u8>, ChonkitError> {
        let response = map_err!(
            self.client
                .get(format!("{FILES_EP}/{drive_file_id}"))
                .header("Authorization", &self.token)
                .query(&[("fields", GET_FILE_FIELDS)])
                .send()
                .await
        );

        if !response.status().is_success() {
            let response = map_err!(response.text().await);
            return err!(ChonkitErr::GoogleApi(GoogleError::App(response)));
        }

        let file = map_err!(response.json::<DriveFile>().await);

        let Some(ref capabilities) = file.capabilities else {
            return err!(ChonkitErr::GoogleApi(GoogleError::App(format!(
                "File '{}' has no capabilities",
                file.name
            ))));
        };

        // Special case for string files since they throw 500 errors
        // for some reason. These can usually be downloaded with `?alt=media`.
        if let Some(ext) = file.file_extension {
            if !is_google_binary(DocumentType::try_from(ext)?) {
                let response = map_err!(
                    self.client
                        .get(format!("{FILES_EP}/{drive_file_id}"))
                        .query(&[("alt", "media")])
                        .header("Authorization", &self.token)
                        .send()
                        .await
                );

                if !response.status().is_success() {
                    let response = map_err!(response.json().await);
                    return err!(ChonkitErr::GoogleApi(GoogleError::Api(response)));
                }

                let content = map_err!(response.bytes().await);
                return Ok(content.to_vec());
            }
        }

        if !capabilities.can_download {
            return err!(ChonkitErr::GoogleApi(GoogleError::App(format!(
                "File '{}' cannot be downloaded",
                file.name
            ))));
        }

        let response = map_err!(
            self.client
                .post(format!("{FILES_EP}/{drive_file_id}/download"))
                .header("Authorization", &self.token)
                .header("content-length", 0)
                .send()
                .await
        );

        if !response.status().is_success() {
            let response = map_err!(response.json().await);
            return err!(ChonkitErr::GoogleApi(GoogleError::Api(response)));
        }

        let operation = map_err!(response.json::<Operation>().await);

        if operation.done {
            let Some(response) = operation.response else {
                return err!(ChonkitErr::GoogleApi(GoogleError::App(format!(
                    "Operation '{}' resulted in null response",
                    operation.name
                ))));
            };

            match response {
                OperationResult::Error(e) => {
                    err!(ChonkitErr::GoogleApi(GoogleError::Operation(e)))
                }
                OperationResult::Response(res) => {
                    let response = map_err!(
                        self.client
                            .get(&res.download_uri)
                            .header("Authorization", &self.token)
                            .send()
                            .await
                    );

                    let content = map_err!(response.bytes().await);
                    Ok(content.to_vec())
                }
            }
        } else {
            tracing::debug!(
                "Operation '{}' incomplete, commencing download loop",
                operation.name
            );
            self.operation_download_loop(operation).await
        }
    }

    async fn operation_download_loop(
        &self,
        mut operation: Operation,
    ) -> Result<Vec<u8>, ChonkitError> {
        const MAX_ATTEMPTS: u64 = 10;
        let mut attempts: u64 = 0;
        tracing::info!("Starting download loop for operation {}", operation.name);

        while !operation.done && attempts < MAX_ATTEMPTS {
            attempts += 1;

            // If the operation is not done, wait a bit and try again
            tokio::time::sleep(std::time::Duration::from_millis(200 * attempts)).await;

            tracing::info!(
                "Polling operation {} for result (attempt {attempts}/{MAX_ATTEMPTS})",
                operation.name
            );

            let response = map_err!(
                self.client
                    .get(format!("{OPERATIONS_EP}/{}", operation.name))
                    .header("Authorization", &self.token)
                    .send()
                    .await
            );

            if !response.status().is_success() {
                let response = map_err!(response.json().await);
                return err!(ChonkitErr::GoogleApi(GoogleError::Api(response)));
            }

            operation = map_err!(response.json::<Operation>().await);
        }

        let Some(response) = operation.response else {
            return err!(ChonkitErr::GoogleApi(GoogleError::App(format!(
                "Operation '{}' resulted in null response",
                operation.name
            ))));
        };

        match response {
            OperationResult::Error(e) => {
                err!(ChonkitErr::GoogleApi(GoogleError::Operation(e)))
            }
            OperationResult::Response(res) => {
                let response = map_err!(
                    self.client
                        .get(&res.download_uri)
                        .header("Authorization", &self.token)
                        .send()
                        .await
                );

                let content = map_err!(response.bytes().await);

                Ok(content.to_vec())
            }
        }
    }
}

impl Identity for GoogleDriveApi {
    fn id(&self) -> &'static str {
        GOOGLE_STORE_ID
    }
}

#[async_trait::async_trait]
impl ExternalDocumentStorage for GoogleDriveApi {
    async fn list_files(
        &self,
        file_ids: Option<&[String]>,
    ) -> Result<Vec<DocumentFile<ExternalPath>>, ChonkitError> {
        let files = self
            .list_all_drive_files()
            .await?
            .into_iter()
            .filter_map(|file| {
                let Some(ref ext) = file.file_extension.or(file.mime_type) else {
                    tracing::warn!("File {} does not have an extension, skipping", file.name);
                    return None;
                };

                let ext = DocumentType::try_from(ext.as_str()).ok()?;

                // Skip files that are not in the provided list
                if let Some(file_ids) = file_ids {
                    if !file_ids.contains(&file.id) {
                        return None;
                    }
                }

                Some(DocumentFile::new(
                    file.name,
                    ext,
                    ExternalPath(file.id),
                    file.modified_time,
                ))
            })
            .collect();

        Ok(files)
    }

    async fn get_file(&self, file_id: &str) -> Result<DocumentFile<ExternalPath>, ChonkitError> {
        let file = self.get_drive_file(file_id).await?;

        let Some(ext) = file.file_extension.or(file.mime_type) else {
            tracing::warn!("File {} does not have an extension, skipping", file.name);
            return err!(InvalidFile, "File does not contain a readable extension");
        };

        let ext = match DocumentType::try_from(ext) {
            Ok(ext) => ext,
            Err(e) => {
                tracing::warn!("{e}");
                return Err(e);
            }
        };

        Ok(DocumentFile::new(
            file.name.clone(),
            ext,
            ExternalPath(file.id.clone()),
            file.modified_time,
        ))
    }

    async fn download(&self, file_id: &str) -> Result<Vec<u8>, ChonkitError> {
        self.download_drive_file(file_id).await
    }
}

fn is_google_binary(ext: DocumentType) -> bool {
    match ext {
        DocumentType::Text(_) | DocumentType::Excel => false,
        DocumentType::Docx | DocumentType::Pdf => true,
    }
}
