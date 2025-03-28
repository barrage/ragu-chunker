use super::{
    batch::{self, BatchEmbedderHandle},
    cache::init_redis,
    server::HttpConfiguration,
};
use crate::{
    app::document::store::FsDocumentStore,
    config::FS_STORE_ID,
    core::{
        chunk::ChunkConfig,
        document::{DocumentType, TextDocumentType},
        provider::{
            DocumentStorageProvider, EmbeddingProvider, Identity, ProviderState, VectorDbProvider,
        },
        repo::Repository,
        service::{
            document::DocumentService, embedding::EmbeddingService, external::ServiceFactory,
            vector::CollectionService, ServiceState,
        },
        token::Tokenizer,
    },
    error::ChonkitError,
};
use serde::Serialize;
use std::{collections::HashMap, sync::Arc};
use tracing_subscriber::EnvFilter;

#[derive(Clone)]
pub struct AppState {
    /// Chonkit services.
    pub services: ServiceState<deadpool_redis::Pool>,

    /// Handle for batch embedding documents.
    pub batch_embedder: BatchEmbedderHandle,

    /// Downstream service providers for chonkit services.
    /// Used for displaying some metadata and in tests.
    pub providers: AppProviderState,

    /// HTTP client for making requests to external services.
    pub http_client: reqwest::Client,

    /// The http configuration for the server for CORS and cookies.
    pub http_config: HttpConfiguration,

    #[cfg(feature = "auth-jwt")]
    pub jwt_verifier: super::auth::JwtVerifier,
}

impl AppState {
    /// Load the application state using the provided configuration.
    pub async fn new(args: &crate::config::StartArgs) -> Self {
        // Ensures the dynamic library is loaded and panics if it isn't
        pdfium_render::prelude::Pdfium::default();

        tracing_subscriber::fmt()
            .with_env_filter(EnvFilter::from(args.log()))
            .init();

        let repository = crate::core::repo::Repository::new(&args.db_url()).await;
        let cache = init_redis(&args.redis_url()).await;

        let providers = AppProviderState {
            database: repository.clone(),
            vector: Self::init_vector_providers(args),
            embedding: Self::init_embedding_providers(args),
            storage: Self::init_storage(args).await,
        };

        let services = ServiceState {
            document: DocumentService::new(
                repository.clone(),
                providers.clone().into(),
                Tokenizer::new(),
            ),
            collection: CollectionService::new(repository.clone(), providers.clone().into()),
            external: ServiceFactory::new(repository.clone(), providers.clone().into()),
            embedding: EmbeddingService::new(repository, providers.clone().into(), cache),
        };

        services.document.create_default_document().await;

        let http_client = reqwest::Client::new();

        #[cfg(feature = "auth-jwt")]
        let jwt_verifier = {
            let mut verifier = jwtk::jwk::RemoteJwksVerifier::new(
                args.jwks_endpoint(),
                Some(http_client.clone()),
                std::time::Duration::from_secs(300),
            );

            verifier.set_require_kid(true);

            super::auth::JwtVerifier::new(verifier, &args.jwt_issuer())
        };

        Self {
            services: services.clone(),

            batch_embedder: batch::start_batch_embedder(services),

            providers,

            http_client,

            http_config: Self::server_config(args),

            #[cfg(feature = "auth-jwt")]
            jwt_verifier,
        }
    }

    fn server_config(args: &crate::config::StartArgs) -> HttpConfiguration {
        let cors_origins = args.allowed_origins();
        let cors_headers = args.allowed_headers();
        let cookie_domain = args.cookie_domain();

        HttpConfiguration {
            cors_origins: std::sync::Arc::from(&*cors_origins.leak()),
            cors_headers: std::sync::Arc::from(&*cors_headers.leak()),
            cookie_domain: cookie_domain.into(),
        }
    }

    fn init_vector_providers(args: &crate::config::StartArgs) -> VectorDbProvider {
        let mut provider = VectorDbProvider::default();

        #[cfg(feature = "qdrant")]
        {
            let qdrant = crate::app::vector::qdrant::init(&args.qdrant_url());
            provider.register(qdrant);
            tracing::info!("Registered Qdrant vector provider");
        }

        #[cfg(feature = "weaviate")]
        {
            let weaviate = crate::app::vector::weaviate::init(&args.weaviate_url());
            provider.register(weaviate);
            tracing::info!("Registered Weaviate vector provider");
        }

        provider
    }

    fn init_embedding_providers(_args: &crate::config::StartArgs) -> EmbeddingProvider {
        #[cfg(not(any(feature = "fe-local", feature = "fe-remote", feature = "openai")))]
        compile_error!("one of `fe-local`, `fe-remote` or `openai` features must be enabled");

        let mut provider = EmbeddingProvider::default();

        #[cfg(feature = "fe-local")]
        {
            let fastembed =
                Arc::new(crate::app::embedder::fastembed::local::LocalFastEmbedder::new());
            tracing::info!("Registered embedding provider: {} (local)", fastembed.id());
            provider.register(fastembed);
        }

        // Remote implementations take precedence. This will override the local implementation
        // in the provider state.
        #[cfg(feature = "fe-remote")]
        {
            let fastembed = Arc::new(
                crate::app::embedder::fastembed::remote::RemoteFastEmbedder::new(
                    _args.fembed_url(),
                ),
            );
            tracing::info!("Registered embedding provider: {} (remote)", fastembed.id());
            provider.register(fastembed);
        }

        #[cfg(feature = "openai")]
        {
            let openai = Arc::new(crate::app::embedder::openai::OpenAiEmbeddings::new(
                &_args.open_ai_key(),
            ));
            tracing::info!("Registered embedding provider: {}", openai.id());
            provider.register(openai);
        }

        #[cfg(feature = "azure")]
        {
            let azure = Arc::new(crate::app::embedder::azure::AzureEmbeddings::new(
                &_args.azure_endpoint(),
                &_args.azure_key(),
                &_args.azure_api_version(),
            ));
            tracing::info!("Registered embedding provider: {}", azure.id());
            provider.register(azure);
        }

        provider
    }

    async fn init_storage(args: &crate::config::StartArgs) -> DocumentStorageProvider {
        let mut storage = DocumentStorageProvider::default();

        let fs = Arc::new(FsDocumentStore::new(&args.upload_path()).await);
        tracing::info!("Registered storage provider: {}", fs.id());
        storage.register(fs);

        #[cfg(feature = "gdrive")]
        {
            let drive = Arc::new(
                crate::app::external::google::store::GoogleDriveStore::new(
                    &args.google_drive_download_path(),
                )
                .await,
            );
            tracing::info!("Registered storage provider: {}", drive.id());
            storage.register(drive);
        };

        storage
    }

    /// Used for metadata display.
    pub async fn get_configuration(&self) -> Result<AppConfig, ChonkitError> {
        let mut embedding_providers = HashMap::new();
        let mut default_chunkers = vec![
            ChunkConfig::sliding_default(),
            ChunkConfig::snapping_default(),
        ];

        for provider in self.providers.embedding.list_provider_ids() {
            let embedder = self.providers.embedding.get_provider(provider)?;
            let default_model = embedder.default_model().0;

            default_chunkers.push(ChunkConfig::semantic_default(
                embedder.id().to_string(),
                default_model,
            ));

            let models = embedder
                .list_embedding_models()
                .await?
                .into_iter()
                .collect();

            embedding_providers.insert(provider.to_string(), models);
        }

        let document_providers = vec![
            FS_STORE_ID.to_string(),
            #[cfg(feature = "gdrive")]
            crate::config::GOOGLE_STORE_ID.to_string(),
        ];

        Ok(AppConfig {
            vector_providers: self
                .providers
                .vector
                .list_provider_ids()
                .iter()
                .map(|s| s.to_string())
                .collect(),
            embedding_providers,
            default_chunkers,
            document_providers,
            supported_document_types: vec![
                DocumentType::Text(TextDocumentType::Md).to_string(),
                DocumentType::Text(TextDocumentType::Csv).to_string(),
                DocumentType::Text(TextDocumentType::Xml).to_string(),
                DocumentType::Text(TextDocumentType::Json).to_string(),
                DocumentType::Text(TextDocumentType::Txt).to_string(),
                DocumentType::Docx.to_string(),
                DocumentType::Pdf.to_string(),
                DocumentType::Excel.to_string(),
            ],
        })
    }
}

/// Concrete version of [ProviderState].
#[derive(Clone)]
pub struct AppProviderState {
    pub database: Repository,
    pub vector: VectorDbProvider,
    pub embedding: EmbeddingProvider,
    pub storage: DocumentStorageProvider,
}

impl From<AppProviderState> for ProviderState {
    fn from(value: AppProviderState) -> ProviderState {
        ProviderState {
            vector: value.vector,
            embedding: value.embedding,
            storage: value.storage,
        }
    }
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AppConfig {
    /// A list of available vector providers.
    pub vector_providers: Vec<String>,

    /// A map of available embedding providers, their models and their respective model sizes.
    pub embedding_providers: HashMap<String, HashMap<String, usize>>,

    /// A list of available document storage providers.
    pub document_providers: Vec<String>,

    /// A list of default chunking configurations.
    pub default_chunkers: Vec<ChunkConfig>,

    /// A list of extensions supported by chonkit.
    pub supported_document_types: Vec<String>,
}
