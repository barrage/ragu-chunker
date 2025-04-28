use clap::Parser;

// Adapter identifiers.

pub const FS_STORE_ID: &str = "fs";
#[cfg(feature = "qdrant")]
pub const QDRANT_ID: &str = "qdrant";
#[cfg(feature = "weaviate")]
pub const WEAVIATE_ID: &str = "weaviate";
#[cfg(feature = "gdrive")]
pub const GOOGLE_STORE_ID: &str = "google";
#[cfg(any(feature = "fe-remote", feature = "fe-local"))]
pub const FEMBED_EMBEDDER_ID: &str = "fembed";
#[cfg(feature = "openai")]
pub const OPENAI_EMBEDDER_ID: &str = "openai";
#[cfg(feature = "azure")]
pub const AZURE_EMBEDDER_ID: &str = "azure";

/// The ID for the default collection created on application startup.
pub const DEFAULT_COLLECTION_ID: uuid::Uuid = uuid::Uuid::nil();
/// The name for the default collection created on application startup.
pub const DEFAULT_COLLECTION_NAME: &str = "Chonkit_Default_Collection";
/// The size for the default collection created on application startup.
pub const DEFAULT_COLLECTION_SIZE: usize = 768;
/// The embedding provider for the default collection created on application startup.
pub const DEFAULT_COLLECTION_EMBEDDING_PROVIDER: &str = "fastembed";
/// The embedding model for the default collection created on application startup.
pub const DEFAULT_COLLECTION_EMBEDDING_MODEL: &str = "Xenova/bge-base-en-v1.5";
pub const DEFAULT_DOCUMENT_NAME: &str = "RaguruLabamba.txt";
pub const DEFAULT_DOCUMENT_CONTENT: &str = r#"Raguru Labamba, the pride of planet Gura, is celebrated as the finest ragu chef in the galaxy. With an innate mastery of Guran spices and interstellar ingredients, his ragus blend cosmic flavors into harmonies never tasted before. From his floating kitchen orbiting Gura’s twin moons, Raguru crafts dishes that draw food pilgrims from across the universe, cementing his legacy as the culinary star of his world."#;
/// The default upload path for the `fs` document storage provider.
const DEFAULT_UPLOAD_PATH: &str = "data/upload";
/// The default address to listen on.
const DEFAULT_ADDRESS: &str = "0.0.0.0:42069";

#[cfg(feature = "gdrive")]
const DEFAULT_GOOGLE_DRIVE_DOWNLOAD_PATH: &str = "data/gdrive";

#[derive(Debug, Parser)]
#[command(name = "chonkit", author = "biblius", version = "0.1", about = "Chunk documents", long_about = None)]
pub struct StartArgs {
    /// Address to listen on.
    #[arg(long, short)]
    address: Option<String>,

    /// RUST_LOG string to use as the env filter.
    #[arg(long, short)]
    log: Option<String>,

    /// Database URL.
    #[arg(long)]
    db_url: Option<String>,

    /// Set the upload path for `FsDocumentStore`.
    #[arg(long)]
    upload_path: Option<String>,

    /// CORS allowed origins.
    #[arg(long)]
    cors_allowed_origins: Option<String>,

    /// CORS allowed headers.
    #[arg(long)]
    cors_allowed_headers: Option<String>,

    /// Redis URL.
    #[arg(long)]
    redis_url: Option<String>,

    #[arg(long)]
    redis_embedding_db: Option<String>,

    #[arg(long)]
    redis_image_db: Option<String>,

    #[arg(long)]
    minio_url: Option<String>,

    #[arg(long)]
    minio_bucket: Option<String>,

    #[arg(long)]
    minio_access_key: Option<String>,

    #[arg(long)]
    minio_secret_key: Option<String>,

    /// Cookie domain used for setting chonkit-specific cookies.
    #[arg(long)]
    cookie_domain: Option<String>,

    /// Qdrant URL.
    #[cfg(feature = "qdrant")]
    #[arg(long)]
    qdrant_url: Option<String>,

    /// Weaviate URL.
    #[cfg(feature = "weaviate")]
    #[arg(long)]
    weaviate_url: Option<String>,

    /// If using the [AzureEmbeddings][crate::app::embedder::azure::AzureEmbeddings] module, set its endpoint.
    #[cfg(feature = "azure")]
    #[arg(long)]
    azure_endpoint: Option<String>,

    /// If using the [AzureEmbeddings][crate::app::embedder::azure::AzureEmbeddings] module, set its API version.
    #[cfg(feature = "azure")]
    #[arg(long)]
    azure_api_version: Option<String>,

    /// If using the fastembedder remote embedding module, set its endpoint.
    #[cfg(feature = "fe-remote")]
    #[arg(short, long)]
    fembed_url: Option<String>,

    /// Vault endpoint.
    #[cfg(feature = "auth-jwt")]
    #[arg(long)]
    jwks_endpoint: Option<String>,

    /// Vault approle role ID.
    #[cfg(feature = "auth-jwt")]
    #[arg(long)]
    jwt_issuer: Option<String>,

    #[cfg(feature = "gdrive")]
    #[arg(long)]
    google_drive_download_path: Option<String>,
}

/// Implement a getter method on [StartArgs], using the `$var` environment variable as a fallback
/// and either panic or default if neither the argument nor the environment variable is set.
macro_rules! arg {
    ($id:ident, $var:literal, panic $msg:literal) => {
        impl StartArgs {
            pub fn $id(&self) -> String {
                match &self.$id {
                    Some(val) => val.to_string(),
                    None => match std::env::var($var) {
                        Ok(val) => val,
                        Err(_) => panic!($msg),
                    },
                }
            }
        }
    };
    ($id:ident, $var:literal, default $value:expr) => {
        impl StartArgs {
            pub fn $id(&self) -> String {
                match &self.$id {
                    Some(val) => val.to_string(),
                    None => match std::env::var($var) {
                        Ok(val) => val,
                        Err(_) => $value,
                    },
                }
            }
        }
    };
}

impl StartArgs {
    pub fn allowed_origins(&self) -> Vec<String> {
        match &self.cors_allowed_origins {
            Some(origins) => origins
                .split(',')
                .filter_map(|o| (!o.is_empty()).then_some(String::from(o)))
                .collect(),
            None => match std::env::var("CORS_ALLOWED_ORIGINS") {
                Ok(origins) => origins
                    .split(',')
                    .filter_map(|o| (!o.is_empty()).then_some(String::from(o)))
                    .collect(),
                Err(_) => panic!(
                    "Allowed origins not found; Pass --cors-allowed-origins or set CORS_ALLOWED_ORIGINS as a comma separated list"
                ),
            },
        }
    }

    pub fn allowed_headers(&self) -> Vec<String> {
        match &self.cors_allowed_headers {
            Some(headers) => headers
                .split(',')
                .filter_map(|h| (!h.is_empty()).then_some(String::from(h)))
                .collect(),
            None => match std::env::var("CORS_ALLOWED_HEADERS") {
                Ok(headers) => headers
                    .split(',')
                    .filter_map(|h| (!h.is_empty()).then_some(String::from(h)))
                    .collect(),
                Err(_) => panic!(
                    "Allowed headers not found; Pass --cors-allowed-headers or set CORS_ALLOWED_HEADERS as a comma separated list"
                ),
            },
        }
    }

    #[cfg(feature = "openai")]
    pub fn open_ai_key(&self) -> String {
        std::env::var("OPENAI_KEY").expect("Missing OPENAI_KEY in env")
    }

    #[cfg(feature = "azure")]
    pub fn azure_key(&self) -> String {
        std::env::var("AZURE_KEY").expect("Missing AZURE_KEY in env")
    }
}

arg!(log,             "RUST_LOG",        default "info".to_string());
arg!(address,         "ADDRESS",         default DEFAULT_ADDRESS.to_string());
arg!(cookie_domain,   "COOKIE_DOMAIN",   panic   "Cookie domain not found; Pass --cookie-domain or set COOKIE_DOMAIN");
arg!(db_url,          "DATABASE_URL",    panic   "Database url not found; Pass --db-url or set DATABASE_URL");
arg!(upload_path,     "UPLOAD_PATH",     default DEFAULT_UPLOAD_PATH.to_string());

// redis

arg!(redis_url,       "REDIS_URL",       panic   "Redis url not found; Pass --redis-url or set REDIS_URL");

arg!(
    redis_embedding_db,
    "REDIS_EMBEDDING_DB",
    panic
    "Redis embedding db not found; Pass --redis-embedding-db or set REDIS_EMBEDDING_DB"
);

arg!(
    redis_image_db,
    "REDIS_IMAGE_DB",
    panic
    "Redis image db not found; Pass --redis-image-db or set REDIS_IMAGE_DB"
);

// minio
arg!(minio_url, "MINIO_URL", panic "Minio url not found; Pass --minio-url or set MINIO_URL");
arg!(minio_bucket, "MINIO_BUCKET", panic "Minio bucket not found; Pass --minio-bucket or set MINIO_BUCKET");
arg!(minio_access_key, "MINIO_ACCESS_KEY", panic "Minio access key not found; Pass --minio-access-key or set MINIO_ACCESS_KEY");
arg!(minio_secret_key, "MINIO_SECRET_KEY", panic "Minio secret key not found; Pass --minio-secret-key or set MINIO_SECRET_KEY");

// qdrant

#[cfg(feature = "qdrant")]
arg!(qdrant_url,      "QDRANT_URL",      panic   "Qdrant url not found; Pass --qdrant-url or set QDRANT_URL");

// weaviate

#[cfg(feature = "weaviate")]
arg!(weaviate_url,    "WEAVIATE_URL",    panic   "Weaviate url not found; Pass --weaviate-url or set WEAVIATE_URL");

// azure

#[cfg(feature = "azure")]
arg!(azure_endpoint,  "AZURE_ENDPOINT",  panic   "Azure endpoint not found; Pass --azure-endpoint or set AZURE_ENDPOINT");

#[cfg(feature = "azure")]
arg!(azure_api_version,  "AZURE_API_VERSION",  panic   "Azure api version not found; Pass --azure-api-version or set AZURE_API_VERSION");

// fe-remote

#[cfg(feature = "fe-remote")]
arg!(fembed_url,      "FEMBED_URL",      panic   "Fembed url not found; Pass --fembed-url or set FEMBED_URL");

// auth-jwt

#[cfg(feature = "auth-jwt")]
arg!(jwks_endpoint,  "JWKS_ENDPOINT",   panic "JWKs endpoint not found; Pass --jwks-endpoint or set JWKS_ENDPOINT");

#[cfg(feature = "auth-jwt")]
arg!(jwt_issuer,  "JWT_ISSUER",   panic "JWT issuer not found; Pass --jwt-issuer or set JWT_ISSUER");

// gdrive

#[cfg(feature = "gdrive")]
arg!(google_drive_download_path, "GOOGLE_DRIVE_DOWNLOAD_PATH", default DEFAULT_GOOGLE_DRIVE_DOWNLOAD_PATH.to_string());
