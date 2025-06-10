use crate::{error::ChonkitError, map_err};
use futures_util::future::BoxFuture;

pub mod collection;
pub mod document;
pub mod embedding;
pub mod image;

pub type Transaction<'tx> = sqlx::Transaction<'tx, sqlx::Postgres>;

/// Thin wrapper around a database connection pool.
/// Theoretically, this should be a generic repository not tied to SQL.
/// In practice, and let's be real here, we'll be using postgres. Always. Literally for
/// the past 3 years. It's okay.
#[derive(Debug, Clone)]
pub struct Repository {
    pub client: sqlx::PgPool,
}

impl Repository {
    pub async fn new(url: &str) -> Self {
        let pool = sqlx::postgres::PgPool::connect(url)
            .await
            .expect("error while connecting to db");

        sqlx::migrate!()
            .run(&pool)
            .await
            .expect("error in migrations");

        tracing::info!("Connected to postgres");

        Self { client: pool }
    }
}

impl Repository {
    pub async fn transaction<'tx, T, F>(&self, op: F) -> Result<T, ChonkitError>
    where
        F: for<'c> FnOnce(&'c mut Transaction<'tx>) -> BoxFuture<'c, Result<T, ChonkitError>>,
    {
        let mut tx = map_err!(self.client.begin().await);
        let result = op(&mut tx).await;
        match result {
            Ok(out) => {
                map_err!(tx.commit().await);
                Ok(out)
            }
            Err(err) => {
                map_err!(tx.rollback().await);
                Err(err)
            }
        }
    }
}
