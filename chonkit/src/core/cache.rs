pub mod embedding;

pub use {redis::init, redis::ImageEmbeddingCache, redis::TextEmbeddingCache};

#[derive(Clone)]
pub struct AppCache {
    pub text: TextEmbeddingCache,
    pub image: ImageEmbeddingCache,
}

mod redis {
    use crate::{
        core::cache::embedding::{
            CachedImageEmbeddings, CachedTextEmbeddings, ImageEmbeddingCacheKey,
            TextEmbeddingCacheKey,
        },
        error::ChonkitError,
        map_err,
    };
    use deadpool_redis::redis;

    #[derive(Clone)]
    pub struct TextEmbeddingCache(deadpool_redis::Pool);

    impl TextEmbeddingCache {
        pub fn new(pool: deadpool_redis::Pool) -> Self {
            Self(pool)
        }
    }

    #[derive(Clone)]
    pub struct ImageEmbeddingCache(deadpool_redis::Pool);

    impl ImageEmbeddingCache {
        pub fn new(pool: deadpool_redis::Pool) -> Self {
            Self(pool)
        }
    }

    pub async fn init(url: &str, db: &str) -> deadpool_redis::Pool {
        deadpool_redis::Config::from_url(format!("{url}/{db}"))
            .create_pool(Some(deadpool_redis::Runtime::Tokio1))
            .unwrap_or_else(|_| panic!("unable to create redis connection pool for db {db}"))
    }

    impl TextEmbeddingCache {
        pub async fn get(
            &self,
            key: &TextEmbeddingCacheKey,
        ) -> Result<Option<CachedTextEmbeddings>, ChonkitError> {
            let __start = std::time::Instant::now();

            let mut conn = map_err!(self.0.get().await);
            let data: Option<String> = map_err!(
                redis::cmd("GET")
                    .arg(key.key())
                    .query_async(&mut conn)
                    .await
            );

            let Some(data) = data else {
                return Ok(None);
            };

            let data = map_err!(serde_json::from_str::<CachedTextEmbeddings>(&data));

            tracing::debug!(
                "embedding retrieval took {}ms ({} vectors)",
                __start.elapsed().as_millis(),
                data.embeddings.len()
            );

            Ok(Some(data))
        }

        pub async fn set(
            &self,
            key: &TextEmbeddingCacheKey,
            value: CachedTextEmbeddings,
        ) -> Result<(), crate::error::ChonkitError> {
            let data = map_err!(serde_json::to_string(&value));
            let mut conn = map_err!(self.0.get().await);
            map_err!(
                redis::cmd("SET")
                    .arg(key.key())
                    .arg(data)
                    .query_async::<()>(&mut conn)
                    .await
            );

            Ok(())
        }

        pub async fn exists(&self, key: &TextEmbeddingCacheKey) -> Result<bool, ChonkitError> {
            let mut conn = map_err!(self.0.get().await);

            Ok(map_err!(
                redis::cmd("EXISTS")
                    .arg(key.key())
                    .query_async::<u64>(&mut conn)
                    .await
            ) == 1)
        }

        pub async fn clear(&self) -> Result<(), ChonkitError> {
            let mut conn = map_err!(self.0.get().await);
            map_err!(redis::cmd("FLUSHDB").query_async::<()>(&mut conn).await);
            Ok(())
        }
    }

    impl ImageEmbeddingCache {
        pub async fn get(
            &self,
            key: &ImageEmbeddingCacheKey<'_>,
        ) -> Result<Option<CachedImageEmbeddings>, ChonkitError> {
            let __start = std::time::Instant::now();

            let mut conn = map_err!(self.0.get().await);
            let data: Option<String> = map_err!(
                redis::cmd("GET")
                    .arg(key.key())
                    .query_async(&mut conn)
                    .await
            );

            let Some(data) = data else {
                return Ok(None);
            };

            let data = map_err!(serde_json::from_str::<CachedImageEmbeddings>(&data));

            tracing::debug!(
                "image embeddings retrieved; key {}; {}ms",
                key.key(),
                __start.elapsed().as_millis(),
            );

            Ok(Some(data))
        }

        pub async fn set(
            &self,
            key: &ImageEmbeddingCacheKey<'_>,
            value: CachedImageEmbeddings,
        ) -> Result<(), ChonkitError> {
            let data = map_err!(serde_json::to_string(&value));
            let mut conn = map_err!(self.0.get().await);
            tracing::debug!("Caching image embedding at key {}", key.key());
            map_err!(
                redis::cmd("SET")
                    .arg(key.key())
                    .arg(data)
                    .query_async::<()>(&mut conn)
                    .await
            );

            Ok(())
        }

        pub async fn exists(&self, key: &str) -> Result<bool, ChonkitError> {
            let mut conn = map_err!(self.0.get().await);
            Ok(map_err!(
                redis::cmd("EXISTS")
                    .arg(key)
                    .query_async::<u64>(&mut conn)
                    .await
            ) == 1)
        }

        pub async fn clear(&self) -> Result<(), ChonkitError> {
            let mut conn = map_err!(self.0.get().await);
            map_err!(redis::cmd("FLUSHDB").query_async::<()>(&mut conn).await);
            Ok(())
        }
    }
}
