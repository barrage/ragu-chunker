use crate::{
    core::{
        model::{
            embedding::{
                Embedding, EmbeddingInsert, EmbeddingRemovalReportInsert, EmbeddingReport,
                EmbeddingReportInsert,
            },
            List, Pagination,
        },
        repo::Repository,
    },
    error::ChonkitError,
    map_err,
};
use uuid::Uuid;

impl Repository {
    pub async fn insert_embeddings(
        &self,
        embeddings: EmbeddingInsert,
    ) -> Result<Embedding, ChonkitError> {
        let EmbeddingInsert {
            id,
            document_id,
            collection_id,
        } = embeddings;

        Ok(map_err!(
            sqlx::query_as!(
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
            )
            .fetch_one(&self.client)
            .await
        ))
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
    ) -> Result<u64, ChonkitError> {
        Ok(map_err!(
            sqlx::query!(
                "DELETE FROM embeddings WHERE document_id = $1 AND collection_id = $2",
                document_id,
                collection_id
            )
            .execute(&self.client)
            .await
        )
        .rows_affected())
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
        report: &EmbeddingReportInsert,
    ) -> Result<(), ChonkitError> {
        map_err!(
            sqlx::query!(
                r#"
                INSERT INTO embedding_reports(
                    collection_id,
                    collection_name,
                    document_id,
                    document_name,
                    model_used,
                    vector_db,
                    total_vectors,
                    tokens_used,
                    started_at,
                    finished_at
                ) 
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            "#,
                report.collection_id,
                report.collection_name,
                report.document_id,
                report.document_name,
                report.model_used,
                report.vector_db,
                report.total_vectors as i32,
                report.tokens_used.map(|t| t as i32),
                report.started_at,
                report.finished_at
            )
            .execute(&self.client)
            .await
        );
        Ok(())
    }

    pub async fn insert_embedding_removal_report(
        &self,
        report: &EmbeddingRemovalReportInsert,
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
        collection_id: Uuid,
    ) -> Result<Vec<EmbeddingReport>, ChonkitError> {
        let rows = map_err!(
            sqlx::query_as!(
                EmbeddingReport,
                r#"
                SELECT 
                    id as "id!", 
                    'addition' as "ty!",
                    collection_id,
                    collection_name as "collection_name!", 
                    document_id,
                    document_name as "document_name!",
                    model_used,
                    embedding_provider,
                    vector_db,
                    total_vectors,
                    tokens_used,
                    started_at as "started_at!",
                    finished_at as "finished_at!"
                FROM embedding_reports
                WHERE collection_id = $1
                UNION
                SELECT 
                    id as "id!", 
                    'removal' as "ty!",
                    collection_id,
                    collection_name as "collection_name!", 
                    document_id,
                    document_name as "document_name!",
                    NULL as model_used,
                    NULL as embedding_provider,
                    NULL as vector_db,
                    NULL as total_vectors,
                    NULL as tokens_used,
                    started_at as "started_at!",
                    finished_at as "finished_at!"
                FROM embedding_removal_reports
                WHERE collection_id = $1
                ORDER BY "finished_at!" DESC
                "#,
                collection_id
            )
            .fetch_all(&self.client)
            .await
        );

        Ok(rows)
    }
}
