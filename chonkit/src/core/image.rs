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
    async fn get_image(&self, path: &str) -> Result<Image, ChonkitError>;

    async fn store_image(
        &self,
        document_id: Uuid,
        image: &Image,
    ) -> Result<ImageModel, ChonkitError>;

    async fn exists(&self, path: &str) -> Result<bool, ChonkitError>;
}

pub mod minio {
    use super::{Image, ImageStorage};
    use crate::{
        core::{model::image::ImageModel, provider::Identity, repo::Repository},
        error::ChonkitError,
        map_err, transaction,
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
                    .put_object(format!("{}/{}", self.prefix, image.path), &image.bytes)
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

            let description = self.repository.get_image_description(path).await?;

            Ok(Image {
                path: path.to_owned(),
                bytes: bytes.to_vec(),
                format: image::ImageFormat::WebP,
                description,
            })
        }

        async fn store_image(
            &self,
            document_id: Uuid,
            image: &Image,
        ) -> Result<ImageModel, ChonkitError> {
            use crate::core::repo::Atomic;

            transaction!(self.repository, |tx| async move {
                let img = self
                    .repository
                    .insert_image(
                        document_id,
                        self.id(),
                        &image.path,
                        image.description.as_deref(),
                        Some(tx),
                    )
                    .await?;
                self.client.store_image(image).await?;
                Ok(img)
            })
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
