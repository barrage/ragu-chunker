use crate::{
    core::{
        model::{
            collection::{Collection, CollectionDisplay, CollectionInsert},
            document::DocumentShort,
            List, PaginationSort,
        },
        repo::{Atomic, Repository},
    },
    err,
    error::ChonkitError,
    map_err,
};
use chrono::{DateTime, Utc};
use sqlx::{prelude::FromRow, Postgres};
use std::collections::HashMap;
use uuid::Uuid;

impl Repository {
    pub async fn list_collections(
        &self,
        params: PaginationSort,
    ) -> Result<List<Collection>, ChonkitError> {
        let mut count =
            sqlx::query_builder::QueryBuilder::<Postgres>::new("SELECT COUNT(*) FROM collections");

        let (limit, offset) = params.to_limit_offset();
        let (sort_by, sort_dir) = params.to_sort();

        let mut query = sqlx::query_builder::QueryBuilder::<Postgres>::new(
            "SELECT id, name, model, embedder, provider, created_at, updated_at FROM collections",
        );

        if let Some(ref search) = params.search {
            let q = format!("%{}%", search.q);
            count
                .push(" WHERE ")
                .push(&search.column)
                .push(" ILIKE ")
                .push_bind(q.clone());

            query
                .push(" WHERE ")
                .push(&search.column)
                .push(" ILIKE ")
                .push_bind(q);
        }

        query
            .push(format!(" ORDER BY {sort_by} {sort_dir} "))
            .push(" LIMIT ")
            .push_bind(limit)
            .push(" OFFSET ")
            .push_bind(offset);

        let total: i64 = map_err!(count.build_query_scalar().fetch_one(&self.client).await);

        let collections: Vec<Collection> =
            map_err!(query.build_query_as().fetch_all(&self.client).await)
                .into_iter()
                .collect();

        Ok(List::new(Some(total as usize), collections))
    }

    pub async fn list_collections_display(
        &self,
        p: PaginationSort,
    ) -> Result<List<CollectionDisplay>, ChonkitError> {
        let (limit, offset) = p.to_limit_offset();
        let (sort_by, sort_dir) = p.to_sort();

        let total = map_err!(sqlx::query!("SELECT COUNT(id) FROM collections")
            .fetch_one(&self.client)
            .await
            .map(|row| row.count));

        let mut query = sqlx::query_builder::QueryBuilder::<Postgres>::new(
            r#"
                WITH docs AS (
                        SELECT
                                embeddings.collection_id,
                                documents.id AS document_id,
                                documents.name AS document_name
                        FROM documents 
                        RIGHT JOIN embeddings ON documents.id = embeddings.document_id
                ),
                counts AS (
                        SELECT
                                embeddings.collection_id,
                                COUNT(DISTINCT embeddings.document_id) AS count
                        FROM embeddings
                        GROUP BY embeddings.collection_id
                ),
                cols AS (
                        SELECT 
                                collections.id,
                                collections.name,
                                collections.model,
                                collections.embedder,
                                collections.provider,
                                collections.created_at,
                                collections.updated_at
                        FROM collections
        "#,
        );
        query
            .push(" LIMIT ")
            .push_bind(limit)
            .push(" OFFSET ")
            .push_bind(offset)
            .push(")")
            .push(
                r#"
                SELECT
                        cols.id,
                        cols.name,
                        cols.model,
                        cols.embedder,
                        cols.provider,
                        cols.created_at,
                        cols.updated_at,
                        docs.document_id,
                        docs.document_name,
                        COALESCE(counts.count, 0) AS "document_count"
                FROM cols
                LEFT JOIN docs ON cols.id = docs.collection_id
                LEFT JOIN counts ON cols.id = counts.collection_id
            "#,
            );

        query.push(format!(" ORDER BY {sort_by} {sort_dir} "));

        let collections: Vec<CollectionDocumentJoin> =
            map_err!(query.build_query_as().fetch_all(&self.client).await);

        let mut result: HashMap<Uuid, CollectionDisplay> = HashMap::new();

        for collection_row in collections {
            let collection = Collection {
                id: collection_row.id,
                name: collection_row.name,
                model: collection_row.model,
                embedder: collection_row.embedder,
                provider: collection_row.provider,
                created_at: collection_row.created_at,
                updated_at: collection_row.updated_at,
            };

            if let Some(collection) = result.get_mut(&collection.id) {
                let (Some(document_id), Some(document_name)) =
                    (collection_row.document_id, collection_row.document_name)
                else {
                    continue;
                };
                let document = DocumentShort::new(document_id, document_name);
                collection.documents.push(document);
            } else {
                let mut collection = CollectionDisplay::new(
                    collection,
                    collection_row.document_count as usize,
                    vec![],
                );

                if let Some(document_id) = collection_row.document_id {
                    let document =
                        DocumentShort::new(document_id, collection_row.document_name.unwrap());
                    collection.documents.push(document);
                }

                result.insert(collection.collection.id, collection);
            }
        }

        Ok(List::new(
            total.map(|total| total as usize),
            result.drain().map(|(_, v)| v).collect(),
        ))
    }

    pub async fn insert_collection(
        &self,
        insert: CollectionInsert<'_>,
        tx: Option<&mut <Repository as Atomic>::Tx>,
    ) -> Result<Collection, ChonkitError> {
        let CollectionInsert {
            id,
            name,
            model,
            embedder,
            provider,
        } = insert;

        let query = sqlx::query_as!(
            Collection,
            "INSERT INTO collections
                (id, name, model, embedder, provider)
             VALUES
                ($1, $2, $3, $4, $5)
             RETURNING 
                id, name, model, embedder, provider, created_at, updated_at
             ",
            id,
            name,
            model,
            embedder,
            provider
        );

        let collection = if let Some(tx) = tx {
            query.fetch_one(&mut **tx).await
        } else {
            query.fetch_one(&self.client).await
        };

        match collection {
            Ok(collection) => Ok(collection),
            Err(sqlx::Error::Database(e)) if e.code().is_some_and(|code| code == "23505") => {
                err!(AlreadyExists, "Collection '{name}' already exists")
            }
            Err(e) => map_err!(Err(e)),
        }
    }

    pub async fn delete_collection(&self, id: Uuid) -> Result<u64, ChonkitError> {
        let result = map_err!(
            sqlx::query!("DELETE FROM collections WHERE id = $1", id)
                .execute(&self.client)
                .await
        );
        Ok(result.rows_affected())
    }

    pub async fn get_collection_by_id(&self, id: Uuid) -> Result<Option<Collection>, ChonkitError> {
        Ok(map_err!(sqlx::query_as!(
            Collection,
            "SELECT id, name, model, embedder, provider, created_at, updated_at FROM collections WHERE id = $1",
            id
        )
        .fetch_optional(&self.client)
        .await))
    }

    pub async fn get_collection_display(
        &self,
        collection_id: Uuid,
    ) -> Result<Option<CollectionDisplay>, ChonkitError> {
        let collection = map_err!(sqlx::query_as!(
            Collection,
            "SELECT id, name, model, embedder, provider, created_at, updated_at FROM collections WHERE id = $1",
            collection_id
        )
        .fetch_optional(&self.client)
        .await);

        let Some(collection) = collection else {
            return Ok(None);
        };

        let documents = map_err!(sqlx::query_as!(
            DocumentShort,
            r#"
                WITH embeddings AS (SELECT document_id FROM embeddings WHERE collection_id = $1) 
                SELECT documents.id, documents.name FROM documents RIGHT JOIN embeddings ON documents.id = embeddings.document_id
            "#,
            collection_id
        )
        .fetch_all(&self.client)
        .await);

        Ok(Some(CollectionDisplay::new(
            collection,
            documents.len(),
            documents,
        )))
    }

    pub async fn get_collection_by_name(
        &self,
        name: &str,
        provider: &str,
    ) -> Result<Option<Collection>, ChonkitError> {
        Ok(map_err!(sqlx::query_as!(
            Collection,
            "SELECT id, name, model, embedder, provider, created_at, updated_at FROM collections WHERE name = $1 AND provider = $2",
            name,
            provider
        )
        .fetch_optional(&self.client)
        .await))
    }
}

/// Private DTO for joining collections and the documents they contain.
#[derive(Debug, FromRow)]
struct CollectionDocumentJoin {
    id: Uuid,
    name: String,
    model: String,
    embedder: String,
    provider: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    document_id: Option<Uuid>,
    document_name: Option<String>,
    document_count: i64,
}
