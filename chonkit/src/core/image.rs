use std::sync::Arc;

use super::provider::Identity;
use crate::error::ChonkitError;
use base64::Engine;

pub type ImageStore = Arc<dyn ImageStorage + Send + Sync>;

/// An encoded image.
pub struct Image {
    /// The ID of the image, relevant to [ImageStorage].
    ///
    /// Since we are usually extracting images from documents, this field will be set by the
    /// parser.
    pub path: String,

    /// Encoded image bytes.
    pub bytes: Vec<u8>,

    /// Image format.
    pub format: image::ImageFormat,
}

impl Image {
    pub fn new(path: String, bytes: Vec<u8>, format: image::ImageFormat) -> Self {
        Self {
            path,
            bytes,
            format,
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

#[async_trait::async_trait]
pub trait ImageStorage: Identity {
    async fn get_image(&self, path: &str) -> Result<Image, ChonkitError>;

    async fn store_image(&self, image: &Image) -> Result<(), ChonkitError>;

    async fn exists(&self, path: &str) -> Result<bool, ChonkitError>;
}

pub mod minio {
    use super::{Image, ImageStorage};
    use crate::{core::provider::Identity, error::ChonkitError, map_err};
    use s3::{creds::Credentials, Bucket, Region};
    use std::sync::Arc;

    #[derive(Clone)]
    pub struct MinioClient {
        bucket: Arc<Bucket>,
        prefix: String,
    }

    impl Identity for MinioClient {
        fn id(&self) -> &'static str {
            "minio"
        }
    }

    impl MinioClient {
        pub async fn new(
            endpoint: String,
            bucket_name: String,
            access_key: String,
            secret_key: String,
        ) -> Self {
            tracing::info!("Connecting to minio at {endpoint}");
            let region = Region::Custom {
                region: "eu-central-1".to_owned(),
                endpoint,
            };
            let credentials =
                Credentials::new(Some(&access_key), Some(&secret_key), None, None, None)
                    .expect("s3 credentials error");

            let bucket = Bucket::new(&bucket_name, region.clone(), credentials.clone())
                .expect("cannot connect to bucket")
                .with_path_style();

            if !bucket
                .exists()
                .await
                .expect("cannot check if bucket exists")
            {
                panic!("bucket {bucket_name} does not exist");
            }

            MinioClient {
                bucket: Arc::new(*bucket),
                prefix: "images".to_owned(),
            }
        }
    }

    #[async_trait::async_trait]
    impl ImageStorage for MinioClient {
        async fn get_image(&self, path: &str) -> Result<Image, ChonkitError> {
            let bytes = map_err!(
                self.bucket
                    .get_object(format!("{}/{path}", self.prefix))
                    .await
            );

            Ok(Image {
                path: path.to_owned(),
                bytes: bytes.to_vec(),
                format: image::ImageFormat::WebP,
            })
        }

        async fn store_image(&self, image: &Image) -> Result<(), ChonkitError> {
            map_err!(
                self.bucket
                    .put_object(format!("{}/{}", self.prefix, image.path), &image.bytes)
                    .await
            );
            Ok(())
        }

        async fn exists(&self, path: &str) -> Result<bool, ChonkitError> {
            Ok(map_err!(
                self.bucket
                    .object_exists(format!("{}/{}", self.prefix, path))
                    .await
            ))
        }
    }
}
