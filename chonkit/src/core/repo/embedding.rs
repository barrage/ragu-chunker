use crate::{
    core::{
        model::{
            embedding::{
                EmbeddingReport, ImageEmbedding, ImageEmbeddingAdditionReport,
                ImageEmbeddingInsert, ImageEmbeddingRemovalReport, TextEmbedding,
                TextEmbeddingAdditionReport, TextEmbeddingInsert, TextEmbeddingRemovalReport,
            },
            List, Pagination,
        },
        repo::{Repository, Transaction},
        service::embedding::ListEmbeddingReportsParams,
    },
    error::ChonkitError,
    map_err,
};
use sqlx::Postgres;
use uuid::Uuid;

impl Repository {
    pub async fn insert_text_embeddings(
        &self,
        embeddings: TextEmbeddingInsert,
        tx: Option<&mut Transaction<'_>>,
    ) -> Result<TextEmbedding, ChonkitError> {
        let TextEmbeddingInsert {
            id,
            document_id,
            collection_id,
        } = embeddings;

        let query = sqlx::query_as!(
            TextEmbedding,
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

    pub async fn insert_image_embeddings(
        &self,
        embeddings: ImageEmbeddingInsert,
        tx: Option<&mut Transaction<'_>>,
    ) -> Result<(), ChonkitError> {
        let ImageEmbeddingInsert {
            id,
            image_id,
            collection_id,
        } = embeddings;

        let query = sqlx::query!(
            r#"
                INSERT INTO image_embeddings(id, image_id, collection_id)
                VALUES ($1, $2, $3)
                ON CONFLICT(id) DO UPDATE
                SET id = $1
            "#,
            id,
            image_id,
            collection_id,
        );

        match tx {
            Some(tx) => map_err!(query.execute(&mut **tx).await),
            None => map_err!(query.execute(&self.client).await),
        };

        Ok(())
    }

    pub async fn get_all_text_embeddings(
        &self,
        document_id: Uuid,
    ) -> Result<Vec<TextEmbedding>, ChonkitError> {
        Ok(map_err!(
            sqlx::query_as!(
                TextEmbedding,
                "SELECT id, document_id, collection_id, created_at, updated_at 
                 FROM embeddings
                 WHERE document_id = $1
                 ORDER BY created_at DESC
                ",
                document_id
            )
            .fetch_all(&self.client)
            .await
        ))
    }

    pub async fn get_text_embeddings(
        &self,
        document_id: Uuid,
        collection_id: Uuid,
    ) -> Result<Option<TextEmbedding>, ChonkitError> {
        Ok(map_err!(
            sqlx::query_as!(
                TextEmbedding,
                "SELECT id, document_id, collection_id, created_at, updated_at 
                 FROM embeddings
                 WHERE document_id = $1 AND collection_id = $2
                ",
                document_id,
                collection_id
            )
            .fetch_optional(&self.client)
            .await
        ))
    }

    pub async fn get_image_embeddings(
        &self,
        image_id: Uuid,
        collection_id: Uuid,
    ) -> Result<Option<ImageEmbedding>, ChonkitError> {
        Ok(map_err!(
            sqlx::query_as!(
                ImageEmbedding,
                "SELECT id, image_id, collection_id, created_at
                 FROM image_embeddings
                 WHERE image_id = $1 AND collection_id = $2
                ",
                image_id,
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
    ) -> Result<List<TextEmbedding>, ChonkitError> {
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
                TextEmbedding,
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

    pub async fn get_text_embeddings_by_name(
        &self,
        document_id: Uuid,
        collection_name: &str,
        provider: &str,
    ) -> Result<Option<TextEmbedding>, ChonkitError> {
        Ok(map_err!(sqlx::query_as!(
            TextEmbedding,
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

    pub async fn delete_text_embeddings(
        &self,
        document_id: Uuid,
        collection_id: Uuid,
        tx: Option<&mut Transaction<'_>>,
    ) -> Result<u64, ChonkitError> {
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

    pub async fn delete_image_embeddings(
        &self,
        image_id: Uuid,
        collection_id: Uuid,
        tx: Option<&mut Transaction<'_>>,
    ) -> Result<u64, ChonkitError> {
        let query = sqlx::query!(
            "DELETE FROM image_embeddings WHERE image_id = $1 AND collection_id = $2",
            image_id,
            collection_id
        );
        match tx {
            Some(tx) => Ok(map_err!(query.execute(&mut **tx).await).rows_affected()),
            None => Ok(map_err!(query.execute(&self.client).await).rows_affected()),
        }
    }

    pub async fn list_outdated_embeddings(
        &self,
        collection_id: Uuid,
    ) -> Result<Vec<TextEmbedding>, ChonkitError> {
        Ok(map_err!(
            sqlx::query_as!(
                TextEmbedding,
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

    pub async fn insert_image_embedding_report(
        &self,
        report: &ImageEmbeddingAdditionReport,
    ) -> Result<(), ChonkitError> {
        map_err!(
            sqlx::query!(
                r#"
                INSERT INTO embedding_reports(
                    collection_id,
                    collection_name,
                    embedding_provider,
                    model_used,
                    vector_db,
                    total_vectors,
                    tokens_used,
                    cache,
                    started_at,
                    finished_at,
                    image_id,
                    type
                ) 
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, 'image')
            "#,
                report.report.base.collection_id,
                report.report.base.collection_name,
                report.report.embedding_provider,
                report.report.model_used,
                report.report.base.vector_db,
                report.report.total_vectors,
                report.report.tokens_used.map(|t| t as i32),
                report.report.cache,
                report.report.base.started_at,
                report.report.base.finished_at,
                report.image_id
            )
            .execute(&self.client)
            .await
        );

        Ok(())
    }

    pub async fn insert_image_embedding_removal_report(
        &self,
        report: &ImageEmbeddingRemovalReport,
    ) -> Result<(), ChonkitError> {
        map_err!(
            sqlx::query!(
                r#"
                INSERT INTO embedding_removal_reports(
                    collection_id,
                    collection_name,
                    started_at,
                    finished_at,
                    image_id,
                    vector_db,
                    type
                ) 
                VALUES ($1, $2, $3, $4, $5, $6, 'image')
            "#,
                report.report.collection_id,
                report.report.collection_name,
                report.report.started_at,
                report.report.finished_at,
                report.image_id,
                report.report.vector_db
            )
            .execute(&self.client)
            .await
        );

        Ok(())
    }

    pub async fn insert_text_embedding_report(
        &self,
        report: &TextEmbeddingAdditionReport,
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
                    finished_at,
                    type
                ) 
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, 'text')
            "#,
                report.report.base.collection_id,
                report.report.base.collection_name,
                report.document_id,
                report.document_name,
                report.report.embedding_provider,
                report.report.model_used,
                report.report.base.vector_db,
                report.report.total_vectors,
                report.report.tokens_used.map(|t| t as i32),
                report.report.cache,
                report.report.base.started_at,
                report.report.base.finished_at,
            )
            .execute(&self.client)
            .await
        );
        Ok(())
    }

    pub async fn insert_text_embedding_removal_report(
        &self,
        report: &TextEmbeddingRemovalReport,
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
                finished_at,
                vector_db,
                type
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, 'text')
            "#,
                report.document_id,
                report.document_name,
                report.report.collection_id,
                report.report.collection_name,
                report.report.started_at,
                report.report.finished_at,
                report.report.vector_db
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
                    'addition' as "report_type",
                    id, 
                    type,
                    collection_id,
                    collection_name, 
                    vector_db,

                    model_used,
                    embedding_provider,
                    total_vectors,
                    tokens_used,
                    cache,

                    started_at,
                    finished_at,

                    -- Document fields
                    document_id,
                    document_name,

                    -- Image fields
                    image_id
                FROM embedding_reports"#,
        );

        let mut removal_query = sqlx::query_builder::QueryBuilder::<Postgres>::new(
            r#"
                SELECT 
                    'removal' as "report_type",
                    id, 
                    type,
                    collection_id,
                    collection_name, 
                    vector_db,

                    -- Fields only applicable to addition
                    NULL as model_used,
                    NULL as embedding_provider,
                    NULL as total_vectors,
                    NULL as tokens_used,
                    NULL as cache,

                    started_at,
                    finished_at,

                    -- Document fields
                    document_id,
                    document_name,

                    -- Image fields
                    image_id

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
