use base64::Engine;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

/// A raw encoded image obtained from the document.
pub struct Image {
    /// The ID of the image, relevant to [ImageStorage].
    ///
    /// Since we are usually extracting images from documents, this field will be set by the
    /// parser and is going to be in the format <IMAGE_HASH>_<PAGE_NUMBER>_<IMAGE_NUMBER>
    /// guaranteeing a unique ID of the image in the document.
    pub path: String,

    /// Encoded image bytes.
    pub bytes: Vec<u8>,

    /// Image format.
    pub format: image::ImageFormat,

    /// An optional description of the image, populated during parsing or when a user saves the
    /// image description.
    pub description: Option<String>,
}

impl PartialEq for Image {
    fn eq(&self, other: &Self) -> bool {
        self.path == other.path
    }
}

impl Image {
    pub fn new(path: String, bytes: Vec<u8>, format: image::ImageFormat) -> Self {
        Self {
            path,
            bytes,
            format,
            description: None,
        }
    }

    pub fn to_b64(&self) -> String {
        base64::engine::general_purpose::STANDARD.encode(self.bytes.as_slice())
    }

    pub fn to_b64_data_uri(&self) -> String {
        format!(
            "data:{};base64,{}",
            self.format.to_mime_type(),
            base64::engine::general_purpose::STANDARD.encode(self.bytes.as_slice())
        )
    }

    pub fn size_in_mb(&self) -> usize {
        (self.bytes.len() as f64 / 1024.0 / 1024.0) as usize
    }
}

impl std::fmt::Display for Image {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "RawImage {{ path: {}, size_MB: {}, format: {} }} ",
            self.path,
            self.size_in_mb(),
            self.format.to_mime_type()
        )
    }
}

impl std::fmt::Debug for Image {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self)
    }
}

/// Image database model.
#[derive(Debug, Serialize, Deserialize, PartialEq, ToSchema)]
pub struct ImageModel {
    pub path: String,
    pub document_id: Uuid,
    pub src: String,
    pub description: Option<String>,
}
