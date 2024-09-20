use app::service::ServiceState;
use std::sync::Arc;
use tracing_subscriber::EnvFilter;

pub mod app;
pub mod cli;
pub mod config;
pub mod core;
pub mod error;

pub const DEFAULT_COLLECTION_NAME: &str = "chonkit_default_0";

pub async fn state(args: &config::StartArgs) -> ServiceState {
    // Ensures the dynamic library is loaded and panics if it isn't
    pdfium_render::prelude::Pdfium::default();

    let db_url = args.db_url();

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from(args.log()))
        .init();

    #[cfg(feature = "fe-local")]
    tracing::info!(
        "Cuda available: {:?}",
        ort::ExecutionProvider::is_available(&ort::CUDAExecutionProvider::default())
    );

    let postgres = app::repo::pg::init(&db_url).await;

    let fs_store = Arc::new(app::document::store::FsDocumentStore::new(
        &args.upload_path(),
    ));

    #[cfg(feature = "fe-local")]
    let fastembed = Arc::new(crate::app::embedder::fastembed::init());

    #[cfg(feature = "fe-remote")]
    let fastembed = Arc::new(crate::app::embedder::fastembed::init(args.fembed_url()));

    #[cfg(feature = "openai")]
    let openai = Arc::new(crate::app::embedder::openai::OpenAiEmbeddings::new(
        &args.open_ai_key(),
    ));

    #[cfg(feature = "qdrant")]
    let qdrant = Arc::new(crate::app::vector::qdrant::init(&args.qdrant_url()));

    #[cfg(feature = "weaviate")]
    let weaviate = Arc::new(crate::app::vector::weaviate::init(&args.weaviate_url()));

    ServiceState {
        postgres,

        fs_store,

        #[cfg(feature = "fembed")]
        fastembed,

        #[cfg(feature = "openai")]
        openai,

        #[cfg(feature = "qdrant")]
        qdrant,

        #[cfg(feature = "weaviate")]
        weaviate,
    }
}
