use std::sync::Arc;

/// OpenAPI definitions.
mod api;

/// HTTP specific DTOs.
mod dto;

/// HTTP API.
pub mod router;

/// HTTP API middleware.
pub mod middleware;

#[derive(Debug, Clone)]
pub struct HttpConfiguration {
    pub cors_origins: Arc<[String]>,
    pub cors_headers: Arc<[String]>,
    pub cookie_domain: Arc<str>,
}

#[cfg(test)]
impl Default for HttpConfiguration {
    fn default() -> Self {
        HttpConfiguration {
            cors_origins: Arc::new([String::from("*")]),
            cors_headers: Arc::new([String::from("*")]),
            cookie_domain: "localhost".into(),
        }
    }
}
