use crate::{core::document::sha256, err, error::ChonkitError, map_err};
use base64::Engine;
use serde::{Deserialize, Serialize};
use sqlx::prelude::FromRow;
use utoipa::ToSchema;
use uuid::Uuid;

/// In memory representation of an image.
pub struct Image {
    /// The image data, including the bytes, format, width and height.
    pub image: ImageData,

    /// An optional description of the image, populated during parsing or when a user saves the
    /// image description, used for embedding purposes.
    pub description: Option<String>,

    /// The page of the document the image was found in (if originating from a document).
    pub page_number: Option<usize>,

    /// The number of the image on the page (if originating from a document).
    pub image_number: Option<usize>,
}

impl PartialEq for Image {
    fn eq(&self, other: &Self) -> bool {
        self.hash().0 == other.hash().0
    }
}

impl Image {
    pub fn new(
        page_number: Option<usize>,
        image_number: Option<usize>,
        bytes: Vec<u8>,
        format: image::ImageFormat,
        width: u32,
        height: u32,
    ) -> Self {
        Self {
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

    /// The path to the image in [ImageStorage][crate::core::image::ImageStorage].
    ///
    /// <BYTES_HASH>.<EXTENSION>.
    pub fn path(&self) -> String {
        format!(
            "{}.{}",
            sha256(self.image.bytes.as_slice()),
            self.image.format.extensions_str()[0]
        )
    }

    /// If the image has a description, it is appended to the bytes vector before hashing.
    /// Otherwise, the hash of the image bytes is returned.
    pub fn hash(&self) -> ImageHash {
        if let Some(description) = &self.description {
            let mut bytes = self.image.bytes.clone();
            bytes.extend_from_slice(description.as_bytes());
            ImageHash(sha256(&bytes))
        } else {
            ImageHash(sha256(&self.image.bytes))
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
    /// Image bytes.
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

    pub fn from_b64_data_uri(uri: &str) -> Result<Self, ChonkitError> {
        let Some(uri) = uri.strip_prefix("data:") else {
            return err!(InvalidFile, "Invalid data URI");
        };

        let Some((mime, b64)) = uri.split_once(";base64,") else {
            return err!(InvalidFile, "Invalid data URI");
        };

        let Some(format) = image::ImageFormat::from_mime_type(mime) else {
            return err!(InvalidFile, "Invalid mime type: {mime}");
        };

        let bytes = map_err!(base64::engine::general_purpose::STANDARD.decode(b64));

        let img = map_err!(image::load_from_memory_with_format(&bytes, format));

        Ok(Self {
            bytes,
            format,
            width: img.width(),
            height: img.height(),
        })
    }

    pub fn from_raw_bytes(bytes: &[u8]) -> Result<Self, ChonkitError> {
        let img = map_err!(image::load_from_memory(bytes));
        let format = map_err!(image::guess_format(bytes));

        Ok(Self {
            bytes: bytes.to_vec(),
            format,
            width: img.width(),
            height: img.height(),
        })
    }

    pub fn size_in_mb(&self) -> usize {
        (self.bytes.len() as f64 / 1024.0 / 1024.0) as usize
    }

    #[inline]
    pub const fn estimate_tokens(&self, patch_size: u32) -> u32 {
        (self.width / patch_size) * (self.height / patch_size)
    }
}

/// SHA256 hash of the image bytes, optionally appended with the image description before hashing.
///
/// Obtained via [Image::hash].
pub struct ImageHash(pub String);

impl std::fmt::Display for ImageHash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Image database model.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, ToSchema, FromRow)]
pub struct ImageModel {
    /// Image primary key
    pub id: Uuid,

    /// Path of the image in the image storage. See [Image::path].
    pub path: String,

    /// Image extension.
    pub format: String,

    /// SHA256 hash of the image bytes.
    pub hash: String,

    /// Image storage provider.
    pub src: String,

    /// Description of the image used when embedding.
    pub description: Option<String>,

    pub width: i32,
    pub height: i32,

    /// ID of the document where the image was found in.
    pub document_id: Option<Uuid>,

    /// The page of the document the image was found in.
    pub page_number: Option<i32>,

    /// The sequence number of the image on the page.
    pub image_number: Option<i32>,
}

pub struct InsertImage<'a> {
    pub path: &'a str,
    pub hash: &'a str,
    pub src: &'a str,
    pub format: &'a str,
    pub width: u32,
    pub height: u32,
    pub description: Option<&'a str>,

    // Document related fields if the image is obtained from a document.
    pub document_id: Option<Uuid>,
    pub page_number: Option<usize>,
    pub image_number: Option<usize>,
}
