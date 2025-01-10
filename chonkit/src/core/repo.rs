use crate::{error::ChonkitError, map_err};
use sqlx::Transaction;
use std::future::Future;

pub mod document;
pub mod vector;

/// Thin wrapper around a database connection pool that implements all core repository traits.
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

/// Bound for repositories that support atomic operations.
pub trait Atomic {
    /// Transaction type.
    type Tx;

    /// Start a database transaction.
    fn start_tx(&self) -> impl Future<Output = Result<Self::Tx, ChonkitError>>;

    /// Commit a database transaction.
    fn commit_tx(&self, tx: Self::Tx) -> impl Future<Output = Result<(), ChonkitError>>;

    /// Abort a database transaction.
    fn abort_tx(&self, tx: Self::Tx) -> impl Future<Output = Result<(), ChonkitError>>;
}

impl Atomic for Repository {
    type Tx = Transaction<'static, sqlx::Postgres>;

    async fn start_tx(&self) -> Result<Self::Tx, ChonkitError> {
        let tx = map_err!(self.client.begin().await);
        Ok(tx)
    }

    async fn commit_tx(&self, tx: Self::Tx) -> Result<(), ChonkitError> {
        map_err!(tx.commit().await);
        Ok(())
    }

    async fn abort_tx(&self, tx: Self::Tx) -> Result<(), ChonkitError> {
        map_err!(tx.rollback().await);
        Ok(())
    }
}

/// Uses `$repo` to start a transaction, passing it to the provided `$op`.
/// The provided `$op` must return a result.
/// Aborts the transaction on error and commits on success.
#[macro_export]
macro_rules! transaction {
    ($repo:expr, $op:expr) => {{
        let mut tx = $repo.start_tx().await?;
        let result = { $op(&mut tx) }.await;
        match result {
            Ok(out) => {
                $repo.commit_tx(tx).await?;
                Result::<_, ChonkitError>::Ok(out)
            }
            Err(err) => {
                $repo.abort_tx(tx).await?;
                return Err(err);
            }
        }
    }};

    (infallible $repo:expr, $op:expr) => {{
        let mut tx = $repo
            .start_tx()
            .await
            .expect("error in starting transaction");
        let result = { $op(&mut tx) }.await;
        match result {
            Ok(_) => {
                $repo
                    .commit_tx(tx)
                    .await
                    .expect("error in commiting transaction");
            }
            Err(_) => {
                $repo
                    .abort_tx(tx)
                    .await
                    .expect("error in aborting transaction");
            }
        }
    }};
}
