use super::batch::{BatchEmbedder, BatchEmbedderHandle};
use crate::{
    app::document::store::FsDocumentStore,
    config::FS_STORE_ID,
    core::{
        chunk::ChunkConfig,
        model::document::{DocumentType, TextDocumentType},
        provider::{DocumentStorageProvider, EmbeddingProvider, ProviderState, VectorDbProvider},
        repo::Repository,
        service::{
            document::DocumentService, external::ExternalServiceFactory, vector::VectorService,
        },
    },
    error::ChonkitError,
};
use serde::Serialize;
use std::{collections::HashMap, sync::Arc};
use tracing_subscriber::EnvFilter;

#[derive(Clone)]
pub struct AppState {
    /// Chonkit services.
    pub services: ServiceState,

    /// Handle for batch embedding documents.
    pub batch_embedder: BatchEmbedderHandle,

    /// Downstream service providers for chonkit services.
    /// Used for displaying some metadata and in tests.
    pub providers: AppProviderState,

    #[cfg(feature = "auth-vault")]
    pub vault: crate::app::auth::vault::VaultAuthenticator,
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

        let vector_provider = Self::init_vector_providers(args);
        let embedding_provider = Self::init_embedding_providers(args);

        let storage = Self::init_storage(args);

        let providers = AppProviderState {
            database: repository.clone(),
            vector: vector_provider,
            embedding: embedding_provider,
            storage,
        };

        let document = DocumentService::new(repository.clone(), providers.clone().into());

        for provider in providers.storage.list_provider_ids() {
            match document.sync(provider).await {
                Ok(_) => tracing::info!("Synced document provider {provider}"),
                Err(e) => e.print(),
            }
        }

        let vector = VectorService::new(repository.clone(), providers.clone().into());

        let external = ExternalServiceFactory::new(
            repository,
            providers.clone().into(),
            #[cfg(feature = "gdrive")]
            crate::app::external::google::auth::GoogleOAuthConfig::new(
                &args.google_oauth_client_id(),
                &args.google_oauth_client_secret(),
            ),
        );

        document.create_default_document().await;

        for provider in providers.vector.list_provider_ids() {
            for e_provider in providers.embedding.list_provider_ids() {
                vector.create_default_collection(provider, e_provider).await;
            }
        }

        let service_state = ServiceState {
            document,
            vector,
            external,
        };

        let batch_embedder = Self::init_batch_embedder(service_state.clone());

        #[cfg(feature = "auth-vault")]
        let vault = Self::init_vault(args).await;

        Self {
            services: service_state,
            batch_embedder,
            providers,
            #[cfg(feature = "auth-vault")]
            vault,
        }
    }

    #[cfg(feature = "auth-vault")]
    async fn init_vault(
        args: &crate::config::StartArgs,
    ) -> crate::app::auth::vault::VaultAuthenticator {
        crate::app::auth::vault::VaultAuthenticator::new(
            args.vault_url(),
            args.vault_role_id(),
            args.vault_secret_id(),
            args.vault_key_name(),
        )
        .await
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
            provider.register(fastembed);
            tracing::info!("Registered local Fastembed embedding provider");
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
            provider.register(fastembed);
            tracing::info!("Registered remote Fastembed embedding provider");
        }

        #[cfg(feature = "openai")]
        {
            let openai = Arc::new(crate::app::embedder::openai::OpenAiEmbeddings::new(
                &_args.open_ai_key(),
            ));
            provider.register(openai);
            tracing::info!("Registered OpenAI embedding provider");
        }

        provider
    }

    fn init_storage(args: &crate::config::StartArgs) -> DocumentStorageProvider {
        let mut storage = DocumentStorageProvider::default();

        let fs = Arc::new(FsDocumentStore::new(&args.upload_path()));
        storage.register(fs);
        tracing::info!("Registered local FS storage provider");

        #[cfg(feature = "gdrive")]
        {
            let drive = Arc::new(crate::app::external::google::store::GoogleDriveStore::new(
                &args.google_drive_download_path(),
            ));
            storage.register(drive);
            tracing::info!("Registered Google Drive storage provider");
        };

        storage
    }

    // fn init_external_apis(_args: &crate::config::StartArgs) -> ExternalApiProvider {
    //
    // }

    fn init_batch_embedder(state: ServiceState) -> BatchEmbedderHandle {
        let (tx, rx) = tokio::sync::mpsc::channel(128);
        BatchEmbedder::new(rx, state).start();
        tx
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
            ],
        })
    }

    #[cfg(test)]
    pub fn new_test(
        services: ServiceState,
        providers: AppProviderState,
        #[cfg(feature = "auth-vault")] vault: super::auth::vault::VaultAuthenticator,
    ) -> Self {
        Self {
            services: services.clone(),
            providers,
            batch_embedder: Self::init_batch_embedder(services),
            #[cfg(feature = "auth-vault")]
            vault,
        }
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

#[derive(Clone)]
pub struct ServiceState {
    pub document: DocumentService,

    pub vector: VectorService,

    pub external: ExternalServiceFactory,
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

    pub supported_document_types: Vec<String>,
}
