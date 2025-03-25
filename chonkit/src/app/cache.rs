use crate::{
    core::cache::{CachedEmbeddings, EmbeddingCache, EmbeddingCacheKey},
    error::ChonkitError,
    map_err,
};
use deadpool_redis::redis;

pub async fn init_redis(url: &str) -> deadpool_redis::Pool {
    deadpool_redis::Config::from_url(url)
        .create_pool(Some(deadpool_redis::Runtime::Tokio1))
        .expect("unable to create redis connection pool")
}

impl EmbeddingCache for deadpool_redis::Pool {
    async fn get(&self, key: &EmbeddingCacheKey) -> Result<Option<CachedEmbeddings>, ChonkitError> {
        let __start = std::time::Instant::now();

        let mut conn = map_err!(self.get().await);
        let data: Option<String> = map_err!(
            redis::cmd("GET")
                .arg(key.key())
                .query_async(&mut conn)
                .await
        );

        let Some(data) = data else {
            return Ok(None);
        };

        let data = map_err!(serde_json::from_str::<CachedEmbeddings>(&data));

        tracing::debug!(
            "embedding retrieval took {}ms ({} vectors)",
            __start.elapsed().as_millis(),
            data.embeddings.len()
        );

        Ok(Some(data))
    }

    async fn set(
        &self,
        key: &EmbeddingCacheKey,
        embeddings: CachedEmbeddings,
    ) -> Result<(), crate::error::ChonkitError> {
        let data = map_err!(serde_json::to_string(&embeddings));
        let mut conn = map_err!(self.get().await);
        map_err!(
            redis::cmd("SET")
                .arg(key.key())
                .arg(data)
                .query_async::<()>(&mut conn)
                .await
        );

        Ok(())
    }

    async fn exists(&self, key: &EmbeddingCacheKey) -> Result<bool, ChonkitError> {
        let mut conn = map_err!(self.get().await);

        Ok(map_err!(
            redis::cmd("EXISTS")
                .arg(key.key())
                .query_async::<u64>(&mut conn)
                .await
        ) == 1)
    }

    async fn clear(&self) -> Result<(), ChonkitError> {
        let mut conn = map_err!(self.get().await);
        map_err!(redis::cmd("FLUSHDB").query_async::<()>(&mut conn).await);
        Ok(())
    }
}
