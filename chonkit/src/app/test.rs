//! Test suites and utilites.

mod document;
mod vector;

use super::{
    document::store::FsDocumentStore,
    state::{AppProviderState, AppState},
};
use crate::core::{
    cache::{init, ImageCache, TextEmbeddingCache},
    provider::{
        DocumentStorageProvider, ImageEmbeddingProvider, TextEmbeddingProvider, VectorDbProvider,
    },
    repo::Repository,
    service::{
        collection::CollectionService, document::DocumentService, embedding::EmbeddingService,
        external::ServiceFactory, ServiceState,
    },
    token::Tokenizer,
};
use crate::core::{image::minio::MinioImageStorage, provider::Identity};
use chonkit_embedders::EmbeddingModel;
use std::{
    collections::HashMap,
    sync::{Arc, OnceLock},
};
use testcontainers::{runners::AsyncRunner, ContainerAsync, GenericImage};
use testcontainers_modules::{postgres::Postgres, redis::Redis};

pub type PostgresContainer = ContainerAsync<Postgres>;
pub type AsyncContainer = ContainerAsync<GenericImage>;
pub type RedisContainer = ContainerAsync<Redis>;
pub type MinioContainer = ContainerAsync<testcontainers_modules::minio::MinIO>;

static DEFAULT_MODELS: OnceLock<HashMap<&'static str, EmbeddingModel>> = OnceLock::new();

struct TestState {
    /// Holds test containers so they don't get dropped.
    pub _containers: TestContainers,

    /// Holds the application state.
    pub app: AppState,

    pub embedding_cache: TextEmbeddingCache,

    /// Holds the list of active vector storage providers. Depends on feature flags.
    pub active_vector_providers: Vec<&'static str>,

    /// Holds the list of active embedding providers. Depends on feature flags.
    pub active_embedding_providers: Vec<&'static str>,
}

impl TestState {
    pub async fn init(config: TestStateConfig) -> Self {
        let mut models = HashMap::new();

        // Set up test containers

        let (postgres, postgres_img) = init_repository().await;
        let (embedding_cache, _, redis_img) = init_cache().await;
        let (minio, minio_img) = init_minio(postgres.clone()).await;

        #[cfg(feature = "qdrant")]
        let (qdrant, qdrant_img) = init_qdrant().await;

        #[cfg(feature = "weaviate")]
        let (weaviate, weaviate_img) = init_weaviate().await;

        // Set up document storage

        let mut document_storage = DocumentStorageProvider::default();

        let fs_store = Arc::new(FsDocumentStore::new(&config.fs_store_path).await);
        document_storage.register(fs_store);

        #[cfg(feature = "gdrive")]
        {
            let drive = Arc::new(
                crate::app::external::google::store::GoogleDriveStore::new(
                    &config._gdrive_download_path,
                )
                .await,
            );
            document_storage.register(drive);
        }

        // Set up vector storage

        let mut vector = VectorDbProvider::default();
        let mut active_vector_providers = vec![];

        #[cfg(feature = "qdrant")]
        {
            active_vector_providers.push(qdrant.id());
            vector.register(qdrant);
        }

        #[cfg(feature = "weaviate")]
        {
            active_vector_providers.push(weaviate.id());
            vector.register(weaviate);
        }

        // Set up embedders

        let mut embedding = TextEmbeddingProvider::default();
        let mut active_embedding_providers = vec![];

        #[cfg(feature = "fe-local")]
        {
            let fastembed = Arc::new(
                crate::app::embedder::fastembed::local::LocalFastEmbedder::new_with_model(
                    "Xenova/bge-base-en-v1.5",
                ),
            );

            active_embedding_providers.push(fastembed.id());

            models.insert(
                fastembed.id(),
                EmbeddingModel {
                    name: "Xenova/bge-base-en-v1.5".to_string(),
                    size: 768,
                    provider: fastembed.id().to_string(),
                    multimodal: false,
                },
            );

            embedding.register(fastembed);
        }

        // If active, overrides the fe-local implementation since we keep it on the same ID.
        #[cfg(feature = "fe-remote")]
        {
            let fastembed = Arc::new(
                crate::app::embedder::fastembed::remote::RemoteFastEmbedder::new(
                    String::new(), /* TODO */
                ),
            );

            if !active_embedding_providers.contains(&fastembed.id()) {
                active_embedding_providers.push(fastembed.id());
            }

            models.insert(
                fastembed.id(),
                EmbeddingModel {
                    name: "Xenova/bge-base-en-v1.5".to_string(),
                    size: 768,
                    provider: fastembed.id().to_string(),
                    multimodal: false,
                },
            );

            embedding.register(fastembed);
        }

        let image = Arc::new(minio);

        let tokenizer = Tokenizer::new();

        let providers = AppProviderState {
            database: postgres.clone(),
            vector: vector.clone(),
            embedding,
            image_embedding: ImageEmbeddingProvider::default(),
            document: document_storage,
            image,
        };

        let _containers = TestContainers {
            _postgres: postgres_img,
            _redis: redis_img,
            _minio: minio_img,
            #[cfg(feature = "qdrant")]
            _qdrant: qdrant_img,
            #[cfg(feature = "weaviate")]
            _weaviate: weaviate_img,
        };

        let services = ServiceState {
            collection: CollectionService::new(postgres.clone(), providers.clone().into()),
            document: DocumentService::new(postgres.clone(), providers.clone().into(), tokenizer),
            external: ServiceFactory::new(postgres.clone(), providers.clone().into()),
            embedding: EmbeddingService::new(
                postgres,
                providers.clone().into(),
                embedding_cache.clone(),
            ),
        };

        let app = AppState::new_test(services, providers);

        DEFAULT_MODELS.get_or_init(|| models);

        TestState {
            _containers,
            app,
            active_vector_providers,
            active_embedding_providers,
            embedding_cache,
        }
    }
}

struct TestStateConfig {
    pub fs_store_path: String,
    // We do not feature gate this to make our lives easier.
    pub _gdrive_download_path: String,
}

/// Holds test container images so they don't get dropped during execution of test suites.
struct TestContainers {
    pub _postgres: PostgresContainer,

    pub _redis: RedisContainer,

    pub _minio: MinioContainer,

    #[cfg(feature = "qdrant")]
    pub _qdrant: ContainerAsync<GenericImage>,

    #[cfg(feature = "weaviate")]
    pub _weaviate: ContainerAsync<GenericImage>,
}

impl AppState {
    #[cfg(test)]
    pub fn new_test(services: ServiceState, providers: AppProviderState) -> Self {
        use super::server::HttpConfiguration;
        use crate::app::batch;

        Self {
            services: services.clone(),
            providers,
            batch_embedder: batch::start_batch_embedder(services.clone()),
            http_client: reqwest::Client::new(),
            http_config: HttpConfiguration::default(),
            #[cfg(feature = "auth-jwt")]
            jwt_verifier: super::auth::JwtVerifier::new(
                jwtk::jwk::RemoteJwksVerifier::new(
                    "".to_string(),
                    None,
                    std::time::Duration::default(),
                ),
                "",
            ),
        }
    }
}

/// Setup a postgres test container and connect to it using PgPool.
/// Runs the migrations in the container.
/// When using suitest's [before_all][suitest::before_all], make sure you keep the TestState, othwerise the
/// container will get dropped and cleaned up.
pub async fn init_repository() -> (Repository, PostgresContainer) {
    let pg_image = Postgres::default()
        .start()
        .await
        .expect("postgres container error");

    let pg_host = pg_image.get_host().await.unwrap();
    let pg_port = pg_image.get_host_port_ipv4(5432).await.unwrap();
    let pg_url = format!("postgresql://postgres:postgres@{pg_host}:{pg_port}/postgres");
    (crate::core::repo::Repository::new(&pg_url).await, pg_image)
}

/// Setup a redis test container and connect to it using RedisPool.
/// When using suitest's [before_all][suitest::before_all], make sure you keep the TestState, othwerise the
/// container will get dropped and cleaned up.
pub async fn init_cache() -> (TextEmbeddingCache, ImageCache, RedisContainer) {
    let redis_image = Redis.start().await.unwrap();
    let redis_host = redis_image.get_host().await.unwrap();
    let redis_port = redis_image.get_host_port_ipv4(6379).await.unwrap();
    let redis_url = format!("redis://{redis_host}:{redis_port}");

    let embedding_cache = TextEmbeddingCache::new(init(&redis_url, "0").await);
    let image_cache = ImageCache::new(init(&redis_url, "1").await);

    (embedding_cache, image_cache, redis_image)
}

pub async fn init_minio(
    repository: Repository,
) -> (crate::core::image::minio::MinioImageStorage, MinioContainer) {
    let minio_image = testcontainers_modules::minio::MinIO::default()
        .start()
        .await
        .unwrap();

    let host = minio_image.get_host().await.unwrap();
    let port = minio_image.get_host_port_ipv4(9000).await.unwrap();

    let endpoint = format!("http://{host}:{port}");
    let bucket = "test-bucket";
    let access_key = "minioadmin";
    let secret_key = "minioadmin";

    let region = s3::region::Region::Custom {
        region: "eu-central-1".to_owned(),
        endpoint: endpoint.clone(),
    };

    let credentials =
        s3::creds::Credentials::new(Some(access_key), Some(secret_key), None, None, None)
            .expect("s3 credentials error");

    s3::Bucket::create_with_path_style(
        bucket,
        region.clone(),
        credentials.clone(),
        s3::BucketConfiguration::default(),
    )
    .await
    .expect("cannot create test bucket");

    let minio_client = crate::core::image::minio::MinioClient::new(
        endpoint,
        bucket.to_string(),
        "minioadmin".to_string(),
        "minioadmin".to_string(),
    )
    .await;

    (
        MinioImageStorage::new(minio_client, repository),
        minio_image,
    )
}

/// Setup a qdrant test container and connect to it using QdrantDb.
/// When using suitest's [before_all][suitest::before_all], make sure you keep the TestState, othwerise the
/// container will get dropped and cleaned up.
#[cfg(feature = "qdrant")]
pub async fn init_qdrant() -> (
    super::vector::qdrant::QdrantDb,
    ContainerAsync<GenericImage>,
) {
    use testcontainers::core::{IntoContainerPort, WaitFor};

    let qd_image = GenericImage::new("qdrant/qdrant", "v1.13.5")
        .with_exposed_port(6334.tcp())
        .with_wait_for(WaitFor::message_on_stdout("gRPC listening on"))
        .start()
        .await
        .expect("qdrant container error");

    let qd_host = qd_image.get_host().await.unwrap();
    let qd_port = qd_image.get_host_port_ipv4(6334).await.unwrap();
    let qd_url = format!("http://{qd_host}:{qd_port}");
    (crate::app::vector::qdrant::init(&qd_url), qd_image)
}

/// Setup a weaviate test container and connect to it using WeaviateDb.
/// When using suitest's [before_all][suitest::before_all], make sure you keep the TestState, othwerise the
/// container will get dropped and cleaned up.
#[cfg(feature = "weaviate")]
pub async fn init_weaviate() -> (
    super::vector::weaviate::WeaviateDb,
    ContainerAsync<GenericImage>,
) {
    use testcontainers::core::{ImageExt, IntoContainerPort, WaitFor};

    let wv_image = GenericImage::new("semitechnologies/weaviate", "1.24.12")
        .with_exposed_port(8080.tcp())
        .with_exposed_port(50051.tcp())
        .with_wait_for(WaitFor::message_on_stderr("Serving weaviate"))
        .with_env_var("AUTHENTICATION_ANONYMOUS_ACCESS_ENABLED", "true")
        .with_env_var("PERSISTENCE_DATA_PATH", "/var/lib/weaviate")
        .start()
        .await
        .expect("weaviate container error");

    let wv_host = wv_image.get_host().await.unwrap();
    let wv_port = wv_image.get_host_port_ipv4(8080).await.unwrap();
    let wv_url = format!("http://{wv_host}:{wv_port}");
    (crate::app::vector::weaviate::init(&wv_url), wv_image)
}
