use crate::core::document::sha256;
use base64::Engine;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

/// An image obtained from a document during parsing.
pub struct Image {
    pub document_hash: String,

    pub page_number: usize,

    pub image_number: usize,

    pub image: ImageData,

    /// An optional description of the image, populated during parsing or when a user saves the
    /// image description, used for embedding purposes.
    pub description: Option<String>,
}

impl PartialEq for Image {
    fn eq(&self, other: &Self) -> bool {
        self.path() == other.path() && self.hash() == other.hash()
    }
}

impl Image {
    pub fn new(
        document_hash: String,
        page_number: usize,
        image_number: usize,
        bytes: Vec<u8>,
        format: image::ImageFormat,
        width: u32,
        height: u32,
    ) -> Self {
        Self {
            document_hash,
            page_number,
            image_number,
            image: ImageData {
                bytes,
                format,
                width,
                height,
            },
            description: None,
        }
    }

    /// The ID of the image, relevant to [ImageStorage].
    ///
    /// Since we are usually extracting images from documents, this field will be set by the
    /// parser and is going to be in the format <DOCUMENT_HASH>_<PAGE_NUMBER>_<IMAGE_NUMBER>
    /// guaranteeing a unique ID of the image in the document.
    pub fn path(&self) -> String {
        format!(
            "{}_{}_{}.{}",
            self.document_hash,
            self.page_number,
            self.image_number,
            self.image.format.extensions_str()[0]
        )
    }

    pub fn hash(&self) -> String {
        if let Some(description) = &self.description {
            let mut bytes = self.image.bytes.clone();
            bytes.extend_from_slice(description.as_bytes());
            sha256(&bytes)
        } else {
            sha256(&self.image.bytes)
        }
    }
}

impl std::fmt::Display for Image {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Image {{ path: {}, estimated_tokens: {}, size_MB: {}, mime: {}, description: {:?}, hash: {}, w: {}, h: {} }} ",
            self.path(),
            self.image.estimate_tokens(14),
            self.image.size_in_mb(),
            self.image.format.to_mime_type(),
            self.description,
            self.hash(),
            self.image.width,
            self.image.height,
        )
    }
}

impl std::fmt::Debug for Image {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self)
    }
}

/// Image obtained when loading from document storage.
pub struct ImageData {
    /// Encoded image bytes.
    pub bytes: Vec<u8>,

    /// Image format.
    pub format: image::ImageFormat,

    pub width: u32,

    pub height: u32,
}

impl ImageData {
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

    #[inline]
    pub const fn estimate_tokens(&self, patch_size: u32) -> u32 {
        (self.width / patch_size) * (self.height / patch_size)
    }
}

/// Image database model.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, ToSchema)]
pub struct ImageModel {
    /// Path of the image in the image storage. See [ParsedImage::path].
    pub path: String,

    /// The page of the document the image was found in.
    pub page_number: i32,

    /// The sequence number of the image on the page.
    pub image_number: i32,

    /// Image extension.
    pub format: String,

    /// SHA256 hash of the image bytes.
    pub hash: String,

    /// ID of the document where the image was found in.
    pub document_id: Uuid,

    /// Image storage provider.
    pub src: String,

    /// Description of the image used when embedding.
    pub description: Option<String>,

    pub width: i32,
    pub height: i32,
}

pub struct InsertImage<'a> {
    pub document_id: Uuid,
    pub page_number: usize,
    pub image_number: usize,
    pub path: &'a str,
    pub hash: &'a str,
    pub src: &'a str,
    pub format: &'a str,
    pub description: Option<&'a str>,
    pub width: u32,
    pub height: u32,
}
