use super::Atomic;
use crate::{
    core::{
        model::{
            embedding::{
                Embedding, EmbeddingInsert, EmbeddingReport, EmbeddingReportAddition,
                EmbeddingReportRemoval,
            },
            List, Pagination,
        },
        repo::Repository,
        service::embedding::ListEmbeddingReportsParams,
    },
    error::ChonkitError,
    map_err,
};
use sqlx::Postgres;
use uuid::Uuid;

impl Repository {
    pub async fn insert_embeddings(
        &self,
        embeddings: EmbeddingInsert,
        tx: Option<&mut <Self as Atomic>::Tx>,
    ) -> Result<Embedding, ChonkitError>
    where
        Self: Atomic,
    {
        let EmbeddingInsert {
            id,
            document_id,
            collection_id,
        } = embeddings;

        let query = sqlx::query_as!(
            Embedding,
            r#"
                    INSERT INTO embeddings(id, document_id, collection_id)
                    VALUES ($1, $2, $3)
                    ON CONFLICT(id) DO UPDATE
                    SET id = $1
                    RETURNING 
                    id, document_id, collection_id, created_at, updated_at
                "#,
            id,
            document_id,
            collection_id,
        );

        match tx {
            Some(tx) => Ok(map_err!(query.fetch_one(&mut **tx).await)),
            None => Ok(map_err!(query.fetch_one(&self.client).await)),
        }
    }

    pub async fn get_all_embeddings(
        &self,
        document_id: Uuid,
    ) -> Result<Vec<Embedding>, ChonkitError> {
        Ok(map_err!(
            sqlx::query_as!(
                Embedding,
                "SELECT id, document_id, collection_id, created_at, updated_at 
             FROM embeddings
             WHERE document_id = $1",
                document_id
            )
            .fetch_all(&self.client)
            .await
        ))
    }

    pub async fn get_embeddings(
        &self,
        document_id: Uuid,
        collection_id: Uuid,
    ) -> Result<Option<Embedding>, ChonkitError> {
        Ok(map_err!(
            sqlx::query_as!(
                Embedding,
                "SELECT id, document_id, collection_id, created_at, updated_at 
             FROM embeddings
             WHERE document_id = $1 AND collection_id = $2",
                document_id,
                collection_id
            )
            .fetch_optional(&self.client)
            .await
        ))
    }

    pub async fn list_embeddings(
        &self,
        pagination: Pagination,
        collection_id: Option<Uuid>,
    ) -> Result<List<Embedding>, ChonkitError> {
        let total = map_err!(sqlx::query!(
            "SELECT COUNT(id) FROM embeddings WHERE $1::UUID IS NULL OR collection_id = $1",
            collection_id
        )
        .fetch_one(&self.client)
        .await
        .map(|row| row.count.map(|count| count as usize)));

        let (limit, offset) = pagination.to_limit_offset();

        let embeddings = map_err!(
            sqlx::query_as!(
                Embedding,
                "SELECT id, document_id, collection_id, created_at, updated_at 
             FROM embeddings
             WHERE $1::UUID IS NULL OR collection_id = $1
             LIMIT $2 OFFSET $3",
                collection_id,
                limit,
                offset
            )
            .fetch_all(&self.client)
            .await
        )
        .into_iter()
        .collect();

        Ok(List::new(total, embeddings))
    }

    pub async fn get_embeddings_by_name(
        &self,
        document_id: Uuid,
        collection_name: &str,
        provider: &str,
    ) -> Result<Option<Embedding>, ChonkitError> {
        Ok(map_err!(sqlx::query_as!(
            Embedding,
            "SELECT id, document_id, collection_id, created_at, updated_at 
             FROM embeddings
             WHERE document_id = $1 AND collection_id = (SELECT id FROM collections WHERE name = $2 AND provider = $3)",
            document_id,
            collection_name,
            provider
        )
        .fetch_optional(&self.client)
        .await))
    }

    pub async fn delete_embeddings(
        &self,
        document_id: Uuid,
        collection_id: Uuid,
        tx: Option<&mut <Self as Atomic>::Tx>,
    ) -> Result<u64, ChonkitError>
    where
        Self: Atomic,
    {
        let query = sqlx::query!(
            "DELETE FROM embeddings WHERE document_id = $1 AND collection_id = $2",
            document_id,
            collection_id
        );

        match tx {
            Some(tx) => Ok(map_err!(query.execute(&mut **tx).await).rows_affected()),
            None => Ok(map_err!(query.execute(&self.client).await).rows_affected()),
        }
    }

    pub async fn delete_all_embeddings(&self, collection_id: Uuid) -> Result<u64, ChonkitError> {
        Ok(map_err!(
            sqlx::query!(
                "DELETE FROM embeddings WHERE collection_id = $1",
                collection_id
            )
            .execute(&self.client)
            .await
        )
        .rows_affected())
    }

    pub async fn list_outdated_embeddings(
        &self,
        collection_id: Uuid,
    ) -> Result<Vec<Embedding>, ChonkitError> {
        Ok(map_err!(
            sqlx::query_as!(
                Embedding,
                r#"
                    SELECT e.id, e.document_id, e.collection_id, e.created_at, e.updated_at 
                    FROM embeddings e
                    LEFT JOIN documents
                    ON e.document_id = documents.id
                    WHERE collection_id = $1 AND e.created_at < documents.updated_at
                "#,
                collection_id
            )
            .fetch_all(&self.client)
            .await
        ))
    }

    pub async fn insert_embedding_report(
        &self,
        report: &EmbeddingReportAddition,
    ) -> Result<(), ChonkitError> {
        map_err!(
            sqlx::query!(
                r#"
                INSERT INTO embedding_reports(
                    collection_id,
                    collection_name,
                    document_id,
                    document_name,
                    embedding_provider,
                    model_used,
                    vector_db,
                    total_vectors,
                    tokens_used,
                    cache,
                    started_at,
                    finished_at
                ) 
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
            "#,
                report.collection_id,
                report.collection_name,
                report.document_id,
                report.document_name,
                report.embedding_provider,
                report.model_used,
                report.vector_db,
                report.total_vectors as i32,
                report.tokens_used.map(|t| t as i32),
                report.cache,
                report.started_at,
                report.finished_at,
            )
            .execute(&self.client)
            .await
        );
        Ok(())
    }

    pub async fn insert_embedding_removal_report(
        &self,
        report: &EmbeddingReportRemoval,
    ) -> Result<(), ChonkitError> {
        map_err!(
            sqlx::query!(
                r#"
            INSERT INTO embedding_removal_reports(
                document_id,
                document_name,
                collection_id,
                collection_name,
                started_at,
                finished_at
            )
            VALUES ($1, $2, $3, $4, $5, $6)
            "#,
                report.document_id,
                report.document_name,
                report.collection_id,
                report.collection_name,
                report.started_at,
                report.finished_at
            )
            .execute(&self.client)
            .await
        );

        Ok(())
    }

    pub async fn list_collection_embedding_reports(
        &self,
        params: ListEmbeddingReportsParams,
    ) -> Result<Vec<EmbeddingReport>, ChonkitError> {
        let (limit, offset) = params
            .options
            .map(|p| p.to_limit_offset())
            .unwrap_or_else(|| Pagination::default().to_limit_offset());

        let mut query = sqlx::query_builder::QueryBuilder::<Postgres>::new(
            r#"
                SELECT 
                    id, 
                    'addition' as "ty",
                    collection_id,
                    collection_name, 
                    document_id,
                    document_name,
                    model_used,
                    embedding_provider,
                    vector_db,
                    total_vectors,
                    tokens_used,
                    cache,
                    started_at,
                    finished_at
                FROM embedding_reports"#,
        );

        let mut removal_query = sqlx::query_builder::QueryBuilder::<Postgres>::new(
            r#"
                SELECT 
                    id, 
                    'removal' as "ty",
                    collection_id,
                    collection_name, 
                    document_id,
                    document_name,
                    NULL as model_used,
                    NULL as embedding_provider,
                    NULL as vector_db,
                    NULL as total_vectors,
                    NULL as tokens_used,
                    NULL as cache,
                    started_at,
                    finished_at
                FROM embedding_removal_reports
            "#,
        );

        match (params.collection, params.document) {
            (Some(collection_id), Some(document_id)) => {
                query
                    .push(" WHERE collection_id = ")
                    .push_bind(collection_id)
                    .push(" AND document_id = ")
                    .push_bind(document_id);
                removal_query
                    .push(" WHERE collection_id = ")
                    .push_bind(collection_id)
                    .push(" AND document_id = ")
                    .push_bind(document_id);
            }
            (Some(collection_id), None) => {
                query
                    .push(" WHERE collection_id = ")
                    .push_bind(collection_id);
                removal_query
                    .push(" WHERE collection_id = ")
                    .push_bind(collection_id);
            }
            (None, Some(document_id)) => {
                query.push(" WHERE document_id = ").push_bind(document_id);
                removal_query
                    .push(" WHERE document_id = ")
                    .push_bind(document_id);
            }
            (None, None) => {}
        }

        query
            .push(" UNION ")
            .push(removal_query.sql())
            .push(" ORDER BY finished_at DESC")
            .push(" LIMIT ")
            .push_bind(limit)
            .push(" OFFSET ")
            .push_bind(offset);

        Ok(map_err!(
            query.build_query_as().fetch_all(&self.client).await
        ))
    }
}
