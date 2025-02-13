use crate::core::model::document::DocumentParameterUpdate;
use crate::core::repo::Repository;
use crate::error::ChonkitError;
use crate::{
    core::{
        chunk::ChunkConfig,
        document::parser::ParseConfig,
        model::{
            collection::CollectionShort,
            document::{
                config::{DocumentChunkConfig, DocumentParseConfig},
                Document, DocumentConfig, DocumentDisplay, DocumentInsert, DocumentMetadataUpdate,
            },
            List, PaginationSort,
        },
    },
    map_err,
};
use chrono::{DateTime, Utc};
use serde::{de::DeserializeOwned, Serialize};
use sqlx::{types::Json, FromRow, Postgres, Row};
use std::collections::HashMap;
use uuid::Uuid;

use super::Atomic;

impl Repository {
    pub async fn get_document_by_id(
        &self,
        id: uuid::Uuid,
    ) -> Result<Option<Document>, ChonkitError> {
        Ok(map_err!(
            sqlx::query_as!(
                Document,
                "SELECT id, name, path, ext, hash, src, label, tags, created_at, updated_at
             FROM documents 
             WHERE id = $1",
                id
            )
            .fetch_optional(&self.client)
            .await
        ))
    }

    pub async fn get_document_config_by_id(
        &self,
        id: uuid::Uuid,
    ) -> Result<Option<DocumentConfig>, ChonkitError> {
        Ok(map_err!(
            sqlx::query_as!(
                SelectDocumentConfig,
                r#"
                    SELECT 
                        d.id,
                        d.name,
                        d.path,
                        d.ext,
                        d.hash,
                        d.src,
                        c.config AS "chunk_config: Option<Json<ChunkConfig>>",
                        p.config AS "parse_config: _"
                    FROM documents d 
                    LEFT JOIN chunkers c ON c.document_id = d.id
                    LEFT JOIN parsers p ON p.document_id = d.id
                    WHERE d.id = $1
                "#,
                id
            )
            .fetch_optional(&self.client)
            .await
        )
        .map(DocumentConfig::from))
    }

    pub async fn get_document_by_path(
        &self,
        path: &str,
        src: &str,
    ) -> Result<Option<Document>, ChonkitError> {
        Ok(map_err!(
            sqlx::query_as!(
                Document,
                r#"
                    SELECT id, name, path, ext, hash, src, label, tags, created_at, updated_at 
                    FROM documents 
                    WHERE path = $1 AND src = $2
                "#,
                path,
                src
            )
            .fetch_optional(&self.client)
            .await
        ))
    }

    pub async fn get_document_path(&self, id: uuid::Uuid) -> Result<Option<String>, ChonkitError> {
        Ok(map_err!(
            sqlx::query!("SELECT path FROM documents WHERE id = $1", id)
                .fetch_optional(&self.client)
                .await
        )
        .map(|el| el.path))
    }

    pub async fn get_document_by_hash(&self, hash: &str) -> Result<Option<Document>, ChonkitError> {
        Ok(map_err!(
            sqlx::query_as!(
                Document,
                "SELECT id, name, path, ext, hash, src, label, tags, created_at, updated_at 
                 FROM documents 
                 WHERE hash = $1",
                hash
            )
            .fetch_optional(&self.client)
            .await
        ))
    }

    pub async fn get_document_count(&self) -> Result<usize, ChonkitError> {
        Ok(map_err!(
            sqlx::query!("SELECT COUNT(id) FROM documents")
                .fetch_one(&self.client)
                .await
        )
        .count
        .unwrap_or(0) as usize)
    }

    pub async fn list_documents(
        &self,
        params: PaginationSort,
        src: Option<&str>,
        ready: Option<bool>,
    ) -> Result<List<Document>, ChonkitError> {
        let mut query =
            sqlx::query_builder::QueryBuilder::<Postgres>::new("SELECT COUNT(id) FROM documents");

        if let Some(src) = src {
            query.push(" WHERE src = ").push_bind(src);
        }

        let total = map_err!(query
            .build()
            .fetch_one(&self.client)
            .await
            .map(|row| row.get::<i64, usize>(0)));

        let (limit, offset) = params.to_limit_offset();
        let (sort_by, sort_dir) = params.to_sort();

        let mut query = sqlx::query_builder::QueryBuilder::<Postgres>::new(
            r#"
            SELECT 
                documents.id,
                documents.name,
                documents.path,
                documents.ext,
                documents.hash,
                documents.src,
                documents.label,
                documents.tags,
                documents.created_at,
                documents.updated_at
            FROM documents"#,
        );

        match (ready, src) {
            (Some(ready), None) => {
                if ready {
                    query.push(
                        r#"
                        INNER JOIN chunkers ON chunkers.document_id = documents.id
                        INNER JOIN parsers ON parsers.document_id = documents.id
                        "#,
                    );
                } else {
                    query.push(
                        r#"
                        WHERE NOT EXISTS (
                            SELECT 1 FROM chunkers WHERE chunkers.document_id = documents.id
                        )
                        AND NOT EXISTS (
                            SELECT 1 FROM parsers WHERE parsers.document_id = documents.id
                        )
                    "#,
                    );
                }
            }
            (Some(ready), Some(src)) => {
                if ready {
                    query.push(
                        r#"
                        INNER JOIN chunkers ON chunkers.document_id = documents.id
                        INNER JOIN parsers ON parsers.document_id = documents.id
                        "#,
                    );
                    query.push(" WHERE src = ").push_bind(src);
                } else {
                    query.push(" WHERE src = ").push_bind(src).push(
                        r#"
                        AND NOT EXISTS (
                            SELECT 1 FROM chunkers WHERE chunkers.document_id = documents.id
                        )
                        AND NOT EXISTS (
                            SELECT 1 FROM parsers WHERE parsers.document_id = documents.id
                        )
                        "#,
                    );
                }
            }
            (None, Some(src)) => {
                query.push(" WHERE src = ").push_bind(src);
            }
            (None, None) => {}
        }

        query
            .push(format!(" ORDER BY {sort_by} {sort_dir} "))
            .push(" LIMIT ")
            .push_bind(limit)
            .push(" OFFSET ")
            .push_bind(offset);

        let documents: Vec<Document> =
            map_err!(query.build_query_as().fetch_all(&self.client).await);

        Ok(List::new(Some(total as usize), documents))
    }

    pub async fn list_all_document_paths(
        &self,
        src: &str,
    ) -> Result<Vec<(Uuid, String)>, ChonkitError> {
        Ok(map_err!(
            sqlx::query!("SELECT id, path FROM documents WHERE src = $1", src)
                .fetch_all(&self.client)
                .await
        )
        .into_iter()
        .map(|el| (el.id, el.path))
        .collect())
    }

    pub async fn list_all_document_update_times(
        &self,
        src: &str,
    ) -> Result<Vec<(Uuid, String, DateTime<Utc>)>, ChonkitError> {
        Ok(map_err!(
            sqlx::query!(
                "SELECT id, path, updated_at FROM documents WHERE src = $1",
                src
            )
            .fetch_all(&self.client)
            .await
        )
        .into_iter()
        .map(|el| (el.id, el.path, el.updated_at))
        .collect())
    }

    pub async fn update_document_updated_at(
        &self,
        id: Uuid,
        updated_at: DateTime<Utc>,
    ) -> Result<(), ChonkitError> {
        map_err!(
            sqlx::query!(
                "UPDATE documents SET updated_at = $2 WHERE id = $1",
                id,
                updated_at
            )
            .execute(&self.client)
            .await
        );
        Ok(())
    }

    pub async fn list_documents_with_collections(
        &self,
        params: PaginationSort,
        src: Option<&str>,
        document_id: Option<Uuid>,
    ) -> Result<List<DocumentDisplay>, ChonkitError> {
        let mut query =
            sqlx::query_builder::QueryBuilder::<Postgres>::new("SELECT COUNT(id) FROM documents");

        if let Some(src) = src {
            query.push(" WHERE src = ").push_bind(src);
        }

        let total = map_err!(query
            .build()
            .fetch_one(&self.client)
            .await
            .map(|row| row.get::<i64, usize>(0)));

        let (limit, offset) = params.to_limit_offset();
        let (sort_by, sort_dir) = params.to_sort();

        let mut query = sqlx::query_builder::QueryBuilder::<Postgres>::new(
            r#"
                WITH emb AS (SELECT document_id, collection_id FROM embeddings)
                SELECT
                        documents.id,
                        documents.name,
                        documents.path,
                        documents.ext,
                        documents.hash,
                        documents.src,
                        documents.label,
                        documents.tags,
                        documents.created_at,
                        documents.updated_at,
                        collections.id AS collection_id,
                        collections.name AS collection_name,
                        collections.model AS collection_model,
                        collections.embedder AS collection_embedder,
                        collections.provider AS collection_provider
                FROM documents
                LEFT JOIN emb ON emb.document_id = documents.id
                LEFT JOIN collections ON collections.id = emb.collection_id
            "#,
        );

        match (src, document_id) {
            (Some(src), None) => {
                query.push(" WHERE src = ").push_bind(src);
            }
            (None, Some(document_id)) => {
                query.push(" WHERE documents.id = ").push_bind(document_id);
            }
            (Some(src), Some(document_id)) => {
                query
                    .push(" WHERE documents.id = ")
                    .push_bind(document_id)
                    .push(" AND src = ")
                    .push_bind(src);
            }
            (None, None) => (),
        }

        query
            .push(format!(" ORDER BY {sort_by} {sort_dir} "))
            .push(" LIMIT ")
            .push_bind(limit)
            .push(" OFFSET ")
            .push_bind(offset);

        let rows: Vec<DocumentCollectionJoin> =
            map_err!(query.build_query_as().fetch_all(&self.client).await);

        let mut result = HashMap::new();

        for row in rows {
            // Each row is a document entry. If the entry does not have a collection ID,
            // it is not embedded yet and it's guaranteed that it is the only entry with this ID
            if row.collection_id.is_none() {
                result.insert(row.document.id, DocumentDisplay::new(row.document, vec![]));
                continue;
            }

            // Safe to unwrap since the fields are guaranteed to exist by their constraints
            let collection = CollectionShort::new(
                row.collection_id.unwrap(),
                row.collection_name.unwrap(),
                row.collection_model.unwrap(),
                row.collection_embedder.unwrap(),
                row.collection_provider.unwrap(),
            );

            if let Some(doc) = result.get_mut(&row.document.id) {
                doc.collections.push(collection);
            } else {
                result.insert(
                    row.document.id,
                    DocumentDisplay::new(row.document, vec![collection]),
                );
            }
        }

        let documents = result.drain().map(|(_, v)| v).collect();

        Ok(List::new(Some(total as usize), documents))
    }

    pub async fn insert_document(
        &self,
        params: DocumentInsert<'_>,
    ) -> Result<Document, ChonkitError> {
        let DocumentInsert {
            id,
            name,
            path,
            ext,
            src,
            hash,
            label,
            tags,
        } = params;

        Ok(map_err!(
            sqlx::query_as!(
                Document,
                "INSERT INTO documents(id, name, path, ext, hash, src, label, tags)
             VALUES($1, $2, $3, $4, $5, $6, $7, $8)
             RETURNING id, name, path, ext, hash, src, label, tags, created_at, updated_at",
                id,
                name,
                path,
                ext.to_string(),
                hash,
                src,
                label,
                tags.as_deref(),
            )
            .fetch_one(&self.client)
            .await
        ))
    }

    pub async fn update_document_metadata(
        &self,
        id: uuid::Uuid,
        update: DocumentMetadataUpdate<'_>,
    ) -> Result<Document, ChonkitError> {
        let DocumentMetadataUpdate { name, label, tags } = update;

        Ok(map_err!(
            sqlx::query_as!(
                Document,
                r#"
                UPDATE documents SET name = $1, label = $2, tags = $3 WHERE id = $4 
                RETURNING id, name, path, ext, hash, src, label, tags, created_at, updated_at"#,
                name.as_ref(),
                label.as_ref(),
                tags.as_deref(),
                id
            )
            .fetch_one(&self.client)
            .await
        ))
    }

    pub async fn update_document_parameters(
        &self,
        id: uuid::Uuid,
        update: DocumentParameterUpdate<'_>,
    ) -> Result<Document, ChonkitError> {
        let DocumentParameterUpdate { path, hash } = update;

        Ok(map_err!(
            sqlx::query_as!(
                Document,
                r#"
                UPDATE documents SET path = $1, hash = $2 WHERE id = $3
                RETURNING id, name, path, ext, hash, src, label, tags, created_at, updated_at"#,
                path,
                hash,
                id
            )
            .fetch_one(&self.client)
            .await
        ))
    }

    pub async fn remove_document_by_id(
        &self,
        id: uuid::Uuid,
        tx: Option<&mut <Self as Atomic>::Tx>,
    ) -> Result<u64, ChonkitError> {
        let query = sqlx::query!("DELETE FROM documents WHERE id = $1", id);
        if let Some(tx) = tx {
            Ok(map_err!(query.execute(&mut **tx).await).rows_affected())
        } else {
            Ok(map_err!(query.execute(&self.client).await).rows_affected())
        }
    }

    pub async fn remove_document_by_path(&self, path: &str) -> Result<u64, ChonkitError> {
        let result = map_err!(
            sqlx::query!("DELETE FROM documents WHERE path = $1", path)
                .execute(&self.client)
                .await
        );
        Ok(result.rows_affected())
    }

    pub async fn get_document_chunk_config(
        &self,
        id: uuid::Uuid,
    ) -> Result<Option<DocumentChunkConfig>, ChonkitError> {
        Ok(map_err!(
            sqlx::query_as!(
                SelectConfig::<ChunkConfig>,
                r#"SELECT 
                id,
                document_id,
                config AS "config: _",
                created_at,
                updated_at 
             FROM chunkers 
             WHERE document_id = $1"#,
                id
            )
            .fetch_optional(&self.client)
            .await
        )
        .map(DocumentChunkConfig::from))
    }

    pub async fn get_document_parse_config(
        &self,
        id: uuid::Uuid,
    ) -> Result<Option<DocumentParseConfig>, ChonkitError> {
        Ok(map_err!(
            sqlx::query_as!(
                SelectConfig::<ParseConfig>,
                r#"SELECT 
                id,
                document_id,
                config AS "config: _",
                created_at,
                updated_at 
             FROM parsers 
             WHERE document_id = $1"#,
                id
            )
            .fetch_optional(&self.client)
            .await
        )
        .map(DocumentParseConfig::from))
    }

    pub async fn upsert_document_chunk_config(
        &self,
        document_id: uuid::Uuid,
        chunker: ChunkConfig,
    ) -> Result<DocumentChunkConfig, ChonkitError> {
        let config = InsertConfig::new(document_id, chunker);

        let InsertConfig {
            id,
            document_id,
            config,
        } = config;

        let config = map_err!(
            sqlx::query_as!(
                SelectConfig::<ChunkConfig>,
                r#"INSERT INTO chunkers
                (id, document_id, config)
             VALUES
                ($1, $2, $3)
             ON CONFLICT(document_id) DO UPDATE SET config = $3
             RETURNING
                id, document_id, config AS "config: _", created_at, updated_at
            "#,
                id,
                document_id,
                config as Json<ChunkConfig>,
            )
            .fetch_one(&self.client)
            .await
        );

        Ok(DocumentChunkConfig::from(config))
    }

    pub async fn upsert_document_parse_config(
        &self,
        document_id: uuid::Uuid,
        config: ParseConfig,
    ) -> Result<DocumentParseConfig, ChonkitError> {
        let config = InsertConfig::new(document_id, config);

        let InsertConfig {
            id,
            document_id,
            config,
        } = config;

        let config = map_err!(
            sqlx::query_as!(
                SelectConfig::<ParseConfig>,
                r#"INSERT INTO parsers
                (id, document_id, config)
             VALUES
                ($1, $2, $3)
             ON CONFLICT(document_id) DO UPDATE SET config = $3
             RETURNING
                id, document_id, config AS "config: _", created_at, updated_at"#,
                id,
                document_id,
                config as Json<ParseConfig>,
            )
            .fetch_one(&self.client)
            .await
        );

        Ok(DocumentParseConfig::from(config))
    }

    pub async fn insert_document_with_configs(
        &self,
        document: DocumentInsert<'_>,
        parse_config: ParseConfig,
        chunk_config: ChunkConfig,
        tx: &mut <Self as Atomic>::Tx,
    ) -> Result<Document, ChonkitError>
    where
        Self: Atomic,
    {
        let DocumentInsert {
            id,
            name,
            path,
            ext,
            src,
            hash,
            label,
            tags,
        } = document;

        let document = map_err!(
            sqlx::query_as!(
                Document,
                r#"
                INSERT INTO documents(id, name, path, ext, hash, src, label, tags)
                VALUES($1, $2, $3, $4, $5, $6, $7, $8)
                RETURNING id, name, path, ext, hash, src, label, tags, created_at, updated_at
                "#,
                id,
                name,
                path,
                ext.to_string(),
                hash,
                src,
                label,
                tags.as_deref(),
            )
            .fetch_one(&mut **tx)
            .await
        );

        let parse_insert = InsertConfig::new(document.id, parse_config);
        map_err!(
            sqlx::query!(
                r#"
                INSERT INTO parsers (id, document_id, config)
                VALUES ($1, $2, $3)
                ON CONFLICT(document_id) DO UPDATE SET config = $3
                "#,
                parse_insert.id,
                parse_insert.document_id,
                parse_insert.config as Json<ParseConfig>,
            )
            .execute(&mut **tx)
            .await
        );

        let chunk_insert = InsertConfig::new(document.id, chunk_config);
        map_err!(
            sqlx::query!(
                r#"
                INSERT INTO chunkers (id, document_id, config)
                VALUES ($1, $2, $3)
                ON CONFLICT(document_id) DO UPDATE SET config = $3
                "#,
                chunk_insert.id,
                chunk_insert.document_id,
                chunk_insert.config as Json<ChunkConfig>,
            )
            .execute(&mut **tx)
            .await
        );

        Ok(document)
    }

    pub async fn get_document_assigned_collection_names(
        &self,
        document_id: Uuid,
    ) -> Result<Vec<(String, String)>, ChonkitError> {
        let query = sqlx::query!(
            r#"
            SELECT collections.name, collections.provider FROM collections
                WHERE collections.id IN (
                        SELECT collection_id FROM embeddings
                        WHERE embeddings.document_id = $1 
                )
            "#,
            document_id
        );

        let results = map_err!(query.fetch_all(&self.client).await);

        Ok(results
            .into_iter()
            .map(|record| (record.name, record.provider))
            .collect())
    }
}

// Private dtos.

#[derive(Debug, FromRow)]
struct DocumentCollectionJoin {
    #[sqlx(flatten)]
    document: Document,
    // Collection params optional since the document may not be in a collection
    collection_id: Option<Uuid>,
    collection_name: Option<String>,
    collection_embedder: Option<String>,
    collection_model: Option<String>,
    collection_provider: Option<String>,
}

struct InsertConfig<T: Serialize> {
    id: uuid::Uuid,
    document_id: uuid::Uuid,
    config: sqlx::types::Json<T>,
}

impl<T> InsertConfig<T>
where
    T: Serialize,
{
    fn new(document_id: uuid::Uuid, config: T) -> Self {
        Self {
            id: uuid::Uuid::new_v4(),
            document_id,
            config: Json(config),
        }
    }
}

struct SelectDocumentConfig {
    id: uuid::Uuid,
    name: String,
    path: String,
    ext: String,
    hash: String,
    src: String,
    chunk_config: Option<Json<ChunkConfig>>,
    parse_config: Option<Json<ParseConfig>>,
}

impl From<SelectDocumentConfig> for DocumentConfig {
    fn from(
        SelectDocumentConfig {
            id,
            name,
            path,
            ext,
            hash,
            src,
            chunk_config,
            parse_config,
        }: SelectDocumentConfig,
    ) -> Self {
        Self {
            id,
            name,
            path,
            ext,
            hash,
            src,
            chunk_config: chunk_config.map(|c| c.0),
            parse_config: parse_config.map(|c| c.0),
        }
    }
}

struct SelectConfig<T: DeserializeOwned> {
    id: uuid::Uuid,
    document_id: uuid::Uuid,
    config: sqlx::types::Json<T>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl From<SelectConfig<ChunkConfig>> for DocumentChunkConfig {
    fn from(value: SelectConfig<ChunkConfig>) -> Self {
        let SelectConfig {
            id,
            document_id,
            config,
            created_at,
            updated_at,
        } = value;
        Self {
            id,
            document_id,
            config: config.0,
            created_at,
            updated_at,
        }
    }
}

impl From<SelectConfig<ParseConfig>> for DocumentParseConfig {
    fn from(value: SelectConfig<ParseConfig>) -> Self {
        let SelectConfig {
            id,
            document_id,
            config,
            created_at,
            updated_at,
        } = value;
        Self {
            id,
            document_id,
            config: config.0,
            created_at,
            updated_at,
        }
    }
}

#[cfg(test)]
#[suitest::suite(pg_document_repo_int)]
mod tests {

    use crate::{
        app::test::{init_repository, PostgresContainer},
        core::{
            chunk::ChunkConfig,
            document::{DocumentType, TextDocumentType},
            model::document::DocumentInsert,
            repo::Repository,
        },
    };
    use suitest::before_all;

    #[before_all]
    async fn setup() -> (Repository, PostgresContainer) {
        let (repository, pg_img) = init_repository().await;
        (repository, pg_img)
    }

    #[test]
    async fn inserting_document_works(repo: Repository) {
        let doc = DocumentInsert::new(
            "My file",
            "path/to/file",
            DocumentType::Text(TextDocumentType::Txt),
            "SHA256",
            "fs",
        );
        let doc = repo.insert_document(doc).await.unwrap();
        let doc = repo.get_document_by_id(doc.id).await.unwrap().unwrap();

        assert_eq!("My file", doc.name);
        assert_eq!("path/to/file", doc.path);
        assert_eq!("txt", doc.ext);

        repo.remove_document_by_id(doc.id, None).await.unwrap();

        let doc = repo.get_document_by_id(doc.id).await.unwrap();

        assert!(doc.is_none());
    }

    #[test]
    async fn inserting_chunk_config_works(repo: Repository) {
        let doc = DocumentInsert::new(
            "My file",
            "path/to/file/2",
            DocumentType::Text(TextDocumentType::Txt),
            "Other hash",
            "fs",
        );
        let doc = repo.insert_document(doc).await.unwrap();
        let chunker = ChunkConfig::sliding(420, 69).unwrap();
        repo.upsert_document_chunk_config(doc.id, chunker.clone())
            .await
            .unwrap();
        let config = repo
            .get_document_chunk_config(doc.id)
            .await
            .unwrap()
            .unwrap();
        let ChunkConfig::Sliding(sliding) = config.config else {
            panic!("invalid config variant");
        };
        let ChunkConfig::Sliding(chunker) = chunker else {
            panic!("the impossible happened");
        };
        assert_eq!(chunker.size, sliding.size);
        assert_eq!(chunker.overlap, sliding.overlap);
        repo.remove_document_by_id(doc.id, None).await.unwrap();
    }
}
