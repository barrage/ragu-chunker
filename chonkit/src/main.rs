/// Concrete implementations of the [core] module.
pub mod app;

/// Application starting arguments and configuration.
pub mod config;

/// Core business logic.
pub mod core;

/// Error types.
pub mod error;

use app::server::router::HttpConfiguration;
use clap::Parser;
use tracing::info;

#[tokio::main]
async fn main() {
    let args = crate::config::StartArgs::parse();
    let app = crate::app::state::AppState::new(&args).await;

    let addr = args.address();

    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .expect("error while starting TCP listener");

    let cors_origins = args.allowed_origins();
    let cors_headers = args.allowed_headers();
    let cookie_domain = args.cookie_domain();

    let config = HttpConfiguration {
        cors_origins: std::sync::Arc::from(&*cors_origins.leak()),
        cors_headers: std::sync::Arc::from(&*cors_headers.leak()),
        cookie_domain: cookie_domain.into(),
    };

    let router = crate::app::server::router::router(app, config);

    info!("Listening on {addr}");

    axum::serve(listener, router)
        .await
        .expect("error while starting server");
}
