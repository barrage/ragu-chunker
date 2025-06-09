use super::provider::Identity;
use crate::{
    core::model::image::{Image, ImageModel},
    error::ChonkitError,
};
use std::sync::Arc;
use uuid::Uuid;

pub type ImageStore = Arc<dyn ImageStorage + Send + Sync>;

#[async_trait::async_trait]
pub trait ImageStorage: Identity {
    /// Get a full image from BLOB storage with its metadata from the repository.
    async fn get_image(&self, path: &str) -> Result<Image, ChonkitError>;

    /// Insert the image to BLOB storage and the repository.
    async fn store_image(
        &self,
        document_id: Option<Uuid>,
        image: &Image,
    ) -> Result<ImageModel, ChonkitError>;

    /// Delete a single image from BLOB storage and the repository using its path.
    async fn delete_image(&self, path: &str) -> Result<(), ChonkitError>;

    /// Delete all image BLOBS and repository entries based on a document ID.
    ///
    /// Used when removing the document to ensure all its images are deleted as well.
    async fn delete_all(&self, document_id: Uuid) -> Result<(), ChonkitError>;

    /// Check whether the image exists in BLOB storage.
    async fn exists(&self, path: &str) -> Result<bool, ChonkitError>;
}

pub mod minio {
    use super::{Image, ImageStorage};
    use crate::{
        core::{
            model::image::{ImageData, ImageModel, InsertImage},
            provider::Identity,
            repo::Repository,
        },
        err,
        error::ChonkitError,
        map_err,
    };
    use s3::{creds::Credentials, Bucket, Region};
    use std::sync::Arc;
    use uuid::Uuid;

    #[derive(Clone)]
    pub struct MinioImageStorage {
        client: MinioClient,
        repository: Repository,
    }

    impl MinioImageStorage {
        pub fn new(client: MinioClient, repository: Repository) -> Self {
            Self { client, repository }
        }
    }

    #[derive(Clone)]
    pub struct MinioClient {
        bucket: Arc<Bucket>,
        prefix: String,
    }

    impl MinioClient {
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
    }

    impl Identity for MinioImageStorage {
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
    impl ImageStorage for MinioImageStorage {
        async fn get_image(&self, path: &str) -> Result<Image, ChonkitError> {
            let bytes = map_err!(
                self.client
                    .bucket
                    .get_object(format!("{}/{path}", self.client.prefix))
                    .await
            );

            let format = match image::ImageFormat::from_path(path) {
                Ok(f) => f,
                Err(e) => {
                    tracing::error!("Failed to parse image format: {e}");
                    return err!(InvalidFile, "Failed to parse image format: {e}");
                }
            };
            let image = self.repository.get_image_by_path(path).await?;

            Ok(Image {
                image: ImageData {
                    bytes: bytes.to_vec(),
                    format,
                    width: image.width as u32,
                    height: image.height as u32,
                },
                page_number: image.page_number.map(|p| p as usize),
                image_number: image.image_number.map(|i| i as usize),
                description: image.description.clone(),
            })
        }

        async fn store_image(
            &self,
            document_id: Option<Uuid>,
            image: &Image,
        ) -> Result<ImageModel, ChonkitError> {
            self.client.store_image(image).await?;
            self.repository
                .insert_image(
                    InsertImage {
                        document_id,
                        page_number: image.page_number,
                        image_number: image.image_number,
                        path: &image.path(),
                        hash: &image.hash().0,
                        src: self.id(),
                        format: image.image.format.extensions_str()[0],
                        description: image.description.as_deref(),
                        width: image.image.width,
                        height: image.image.height,
                    },
                    None,
                )
                .await
        }

        async fn delete_image(&self, path: &str) -> Result<(), ChonkitError> {
            map_err!(
                self.client
                    .bucket
                    .delete_object(format!("{}/{}", self.client.prefix, path))
                    .await
            );
            self.repository.delete_image_by_path(path).await?;

            Ok(())
        }

        async fn delete_all(&self, document_id: Uuid) -> Result<(), ChonkitError> {
            let images = self
                .repository
                .list_all_document_images(document_id, self.id())
                .await?;

            for image in images {
                self.delete_image(&image.path).await?;
                self.repository.delete_image_by_path(&image.path).await?;
            }

            Ok(())
        }

        async fn exists(&self, path: &str) -> Result<bool, ChonkitError> {
            Ok(map_err!(
                self.client
                    .bucket
                    .object_exists(format!("{}/{}", self.client.prefix, path))
                    .await
            ))
        }
    }
}
