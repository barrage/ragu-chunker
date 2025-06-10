use super::provider::Identity;
use crate::{core::model::image::Image, error::ChonkitError};
use std::sync::Arc;

pub type ImageStore = Arc<dyn ImageStorage + Send + Sync>;

/// Image BLOB storage interface.
#[async_trait::async_trait]
pub trait ImageStorage: Identity {
    /// Get the image bytes from the storage.
    async fn get_image(&self, path: &str) -> Result<Vec<u8>, ChonkitError>;

    /// Insert the image to BLOB storage.
    async fn store_image(&self, image: &Image) -> Result<(), ChonkitError>;

    /// Delete a single image from BLOB storage.
    async fn delete_image(&self, path: &str) -> Result<(), ChonkitError>;

    /// Check whether the image exists in BLOB storage.
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

    impl Identity for MinioClient {
        fn id(&self) -> &'static str {
            "minio"
        }
    }

    #[async_trait::async_trait]
    impl ImageStorage for MinioClient {
        async fn get_image(&self, path: &str) -> Result<Vec<u8>, ChonkitError> {
            Ok(map_err!(
                self.bucket
                    .get_object(format!("{}/{path}", self.prefix))
                    .await
            )
            .to_vec())
        }

        async fn store_image(&self, image: &Image) -> Result<(), ChonkitError> {
            map_err!(
                self.bucket
                    .put_object(
                        format!("{}/{}", self.prefix, image.path()),
                        &image.image.bytes
                    )
                    .await
            );

            Ok(())
        }

        async fn delete_image(&self, path: &str) -> Result<(), ChonkitError> {
            map_err!(
                self.bucket
                    .delete_object(format!("{}/{}", self.prefix, path))
                    .await
            );
            Ok(())
        }

        // async fn delete_all(&self, document_id: Uuid) -> Result<(), ChonkitError> {
        //     let images = self
        //         .repository
        //         .list_all_document_images(document_id, self.id())
        //         .await?;
        //
        //     for image in images {
        //         self.delete_image(&image.path).await?;
        //     }
        //
        //     Ok(())
        // }

        async fn exists(&self, path: &str) -> Result<bool, ChonkitError> {
            Ok(map_err!(
                self.bucket
                    .object_exists(format!("{}/{}", self.prefix, path))
                    .await
            ))
        }
    }
}
